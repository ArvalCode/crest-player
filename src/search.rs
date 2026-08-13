//
use crate::lyrics::fetch_lyrics_with_caption_fallback;
use crate::security::{
    MAX_METADATA_BYTES, bounded_output, contained_media_path, external_command,
    sanitize_display_text_limited, valid_youtube_id,
};
use crate::video_cache::build_video_cache;
use dirs::audio_dir;
use std::path::PathBuf;

pub fn search_youtube(query: &str) -> Result<Vec<(String, String)>, String> {
    let mut command = external_command("yt-dlp");
    command.args([
        "--socket-timeout".to_string(),
        "10".to_string(),
        "--retries".to_string(),
        "2".to_string(),
        format!("ytsearch20:{}", query), // changed from ytsearch5 to ytsearch20
        "--flat-playlist".to_string(),
        "--dump-json".to_string(),
    ]);
    let output =
        bounded_output(command, MAX_METADATA_BYTES).map_err(|e| format!("yt-dlp failed: {}", e))?;
    if !output.status.success() {
        return Err("yt-dlp search failed".to_string());
    }
    let results = String::from_utf8_lossy(&output.stdout);
    let mut songs = vec![];
    for line in results.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line)
            && let (Some(title), Some(id)) = (json.get("title"), json.get("id"))
        {
            let title = sanitize_display_text_limited(title.as_str().unwrap_or(""), 512);
            let id = id.as_str().unwrap_or("");
            if !title.is_empty() && valid_youtube_id(id) {
                songs.push((title, id.to_string()));
            }
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
    let dir = audio_dir()?;
    let path = contained_media_path(&dir, title, "_ytmusic.mp3").ok()?;
    let mut command = external_command("yt-dlp");
    let status = command
        .args([
            "--socket-timeout",
            "10",
            "--retries",
            "2",
            "--no-playlist",
            "-f",
            "bestaudio/best",
            "-x",
            "--audio-format",
            "mp3",
            "-o",
            path.to_str()?,
            url,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if status.success() {
        if let Some((width, height, fps)) = video_cache_plan {
            let video_path = path.with_extension("video.cache");
            let cache_path = path.with_extension("crestvid");
            let video_downloaded = external_command("yt-dlp")
                .args([
                    "--socket-timeout",
                    "10",
                    "--retries",
                    "2",
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
                let lyrics = fetch_lyrics_with_caption_fallback(title, url).ok();
                let _ = build_video_cache(
                    video_path.to_str().unwrap_or_default(),
                    cache_path.to_str().unwrap_or_default(),
                    width,
                    height,
                    fps,
                    lyrics.as_ref(),
                );
            }
            let _ = std::fs::remove_file(video_path);
        }
        Some(path)
    } else {
        None
    }
}
