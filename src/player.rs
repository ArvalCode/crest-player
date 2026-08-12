use crate::security::{external_command, sanitize_display_text_limited, valid_media_url};
use std::collections::HashMap;
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const RECONNECT_OVERLAP: Duration = Duration::from_secs(5);
const STREAM_READ_TIMEOUT_MICROS: &str = "5000000";

pub struct Player {
    pub child: Option<Child>,
    pub title: Option<String>,
    pub status: String,
    pub queue: Vec<(String, String)>,
    pub last_temp_file: Option<String>, // Track last temp file for deletion
    playback_started: Option<Instant>,
    elapsed_before_start: Duration,
    current_path: Option<String>,
    last_finished_title: Option<String>,
    video_sources: HashMap<String, Arc<str>>,
    stream_durations: HashMap<String, Duration>,
    audio_retry_at: Option<Instant>,
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
            last_finished_title: None,
            video_sources: HashMap::new(),
            stream_durations: HashMap::new(),
            audio_retry_at: None,
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
        // A streaming download creates its output file before it has finished.
        // Do not treat that partial file as playable until the completion event
        // replaces the temporary queue label with the resolved title.
        if title.ends_with(" (Downloading...)") {
            self.status = "Downloading...".to_string();
            self.title = Some(title.trim_end_matches(" (Downloading...)").to_string());
            self.current_path = Some(path.to_string());
            self.queue.push((title.to_string(), path.to_string()));
            return;
        }
        // If path is a local file and exists, play directly
        // If path is in the library, use the actual file path
        let play_path = if valid_media_url(path)
            || (Path::new(path).exists()
                && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false))
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
            self.status = format!(
                "Invalid file or ID: {}",
                sanitize_display_text_limited(path, 512)
            );
            return;
        };

        // Track temp file for deletion after playback if it's a temp streaming file
        if play_path.contains("ytmusic_play_") && play_path.ends_with(".mp3") {
            self.last_temp_file = Some(play_path.clone());
        } else {
            self.last_temp_file = None;
        }

        let mut command = external_command("ffplay");
        if valid_media_url(&play_path) {
            command.args(["-rw_timeout", STREAM_READ_TIMEOUT_MICROS]);
        }
        let child = command
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
        self.last_finished_title = None;
        self.audio_retry_at = None;
        self.status = "Playing".to_string();
        self.elapsed_before_start = Duration::default();
        self.playback_started = Some(Instant::now());
    }

    pub fn register_video_source(&mut self, audio_path: &str, youtube_url: &str) {
        self.video_sources
            .insert(audio_path.to_string(), Arc::from(youtube_url));
    }

    pub fn register_stream_duration(&mut self, audio_path: &str, duration: Duration) {
        self.stream_durations
            .insert(audio_path.to_string(), duration);
    }

    pub fn video_source(&self) -> Option<Arc<str>> {
        let title = self.title.as_ref()?;
        Some(
            self.current_path
                .as_ref()
                .and_then(|path| {
                    let cache = std::path::Path::new(path).with_extension("crestvid");
                    cache
                        .is_file()
                        .then(|| Arc::from(cache.to_string_lossy().into_owned()))
                })
                .or_else(|| {
                    self.current_path
                        .as_ref()
                        .and_then(|path| self.video_sources.get(path))
                        .cloned()
                })
                .unwrap_or_else(|| Arc::from(format!("ytsearch1:{title} official music video"))),
        )
    }

    pub fn cleanup_temp_media(&mut self) {
        if let Some(audio_path) = self.last_temp_file.take() {
            let _ = std::fs::remove_file(&audio_path);
            self.video_sources.remove(&audio_path);
            self.stream_durations.remove(&audio_path);
        }
        for (_, path) in self.queue.clone() {
            self.video_sources.remove(&path);
            self.stream_durations.remove(&path);
            if path.contains("ytmusic_play_") {
                let _ = std::fs::remove_file(&path);
            }
        }
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
            let _ = external_command("kill")
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
            let _ = external_command("kill")
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
        self.last_finished_title = None;
        self.audio_retry_at = None;
        // Do not clear the queue here; only clear on quit
    }

    pub fn last_finished_title(&self) -> Option<&str> {
        self.last_finished_title.as_deref()
    }

    pub fn download_failed(&mut self, path: &str) -> bool {
        if self.child.is_some()
            || self.status != "Downloading..."
            || self.current_path.as_deref() != Some(path)
        {
            return false;
        }
        self.status = "Stopped".to_string();
        self.title = None;
        self.current_path = None;
        self.playback_started = None;
        self.elapsed_before_start = Duration::default();
        self.advance_queue();
        true
    }
    pub fn is_playing(&mut self) -> bool {
        use std::fs;
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(exit_status)) if exit_status.success() && self.reached_expected_end() => {
                    self.child = None;
                    self.status = "Stopped".to_string();
                    self.last_finished_title = self.title.take();
                    if let Some(path) = self.current_path.take() {
                        self.video_sources.remove(&path);
                        self.stream_durations.remove(&path);
                    }
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
                    self.advance_queue()
                }
                Ok(Some(_)) | Err(_) => {
                    self.child = None;
                    if let Some(started) = self.playback_started.take() {
                        self.elapsed_before_start += started.elapsed();
                    }
                    // ffplay's lifetime is only an approximation of audible
                    // progress. A network stall can keep the process alive while
                    // no audio advances, so retry slightly behind that estimate
                    // instead of jumping over content the listener never heard.
                    self.elapsed_before_start =
                        self.elapsed_before_start.saturating_sub(RECONNECT_OVERLAP);
                    self.status = "Reconnecting audio...".to_string();
                    self.audio_retry_at = Some(Instant::now() + Duration::from_secs(2));
                    true
                }
                // The process is still running, but playback state did not change.
                Ok(None) => false,
            }
        } else {
            if self.audio_retry_at.is_some() {
                self.retry_audio_if_due()
            } else if self.status != "Downloading..." {
                self.advance_queue()
            } else {
                false
            }
        }
    }

    fn reached_expected_end(&self) -> bool {
        let Some(expected) = self
            .current_path
            .as_ref()
            .and_then(|path| self.stream_durations.get(path))
        else {
            // Local files and older queue entries have no separately supplied
            // duration; ffplay's successful exit remains authoritative for them.
            return true;
        };

        // Container timestamps and wall-clock playback can differ slightly.
        self.position() + Duration::from_secs(2) >= *expected
    }

    fn retry_audio_if_due(&mut self) -> bool {
        let Some(retry_at) = self.audio_retry_at else {
            return false;
        };
        if Instant::now() < retry_at {
            return false;
        }
        let (Some(path), Some(title)) = (self.current_path.clone(), self.title.clone()) else {
            self.audio_retry_at = None;
            return false;
        };
        let seek = format!("{:.3}", self.elapsed_before_start.as_secs_f64());
        let mut command = external_command("ffplay");
        if valid_media_url(&path) {
            command.args(["-rw_timeout", STREAM_READ_TIMEOUT_MICROS]);
        }
        match command
            .args(["-ss", &seek, "-nodisp", "-autoexit", &path])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                self.child = Some(child);
                self.title = Some(title);
                self.status = "Playing".to_string();
                self.playback_started = Some(Instant::now());
                self.audio_retry_at = None;
            }
            Err(_) => {
                self.status = "Reconnecting audio...".to_string();
                self.audio_retry_at = Some(Instant::now() + Duration::from_secs(2));
            }
        }
        true
    }

    fn advance_queue(&mut self) -> bool {
        let Some((title, path)) = self.queue.first().cloned() else {
            return false;
        };
        if title.ends_with(" (Downloading...)") {
            self.status = "Downloading...".to_string();
            self.title = Some(title.trim_end_matches(" (Downloading...)").to_string());
            self.current_path = Some(path);
            self.playback_started = None;
            self.elapsed_before_start = Duration::default();
            return true;
        }
        self.queue.remove(0);
        self.play(&path, &title);
        true
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
        let mut command = external_command("ffplay");
        if valid_media_url(&path) {
            command.args(["-rw_timeout", STREAM_READ_TIMEOUT_MICROS]);
        }
        let child = command
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
                let _ = external_command("kill")
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

#[cfg(test)]
mod tests {
    use super::{Player, RECONNECT_OVERLAP};

    #[cfg(unix)]
    #[test]
    fn failed_audio_process_reconnects_instead_of_skipping() {
        use std::process::Command;
        use std::time::{Duration, Instant};

        let mut player = Player::new();
        player.child = Some(Command::new("sh").args(["-c", "exit 1"]).spawn().unwrap());
        player.title = Some("Interrupted".to_string());
        player.current_path = Some("interrupted.mp3".to_string());
        player.status = "Playing".to_string();
        player.playback_started = Some(Instant::now());
        player
            .queue
            .push(("Next".to_string(), "next.mp3".to_string()));
        std::thread::sleep(Duration::from_millis(20));

        assert!(player.is_playing());
        assert_eq!(player.status, "Reconnecting audio...");
        assert_eq!(player.title.as_deref(), Some("Interrupted"));
        assert_eq!(player.queue.len(), 1);
        assert!(player.audio_retry_at.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn clean_early_stream_exit_reconnects_instead_of_skipping() {
        use std::process::Command;
        use std::time::{Duration, Instant};

        let mut player = Player::new();
        player.child = Some(Command::new("sh").args(["-c", "exit 0"]).spawn().unwrap());
        player.title = Some("Interrupted stream".to_string());
        player.current_path = Some("https://media.example/audio".to_string());
        player.status = "Playing".to_string();
        player.playback_started = Some(Instant::now());
        player.register_stream_duration("https://media.example/audio", Duration::from_secs(180));
        player
            .queue
            .push(("Next".to_string(), "next.mp3".to_string()));
        std::thread::sleep(Duration::from_millis(20));

        assert!(player.is_playing());
        assert_eq!(player.status, "Reconnecting audio...");
        assert_eq!(player.title.as_deref(), Some("Interrupted stream"));
        assert_eq!(player.queue.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_stream_rewinds_before_reconnecting() {
        use std::process::Command;
        use std::time::{Duration, Instant};

        let mut player = Player::new();
        player.child = Some(Command::new("sh").args(["-c", "exit 0"]).spawn().unwrap());
        player.title = Some("Interrupted stream".to_string());
        player.current_path = Some("https://media.example/audio".to_string());
        player.status = "Playing".to_string();
        player.playback_started = Some(Instant::now() - Duration::from_secs(30));
        player.register_stream_duration("https://media.example/audio", Duration::from_secs(180));
        std::thread::sleep(Duration::from_millis(20));

        assert!(player.is_playing());
        let expected = Duration::from_secs(30).saturating_sub(RECONNECT_OVERLAP);
        assert!(player.position() >= expected);
        assert!(player.position() < Duration::from_secs(27));
    }

    #[test]
    fn idle_player_consumes_the_next_queue_entry() {
        let mut player = Player::new();
        player.queue.push((
            "Missing".to_string(),
            "definitely-missing-track".to_string(),
        ));

        assert!(player.is_playing());
        assert!(player.queue.is_empty());
        assert!(player.status.starts_with("Invalid file or ID:"));
    }

    #[test]
    fn idle_player_waits_for_a_downloading_queue_entry() {
        let mut player = Player::new();
        player.queue.push((
            "Pending (Downloading...)".to_string(),
            "ytmusic_play_pending_test.mp3".to_string(),
        ));

        assert!(player.is_playing());
        assert_eq!(player.status, "Downloading...");
        assert_eq!(player.queue.len(), 1);
        assert!(!player.is_playing());
        assert_eq!(player.queue.len(), 1);
    }

    #[test]
    fn partial_download_file_is_not_started_early() {
        let mut player = Player::new();
        let path = std::env::temp_dir().join(format!(
            "ytmusic_play_partial_test_{}.mp3",
            std::process::id()
        ));
        std::fs::write(&path, b"partial audio").unwrap();
        player.queue.push((
            "Pending (Downloading...)".to_string(),
            path.to_string_lossy().into_owned(),
        ));

        assert!(player.is_playing());
        assert!(player.child.is_none());
        assert_eq!(player.status, "Downloading...");
        assert_eq!(player.queue.len(), 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_download_releases_the_waiting_state() {
        let mut player = Player::new();
        let failed_path = "ytmusic_play_failed_test.mp3";
        player.status = "Downloading...".to_string();
        player.title = Some("Failed".to_string());
        player.current_path = Some(failed_path.to_string());

        assert!(player.download_failed(failed_path));
        assert_eq!(player.status, "Stopped");
        assert!(player.title.is_none());
        assert!(player.current_path.is_none());
    }
}
