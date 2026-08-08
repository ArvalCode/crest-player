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

pub struct VideoFrame {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

pub struct VideoScreensaver {
    receiver: Option<Receiver<VideoFrame>>,
    stop: Option<Arc<AtomicBool>>,
    key: Option<(String, u16, u16)>,
    latest: Option<VideoFrame>,
}

impl VideoScreensaver {
    pub fn new() -> Self {
        Self {
            receiver: None,
            stop: None,
            key: None,
            latest: None,
        }
    }

    pub fn update(
        &mut self,
        visible: bool,
        source: Option<String>,
        position: Duration,
        width: u16,
        cell_height: u16,
    ) {
        if !visible || source.is_none() || width == 0 || cell_height == 0 {
            self.stop();
            return;
        }
        let source = source.unwrap();
        let pixel_height = cell_height.saturating_mul(2);
        let key = (source.clone(), width, pixel_height);
        if self.key.as_ref() != Some(&key) {
            self.stop();
            self.start(source, position, width, pixel_height);
        }

        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(frame) => self.latest = Some(frame),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.receiver = None;
                        break;
                    }
                }
            }
        }
    }

    pub fn frame(&self) -> Option<&VideoFrame> {
        self.latest.as_ref()
    }

    fn start(&mut self, source: String, position: Duration, width: u16, height: u16) {
        let (sender, receiver) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_source = source.clone();
        thread::spawn(move || {
            let resolution_started = Instant::now();
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
            let direct_url = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            if direct_url.is_empty() {
                return;
            }

            let filter = format!(
                "fps=15,scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:black"
            );
            // Account for URL resolution time so video starts at the audio clock's
            // current position rather than where it was when the worker spawned.
            let synchronized_position = position + resolution_started.elapsed();
            let mut child = match Command::new("ffmpeg")
                .args([
                    "-loglevel",
                    "error",
                    "-readrate",
                    "1",
                    "-ss",
                    &format!("{:.3}", synchronized_position.as_secs_f64()),
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
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => child,
                Err(_) => return,
            };
            let Some(mut stdout) = child.stdout.take() else {
                return;
            };
            let frame_size = width as usize * height as usize * 3;
            while !worker_stop.load(Ordering::Relaxed) {
                let mut pixels = vec![0; frame_size];
                if stdout.read_exact(&mut pixels).is_err() {
                    break;
                }
                let _ = sender.try_send(VideoFrame {
                    width,
                    height,
                    pixels,
                });
            }
            let _ = child.kill();
            let _ = child.wait();
        });
        self.receiver = Some(receiver);
        self.stop = Some(stop);
        self.key = Some((source, width, height));
    }

    fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.receiver = None;
        self.key = None;
        self.latest = None;
    }
}

impl Drop for VideoScreensaver {
    fn drop(&mut self) {
        self.stop();
    }
}
