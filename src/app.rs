use dirs::audio_dir;

pub struct App {
    pub input: String,
    pub results: Vec<(String, String)>,
    pub selected: usize,
    pub searching: bool,
    pub error: Option<String>,
    pub queue: Vec<(String, String)>,
    pub library: Vec<(String, String)>,
    pub show_library: bool,
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
