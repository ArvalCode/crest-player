use std::process::{Child, Stdio, Command};

pub struct Player {
    pub child: Option<Child>,
    pub title: Option<String>,
    pub status: String,
    pub queue: Vec<(String, String)>,
    pub last_temp_file: Option<String>, // Track last temp file for deletion
}

impl Player {
    pub fn new() -> Self {
        Self {
            child: None,
            title: None,
            status: "Stopped".to_string(),
            queue: Vec::new(),
            last_temp_file: None,
        }
    }

    pub fn play(&mut self, path: &str, title: &str) {
        use std::path::Path;
        use std::fs;
        // Before playing, clean up previous temp file if needed
        if let Some(last) = self.last_temp_file.take() {
            // Only delete if it is a temp streaming file (not a library file)
            if last.contains("ytmusic_play_") && last.ends_with(".mp3") {
                let _ = fs::remove_file(&last);
            }
        }

        if self.child.is_some() {
            // If already playing, add to queue
            self.queue.push((title.to_string(), path.to_string()));
            return;
        }
        self.stop();
        // If path is a local file and exists, play directly
        // If path is in the library, use the actual file path
        let play_path = if Path::new(path).exists() && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) {
            path.to_string()
        } else if path.len() == 11 && !path.contains('/') && !path.contains('.') {
            // YouTube ID: download to temp file
            let url = format!("https://www.youtube.com/watch?v={}", path);
            use std::time::{SystemTime, UNIX_EPOCH};
            let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
            let tmp_path = std::env::temp_dir().join(format!("ytmusic_play_{}.mp3", unique));
            let output = Command::new("yt-dlp")
                .args(["-f", "bestaudio", "-x", "--audio-format", "mp3", "-o", tmp_path.to_str().unwrap(), &url])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output();
            if let Ok(out) = &output {
                if !out.status.success() {
                    self.status = format!("yt-dlp failed: {}", String::from_utf8_lossy(&out.stderr));
                    return;
                }
            }
            if tmp_path.exists() && tmp_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                let tmp_str = tmp_path.to_str().unwrap().to_string();
                self.last_temp_file = Some(tmp_str.clone());
                tmp_str
            } else {
                self.status = format!("yt-dlp failed: file not created");
                return;
            }
        } else if path.contains("ytmusic_play_") && path.contains(".mp3") {
            // If this is a temp file path, but it doesn't exist, try to find the real file in the library
            if let Some(lib_path) = self.find_library_file(title) {
                lib_path
            } else {
                self.status = format!("File not found: {}", path);
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
            .spawn()
            .ok();
        self.child = child;
        self.title = Some(title.to_string());
        self.status = "Playing".to_string();
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
        }
    }
    pub fn resume(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = Command::new("kill")
                .arg("-CONT")
                .arg(child.id().to_string())
                .status();
            self.status = "Playing".to_string();
        }
    }
    pub fn stop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
        }
        self.child = None;
        self.status = "Stopped".to_string();
        self.title = None;
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
                    // After playback, delete temp streaming file if needed
                    if let Some(last) = self.last_temp_file.take() {
                        if last.contains("ytmusic_play_") && last.ends_with(".mp3") {
                            let _ = fs::remove_file(&last);
                        }
                    }
                    // Play next in queue if available (FIFO order)
                    if !self.queue.is_empty() {
                        let (title, path) = self.queue.remove(0);
                        self.play(&path, &title);
                        return true;
                    }
                    false
                },
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }
}
