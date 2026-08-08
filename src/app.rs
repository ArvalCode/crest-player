use dirs::audio_dir;
use crate::lyrics::LyricLine;

pub struct App {
    pub input: String,
    pub results: Vec<(String, String)>,
    pub selected: usize,
    pub searching: bool,
    pub error: Option<String>,
    pub queue: Vec<(String, String)>,
    pub library: Vec<(String, String)>,
    pub show_library: bool,
    pub lyrics: Vec<LyricLine>,
    pub lyrics_message: String,
    pub lyrics_synced: bool,
    pub lyrics_active: Option<usize>,
    pub lyrics_scroll: u16,
    pub lyrics_enabled: bool,
    pub live_sync_enabled: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            searching: false,
            error: None,
            queue: Vec::new(),
            library: load_library(),
            show_library: false,
            lyrics: Vec::new(),
            lyrics_message: "Play a song to load lyrics.".to_string(),
            lyrics_synced: false,
            lyrics_active: None,
            lyrics_scroll: 0,
            lyrics_enabled: true,
            live_sync_enabled: true,
        }
    }
}

pub fn save_library(library: &[(String, String)]) {
    if let Some(dir) = audio_dir() {
        let path = dir.join("ytmusic_library.csv");
        let _ = std::fs::write(
            path,
            library.iter().map(|(t, p)| format!("{}|{}\n", t, p)).collect::<String>(),
        );
    }
}

pub fn load_library() -> Vec<(String, String)> {
    if let Some(dir) = audio_dir() {
        let path = dir.join("ytmusic_library.csv");
        if let Ok(data) = std::fs::read_to_string(path) {
            return data.lines().filter_map(|l| l.split_once('|').map(|(t, p)| (t.to_string(), p.to_string()))).collect();
        }
    }
    Vec::new()
}
