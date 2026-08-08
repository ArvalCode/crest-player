use crate::idle_mode::VideoRenderMode;
use crate::lyrics::LyricLine;
use dirs::audio_dir;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct PersistedSettings {
    lyrics_enabled: bool,
    live_sync_enabled: bool,
    pronunciations_enabled: bool,
    idle_video_enabled: bool,
    idle_video_render_mode: String,
    idle_video_fps: u16,
    autoplay_enabled: bool,
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            lyrics_enabled: true,
            live_sync_enabled: true,
            pronunciations_enabled: true,
            idle_video_enabled: true,
            idle_video_render_mode: "ascii_fast".to_string(),
            idle_video_fps: 15,
            autoplay_enabled: false,
        }
    }
}

pub struct App {
    pub input: String,
    pub results: Vec<(String, String)>,
    pub selected: usize,
    pub searching: bool,
    pub error: Option<String>,
    pub library: Vec<(String, String)>,
    pub show_library: bool,
    pub lyrics: Vec<LyricLine>,
    pub lyrics_message: String,
    pub lyrics_synced: bool,
    pub lyrics_active: Option<usize>,
    pub lyrics_scroll: u16,
    pub lyrics_enabled: bool,
    pub live_sync_enabled: bool,
    pub pronunciations_enabled: bool,
    pub idle_video_enabled: bool,
    pub idle_video_render_mode: VideoRenderMode,
    pub idle_video_fps: u16,
    pub autoplay_enabled: bool,
}

impl App {
    pub fn new() -> Self {
        let settings = load_settings();
        Self {
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            searching: false,
            error: None,
            library: load_library(),
            show_library: false,
            lyrics: Vec::new(),
            lyrics_message: "Play a song to load lyrics.".to_string(),
            lyrics_synced: false,
            lyrics_active: None,
            lyrics_scroll: 0,
            lyrics_enabled: settings.lyrics_enabled,
            live_sync_enabled: settings.live_sync_enabled,
            pronunciations_enabled: settings.pronunciations_enabled,
            idle_video_enabled: settings.idle_video_enabled,
            idle_video_render_mode: match settings.idle_video_render_mode.as_str() {
                "ascii_detailed" => VideoRenderMode::AsciiDetailed,
                "color_pixels" => VideoRenderMode::ColorPixels,
                _ => VideoRenderMode::AsciiFast,
            },
            idle_video_fps: match settings.idle_video_fps {
                30 | 60 => settings.idle_video_fps,
                _ => 15,
            },
            autoplay_enabled: settings.autoplay_enabled,
        }
    }
}

fn settings_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|directory| directory.join("crest-player/settings.json"))
}

fn load_settings() -> PersistedSettings {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

pub fn save_settings(app: &App) {
    let Some(path) = settings_path() else { return };
    let settings = PersistedSettings {
        lyrics_enabled: app.lyrics_enabled,
        live_sync_enabled: app.live_sync_enabled,
        pronunciations_enabled: app.pronunciations_enabled,
        idle_video_enabled: app.idle_video_enabled,
        idle_video_render_mode: match app.idle_video_render_mode {
            VideoRenderMode::AsciiFast => "ascii_fast",
            VideoRenderMode::AsciiDetailed => "ascii_detailed",
            VideoRenderMode::ColorPixels => "color_pixels",
        }
        .to_string(),
        idle_video_fps: app.idle_video_fps,
        autoplay_enabled: app.autoplay_enabled,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(path, json);
    }
}

pub fn save_library(library: &[(String, String)]) {
    if let Some(dir) = audio_dir() {
        let path = dir.join("ytmusic_library.csv");
        let _ = std::fs::write(
            path,
            library
                .iter()
                .map(|(t, p)| format!("{}|{}\n", t, p))
                .collect::<String>(),
        );
    }
}

pub fn load_library() -> Vec<(String, String)> {
    if let Some(dir) = audio_dir() {
        let path = dir.join("ytmusic_library.csv");
        if let Ok(data) = std::fs::read_to_string(path) {
            return data
                .lines()
                .filter_map(|l| {
                    l.split_once('|')
                        .map(|(t, p)| (t.to_string(), p.to_string()))
                })
                .collect();
        }
    }
    Vec::new()
}
