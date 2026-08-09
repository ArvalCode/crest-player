use std::{
    collections::VecDeque,
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

pub struct VideoFrame {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
    presentation_time: Duration,
}

pub struct VideoScreensaver {
    receiver: Option<Receiver<VideoFrame>>,
    stop: Option<Arc<AtomicBool>>,
    key: Option<(String, u16, u16, u16, bool)>,
    latest: Option<VideoFrame>,
    buffer: VecDeque<VideoFrame>,
    buffer_target: usize,
    buffer_limit: usize,
    buffering: bool,
    frame_interval: Duration,
    next_frame_at: Option<Instant>,
    frame_serial: u64,
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
            buffer: VecDeque::new(),
            buffer_target: 1,
            buffer_limit: 1,
            buffering: true,
            frame_interval: Duration::from_millis(66),
            next_frame_at: None,
            frame_serial: 0,
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

        let mut disconnected = false;
        if let Some(receiver) = &self.receiver {
            while self.buffer.len() < self.buffer_limit {
                match receiver.try_recv() {
                    Ok(frame) => self.buffer.push_back(frame),
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

        if self.buffering {
            if self.buffer.len() < self.buffer_target {
                return;
            }
            self.buffering = false;
            self.next_frame_at = None;
        }

        let now = Instant::now();
        let frames_due = match self.next_frame_at {
            None => 1,
            Some(deadline) if now >= deadline => {
                1 + (now.duration_since(deadline).as_nanos() / self.frame_interval.as_nanos())
                    as usize
            }
            Some(_) => 0,
        };
        if frames_due == 0 {
            return;
        }
        let mut frames_to_take = 0;
        while self
            .buffer
            .front()
            .is_some_and(|frame| frame.presentation_time <= position)
        {
            self.latest = self.buffer.pop_front();
            self.frame_serial = self.frame_serial.wrapping_add(1);
            frames_to_take += 1;
        }
        if self.buffer.is_empty() && frames_to_take == 0 {
            self.buffering = true;
            self.next_frame_at = None;
        } else {
            self.next_frame_at = Some(match self.next_frame_at {
                Some(deadline) => {
                    deadline + self.frame_interval * u32::try_from(frames_due).unwrap_or(u32::MAX)
                }
                None => now + self.frame_interval,
            });
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
        let buffer_target = fps as usize;
        let buffer_limit = buffer_target.saturating_mul(3);
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
            let frame_size = width as usize * height as usize * 3;
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
                    .send(VideoFrame {
                        width,
                        height,
                        pixels,
                        presentation_time: synchronized_position,
                    })
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
                        .send(VideoFrame {
                            width,
                            height,
                            pixels,
                            presentation_time: synchronized_position
                                + Duration::from_secs_f64(frame_index as f64 / f64::from(fps)),
                        })
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
        self.buffer_target = buffer_target;
        self.buffer_limit = buffer_limit;
        self.buffering = true;
        self.frame_interval = Duration::from_secs_f64(1.0 / f64::from(fps));
        self.next_frame_at = None;
    }

    fn stop(&mut self) {
        self.suspend();
        self.latest = None;
    }

    fn suspend(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.receiver = None;
        self.key = None;
        self.buffer.clear();
        self.buffering = true;
        self.next_frame_at = None;
    }
}

impl Drop for VideoScreensaver {
    fn drop(&mut self) {
        self.stop();
    }
}
