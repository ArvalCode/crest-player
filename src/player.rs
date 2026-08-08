use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct Player {
    pub child: Option<Child>,
    pub title: Option<String>,
    pub status: String,
    pub queue: Vec<(String, String)>,
    pub last_temp_file: Option<String>, // Track last temp file for deletion
    playback_started: Option<Instant>,
    elapsed_before_start: Duration,
    current_path: Option<String>,
    video_sources: HashMap<String, String>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            child: None,
            title: None,
            status: "Stopped".to_string(),
            queue: Vec::new(),
            last_temp_file: None,
            playback_started: None,
            elapsed_before_start: Duration::default(),
            current_path: None,
            video_sources: HashMap::new(),
        }
    }

    pub fn play(&mut self, path: &str, title: &str) {
        use std::fs;
        use std::path::Path;
        if self.child.is_some() {
            self.queue.push((title.to_string(), path.to_string()));
            return;
        }

        // Before playing, clean up previous temp file if needed
        if let Some(last) = self.last_temp_file.take() {
            // Only delete if it is a temp streaming file (not a library file)
            if last.contains("ytmusic_play_") && last.ends_with(".mp3") {
                let _ = fs::remove_file(&last);
                self.video_sources.remove(&last);
            }
        }
        self.stop();
        // If path is a local file and exists, play directly
        // If path is in the library, use the actual file path
        let play_path = if Path::new(path).exists()
            && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
        {
            path.to_string()
        } else if path.contains("ytmusic_play_") && path.contains(".mp3") {
            // Downloads notify the main event loop through a channel. Never block
            // terminal input while waiting for a partially downloaded queue item.
            if let Some(lib_path) = self.find_library_file(title) {
                lib_path
            } else {
                self.status = "Downloading...".to_string();
                self.title = Some(title.trim_end_matches(" (Downloading...)").to_string());
                self.current_path = Some(path.to_string());
                self.queue.push((title.to_string(), path.to_string()));
                return;
            }
        } else {
            // Not a valid file or YouTube ID
            self.status = format!("Invalid file or ID: {}", path);
            return;
        };

        // Track temp file for deletion after playback if it's a temp streaming file
        if play_path.contains("ytmusic_play_") && play_path.ends_with(".mp3") {
            self.last_temp_file = Some(play_path.clone());
        } else {
            self.last_temp_file = None;
        }

        let child = Command::new("ffplay")
            .args(["-nodisp", "-autoexit", &play_path])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(child) = child else {
            self.status = "Unable to start ffplay".to_string();
            return;
        };
        self.child = Some(child);
        self.current_path = Some(play_path);
        self.title = Some(title.to_string());
        self.status = "Playing".to_string();
        self.elapsed_before_start = Duration::default();
        self.playback_started = Some(Instant::now());
    }

    pub fn register_video_source(&mut self, audio_path: &str, youtube_url: &str) {
        self.video_sources
            .insert(audio_path.to_string(), youtube_url.to_string());
    }

    pub fn video_source(&self) -> Option<String> {
        let title = self.title.as_ref()?;
        Some(
            self.current_path
                .as_ref()
                .and_then(|path| self.video_sources.get(path))
                .cloned()
                .unwrap_or_else(|| format!("ytsearch1:{title} official music video")),
        )
    }

    pub fn current_video_id(&self) -> Option<String> {
        let source = self
            .current_path
            .as_ref()
            .and_then(|path| self.video_sources.get(path))?;
        source
            .split_once("v=")
            .map(|(_, value)| value.split('&').next().unwrap_or(value).to_string())
    }

    /// Try to find a library file by title (for fallback if temp file is missing)
    pub fn find_library_file(&self, title: &str) -> Option<String> {
        if let Some(dir) = dirs::audio_dir() {
            let prefix = format!("{}_ytmusic", title.replace('/', "_"));
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.starts_with(&prefix) && fname.ends_with(".mp3") {
                        return Some(entry.path().to_string_lossy().to_string());
                    }
                }
            }
        }
        None
    }
    pub fn pause(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = Command::new("kill")
                .arg("-STOP")
                .arg(child.id().to_string())
                .status();
            self.status = "Paused".to_string();
            if let Some(started) = self.playback_started.take() {
                self.elapsed_before_start += started.elapsed();
            }
        }
    }
    pub fn resume(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = Command::new("kill")
                .arg("-CONT")
                .arg(child.id().to_string())
                .status();
            self.status = "Playing".to_string();
            self.playback_started = Some(Instant::now());
        }
    }
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.status = "Stopped".to_string();
        self.title = None;
        self.current_path = None;
        self.playback_started = None;
        self.elapsed_before_start = Duration::default();
        // Do not clear the queue here; only clear on quit
    }
    pub fn is_playing(&mut self) -> bool {
        use std::fs;
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.child = None;
                    self.status = "Stopped".to_string();
                    self.title = None;
                    self.current_path = None;
                    self.playback_started = None;
                    self.elapsed_before_start = Duration::default();
                    // After playback, delete temp streaming file if needed
                    if let Some(last) = self.last_temp_file.take()
                        && last.contains("ytmusic_play_")
                        && last.ends_with(".mp3")
                    {
                        let _ = fs::remove_file(&last);
                        self.video_sources.remove(&last);
                    }
                    // Play next in queue if available (FIFO order)
                    if !self.queue.is_empty() {
                        let (title, path) = self.queue.remove(0);
                        self.play(&path, &title);
                        return true;
                    }
                    false
                }
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    pub fn position(&self) -> Duration {
        self.elapsed_before_start
            + self
                .playback_started
                .map(|started| started.elapsed())
                .unwrap_or_default()
    }

    pub fn seek_by(&mut self, seconds: i64) {
        let Some(path) = self.current_path.clone() else {
            return;
        };
        let Some(title) = self.title.clone() else {
            return;
        };
        let was_paused = self.status == "Paused";
        let current = self.position().as_secs_f64();
        let target = (current + seconds as f64).max(0.0);

        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let child = Command::new("ffplay")
            .args([
                "-ss",
                &format!("{target:.3}"),
                "-nodisp",
                "-autoexit",
                &path,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();
        self.child = child;
        self.current_path = Some(path);
        self.title = Some(title);
        self.elapsed_before_start = Duration::from_secs_f64(target);
        if was_paused {
            if let Some(child) = &self.child {
                let _ = Command::new("kill")
                    .arg("-STOP")
                    .arg(child.id().to_string())
                    .status();
            }
            self.playback_started = None;
            self.status = "Paused".to_string();
        } else {
            self.playback_started = Some(Instant::now());
            self.status = "Playing".to_string();
        }
    }
}
