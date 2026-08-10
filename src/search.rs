//
use crate::video_cache::build_video_cache;
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

pub fn download_audio(
    url: &str,
    title: &str,
    video_cache_plan: Option<(u16, u16, u16)>,
) -> Option<PathBuf> {
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
        .ok()?;
    if output.status.success() {
        if let Some((width, height, fps)) = video_cache_plan {
            let video_path = path.with_extension("video.cache");
            let cache_path = path.with_extension("crestvid");
            let video_downloaded = Command::new("yt-dlp")
                .args([
                    "--no-playlist",
                    "-f",
                    "bestvideo[height<=720]/bestvideo/best[height<=720]/best",
                    "-o",
                    video_path.to_str().unwrap_or_default(),
                    url,
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if video_downloaded {
                let _ = build_video_cache(
                    video_path.to_str().unwrap_or_default(),
                    cache_path.to_str().unwrap_or_default(),
                    width,
                    height,
                    fps,
                );
            }
            let _ = std::fs::remove_file(video_path);
        }
        Some(path)
    } else {
        eprintln!("yt-dlp failed: {}", String::from_utf8_lossy(&output.stderr));
        None
    }
}
