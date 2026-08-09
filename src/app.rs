use crate::idle_mode::{ColorPrecision, VideoRenderMode};
use crate::lyrics::LyricLine;
use crate::wallpaper::HomeWallpaper;
use dirs::audio_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct PersistedSettings {
    lyrics_enabled: bool,
    live_sync_enabled: bool,
    pronunciations_enabled: bool,
    idle_video_enabled: bool,
    idle_video_render_mode: String,
    color_precision: String,
    idle_video_fps: u16,
    hardware_acceleration_enabled: bool,
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
            color_precision: "high".to_string(),
            idle_video_fps: 15,
            hardware_acceleration_enabled: false,
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
    library_paths: HashSet<String>,
    available_library_paths: HashSet<String>,
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
    pub color_precision: ColorPrecision,
    pub idle_video_fps: u16,
    pub hardware_acceleration_enabled: bool,
    pub autoplay_enabled: bool,
    pub home_wallpaper: Option<HomeWallpaper>,
}

impl App {
    pub fn new() -> Self {
        let settings = load_settings();
        let library = load_library();
        let library_paths = library.iter().map(|(_, path)| path.clone()).collect();
        let available_library_paths = library
            .iter()
            .filter(|(_, path)| std::path::Path::new(path).is_file())
            .map(|(_, path)| path.clone())
            .collect();
        Self {
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            searching: false,
            error: None,
            library,
            library_paths,
            available_library_paths,
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
            color_precision: match settings.color_precision.as_str() {
                "low" => ColorPrecision::Low,
                "medium" => ColorPrecision::Medium,
                _ => ColorPrecision::High,
            },
            idle_video_fps: match settings.idle_video_fps {
                0 | 30 | 60 => settings.idle_video_fps,
                _ => 15,
            },
            hardware_acceleration_enabled: settings.hardware_acceleration_enabled,
            autoplay_enabled: settings.autoplay_enabled,
            home_wallpaper: HomeWallpaper::load(),
        }
    }

    pub fn is_library_path(&self, path: &str) -> bool {
        self.library_paths.contains(path)
    }

    pub fn is_library_file_available(&self, path: &str) -> bool {
        self.available_library_paths.contains(path)
    }

    pub fn add_library_track(&mut self, title: String, path: String) {
        let path = normalize_existing_path(path);
        self.library_paths.insert(path.clone());
        self.available_library_paths.insert(path.clone());
        self.library.push((title, path));
    }

    pub fn remove_library_track(&mut self, path: &str) -> std::io::Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.library
            .retain(|(_, library_path)| library_path != path);
        self.library_paths.remove(path);
        self.available_library_paths.remove(path);
        Ok(())
    }
}

fn normalize_existing_path(path: String) -> String {
    std::fs::canonicalize(&path)
        .map(|canonical| canonical.to_string_lossy().into_owned())
        .unwrap_or(path)
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
        color_precision: match app.color_precision {
            ColorPrecision::Low => "low",
            ColorPrecision::Medium => "medium",
            ColorPrecision::High => "high",
        }
        .to_string(),
        idle_video_fps: app.idle_video_fps,
        hardware_acceleration_enabled: app.hardware_acceleration_enabled,
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
                        .map(|(t, p)| (t.to_string(), normalize_existing_path(p.to_string())))
                })
                .collect();
        }
    }
    Vec::new()
}
