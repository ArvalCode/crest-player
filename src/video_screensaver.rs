use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_BUFFER_BYTES: usize = 32 * 1024 * 1024;
const MAX_BUFFER_SECONDS: usize = 2;

#[derive(Clone)]
pub struct VideoFrame {
    pub width: u16,
    pub height: u16,
    pub pixels: Arc<[u8]>,
    presentation_time: Duration,
    signature: u64,
}

impl VideoFrame {
    pub fn from_rgb(width: u16, height: u16, pixels: Vec<u8>) -> Option<Self> {
        Self::new(width, height, pixels, Duration::ZERO)
    }

    fn new(width: u16, height: u16, pixels: Vec<u8>, presentation_time: Duration) -> Option<Self> {
        let expected = width as usize * height as usize * 3;
        if width == 0 || height == 0 || pixels.len() != expected {
            return None;
        }
        let signature = rough_frame_signature(&pixels);
        Some(Self {
            width,
            height,
            pixels: pixels.into(),
            presentation_time,
            signature,
        })
    }
}

fn rough_frame_signature(pixels: &[u8]) -> u64 {
    pixels.chunks_exact(3).step_by(64).fold(0, |hash, pixel| {
        hash.wrapping_mul(31)
            .wrapping_add(u64::from(pixel[0]))
            .wrapping_add(u64::from(pixel[1]))
            .wrapping_add(u64::from(pixel[2]))
    })
}

fn buffer_limit(width: u16, height: u16, fps: u16) -> usize {
    let frame_size = width as usize * height as usize * 3;
    let frames_by_memory = MAX_BUFFER_BYTES / frame_size.max(1);
    let frames_by_time = fps as usize * MAX_BUFFER_SECONDS;
    frames_by_memory.min(frames_by_time).max(1)
}

pub struct VideoScreensaver {
    receiver: Option<Receiver<VideoFrame>>,
    stop: Option<Arc<AtomicBool>>,
    key: Option<(String, u16, u16, u16, bool)>,
    latest: Option<VideoFrame>,
    pending: Option<VideoFrame>,
    frame_serial: u64,
    latest_signature: Option<u64>,
}

struct DecodePlan {
    fps: u16,
    hardware_acceleration: bool,
}

impl VideoScreensaver {
    pub fn new() -> Self {
        Self {
            receiver: None,
            stop: None,
            key: None,
            latest: None,
            pending: None,
            frame_serial: 0,
            latest_signature: None,
        }
    }

    pub fn update(
        &mut self,
        visible: bool,
        source: Option<String>,
        position: Duration,
        width: u16,
        cell_height: u16,
        settings: (u16, (u16, u16), bool, bool),
    ) {
        if !visible || source.is_none() || width == 0 || cell_height == 0 {
            self.stop();
            return;
        }
        let (fps, samples_per_cell, playback_running, hardware_acceleration) = settings;
        if !playback_running {
            self.suspend();
            return;
        }
        let source = source.unwrap();
        let pixel_width = width.saturating_mul(samples_per_cell.0);
        let pixel_height = cell_height.saturating_mul(samples_per_cell.1);
        let fps = match fps {
            30 | 60 => fps,
            _ => 15,
        };
        let key = (
            source.clone(),
            pixel_width,
            pixel_height,
            fps,
            hardware_acceleration,
        );
        if self.key.as_ref() != Some(&key) {
            self.stop();
            self.start(
                source,
                position,
                pixel_width,
                pixel_height,
                DecodePlan {
                    fps,
                    hardware_acceleration,
                },
            );
        }

        self.present_due_frame(position);
    }

    fn present_due_frame(&mut self, position: Duration) {
        let mut current = None;
        if let Some(frame) = self.pending.take() {
            if frame.presentation_time <= position {
                current = Some(frame);
            } else {
                self.pending = Some(frame);
            }
        }

        let mut disconnected = false;
        if self.pending.is_none()
            && let Some(receiver) = &self.receiver
        {
            loop {
                match receiver.try_recv() {
                    Ok(frame) if frame.presentation_time <= position => current = Some(frame),
                    Ok(frame) => {
                        self.pending = Some(frame);
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected {
            self.receiver = None;
        }
        if let Some(frame) = current {
            if self.latest_signature != Some(frame.signature) {
                self.frame_serial = self.frame_serial.wrapping_add(1);
                self.latest_signature = Some(frame.signature);
            }
            self.latest = Some(frame);
        }
    }

    pub fn frame(&self) -> Option<&VideoFrame> {
        self.latest.as_ref()
    }

    pub fn frame_serial(&self) -> u64 {
        self.frame_serial
    }

    pub fn restart(&mut self) {
        self.stop();
    }

    fn start(
        &mut self,
        source: String,
        position: Duration,
        width: u16,
        height: u16,
        plan: DecodePlan,
    ) {
        let DecodePlan {
            fps,
            hardware_acceleration,
        } = plan;
        let frame_size = width as usize * height as usize * 3;
        let buffer_limit = buffer_limit(width, height, fps);
        // The bounded channel is the only frame queue. The consumer retains at
        // most one future frame while dropping obsolete frames against audio time.
        let (sender, receiver) = mpsc::sync_channel(buffer_limit);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_source = source.clone();
        thread::spawn(move || {
            let resolution_started = Instant::now();
            let direct_url = if std::path::Path::new(&worker_source).is_file() {
                worker_source.clone()
            } else {
                let resolved = Command::new("yt-dlp")
                    .args([
                        "--no-playlist",
                        "-g",
                        "-f",
                        "bestvideo[height<=720]/bestvideo/best[height<=720]/best",
                        &worker_source,
                    ])
                    .stdin(Stdio::null())
                    .stderr(Stdio::null())
                    .output();
                if worker_stop.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(output) = resolved else { return };
                if !output.status.success() {
                    return;
                }
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            };
            if direct_url.is_empty() {
                return;
            }

            let filter = format!(
                "fps={fps},scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:black"
            );
            // Account for URL resolution time so video starts at the audio clock's
            // current position rather than where it was when the worker spawned.
            let synchronized_position = position + resolution_started.elapsed();
            let attempts: &[bool] = if hardware_acceleration {
                &[true, false]
            } else {
                &[false]
            };
            for &accelerated in attempts {
                let seek = format!("{:.3}", synchronized_position.as_secs_f64());
                let mut command = Command::new("ffmpeg");
                command.args(["-loglevel", "error", "-ss", &seek]);
                if accelerated {
                    command.args(["-hwaccel", "auto"]);
                }
                command.args([
                    "-i",
                    &direct_url,
                    "-an",
                    "-vf",
                    &filter,
                    "-pix_fmt",
                    "rgb24",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ]);
                let Ok(mut child) = command
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                else {
                    continue;
                };
                let Some(mut stdout) = child.stdout.take() else {
                    let _ = child.kill();
                    let _ = child.wait();
                    continue;
                };
                let mut pixels = vec![0; frame_size];
                if stdout.read_exact(&mut pixels).is_err() {
                    let _ = child.kill();
                    let _ = child.wait();
                    continue;
                }
                let mut frame_index = 0u64;
                if sender
                    .send(VideoFrame::new(width, height, pixels, synchronized_position).unwrap())
                    .is_err()
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                frame_index = frame_index.saturating_add(1);
                while !worker_stop.load(Ordering::Relaxed) {
                    let mut pixels = vec![0; frame_size];
                    if stdout.read_exact(&mut pixels).is_err() {
                        break;
                    }
                    if sender
                        .send(
                            VideoFrame::new(
                                width,
                                height,
                                pixels,
                                synchronized_position
                                    + Duration::from_secs_f64(frame_index as f64 / f64::from(fps)),
                            )
                            .unwrap(),
                        )
                        .is_err()
                    {
                        break;
                    }
                    frame_index = frame_index.saturating_add(1);
                }
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        });
        self.receiver = Some(receiver);
        self.stop = Some(stop);
        self.key = Some((source, width, height, fps, hardware_acceleration));
        self.pending = None;
    }

    fn stop(&mut self) {
        self.suspend();
        self.latest = None;
        self.latest_signature = None;
    }

    fn suspend(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.receiver = None;
        self.key = None;
        self.pending = None;
    }
}

impl Drop for VideoScreensaver {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_BUFFER_BYTES, VideoFrame, buffer_limit};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    #[test]
    fn buffer_is_capped_by_time_for_small_frames() {
        assert_eq!(buffer_limit(100, 100, 60), 120);
    }

    #[test]
    fn buffer_is_capped_by_memory_for_large_frames() {
        let width = 1_000;
        let height = 1_000;
        let limit = buffer_limit(width, height, 60);
        assert!(limit * width as usize * height as usize * 3 <= MAX_BUFFER_BYTES);
    }

    #[test]
    fn cloning_a_frame_shares_pixel_storage() {
        let frame = VideoFrame::from_rgb(1, 1, vec![1, 2, 3]).unwrap();
        let clone = frame.clone();
        assert!(Arc::ptr_eq(&frame.pixels, &clone.pixels));
    }

    #[test]
    fn scheduler_drops_old_frames_and_holds_the_next_future_frame() {
        let (sender, receiver) = mpsc::sync_channel(3);
        for (millis, value) in [(100, 1), (200, 2), (300, 3)] {
            sender
                .send(
                    VideoFrame::new(
                        1,
                        1,
                        vec![value, value, value],
                        Duration::from_millis(millis),
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let mut screensaver = super::VideoScreensaver::new();
        screensaver.receiver = Some(receiver);

        screensaver.present_due_frame(Duration::from_millis(250));
        assert_eq!(screensaver.frame().unwrap().pixels[0], 2);
        assert_eq!(screensaver.pending.as_ref().unwrap().pixels[0], 3);

        screensaver.present_due_frame(Duration::from_millis(350));
        assert_eq!(screensaver.frame().unwrap().pixels[0], 3);
        assert!(screensaver.pending.is_none());
    }
}
