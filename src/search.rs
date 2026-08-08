//
use dirs::audio_dir;
use std::path::PathBuf;
use std::process::Command;

pub fn search_youtube(query: &str) -> Result<Vec<(String, String)>, String> {
    let output = Command::new("yt-dlp")
        .args([
            format!("ytsearch20:{}", query), // changed from ytsearch5 to ytsearch20
            "--flat-playlist".to_string(),
            "--dump-json".to_string(),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("yt-dlp failed: {}", e))?;
    if !output.status.success() {
        return Err("yt-dlp search failed".to_string());
    }
    let results = String::from_utf8_lossy(&output.stdout);
    let mut songs = vec![];
    for line in results.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line)
            && let (Some(title), Some(id)) = (json.get("title"), json.get("id"))
        {
            songs.push((
                title.as_str().unwrap_or("").to_string(),
                id.as_str().unwrap_or("").to_string(),
            ));
        }
        if songs.len() >= 20 {
            break;
        }
    }
    Ok(songs)
}

pub fn download_audio(url: &str, title: &str) -> Option<PathBuf> {
    let dir = audio_dir().unwrap_or_else(|| PathBuf::from("."));
    let filename = format!("{}_ytmusic.mp3", title.replace('/', "_"));
    let path = dir.join(filename);
    let output = Command::new("yt-dlp")
        .args([
            "-f",
            "bestaudio",
            "-x",
            "--audio-format",
            "mp3",
            "-o",
            path.to_str().unwrap(),
            url,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("Failed to run yt-dlp");
    if output.status.success() {
        Some(path)
    } else {
        eprintln!("yt-dlp failed: {}", String::from_utf8_lossy(&output.stderr));
        None
    }
}
