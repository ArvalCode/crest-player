//
use crate::lyrics::fetch_lyrics_with_caption_fallback;
use crate::security::{
    MAX_METADATA_BYTES, bounded_output, external_command, sanitize_display_text_limited,
    valid_youtube_id,
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
    path: &std::path::Path,
    video_cache_plan: Option<(u16, u16, u16)>,
) -> Result<PathBuf, String> {
    let dir = audio_dir().ok_or_else(|| "the Music directory is unavailable".to_string())?;
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("could not create the Music directory: {error}"))?;
    if path.parent() != Some(dir.as_path()) {
        return Err("the queued output path is outside the Music directory".to_string());
    }
    let mut command = external_command("yt-dlp");
    let status = command
        .args([
            "--socket-timeout",
            "10",
            "--retries",
            "2",
            "--force-overwrites",
            "--no-playlist",
            "-f",
            "bestaudio/best",
            "-x",
            "--audio-format",
            "mp3",
            "-o",
            path.to_str()
                .ok_or_else(|| "the output path is not valid UTF-8".to_string())?,
            url,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| format!("could not start yt-dlp: {error}"))?;
    if status.success() {
        let valid_output = path
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0);
        if !valid_output {
            return Err("yt-dlp exited successfully but did not create an MP3".to_string());
        }
        if let Some((width, height, fps)) = video_cache_plan {
            let video_path = path.with_extension("video.cache");
            let cache_path = path.with_extension("crestvid");
            let video_path_string = video_path
                .to_str()
                .ok_or_else(|| "the temporary video path is not valid UTF-8".to_string())?;
            let cache_path_string = cache_path
                .to_str()
                .ok_or_else(|| "the video cache path is not valid UTF-8".to_string())?;
            let video_status = external_command("yt-dlp")
                .args([
                    "--socket-timeout",
                    "10",
                    "--retries",
                    "2",
                    "--force-overwrites",
                    "--no-playlist",
                    "-f",
                    "bestvideo[height<=720]/bestvideo/best[height<=720]/best",
                    "-o",
                    video_path_string,
                    url,
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map_err(|error| format!("could not start the video download: {error}"))?;
            if !video_status.success() {
                let _ = std::fs::remove_file(&video_path);
                return Err(format!("the video download exited with {video_status}"));
            }
            let lyrics = fetch_lyrics_with_caption_fallback(title, url).ok();
            let cache_result = build_video_cache(
                video_path_string,
                cache_path_string,
                width,
                height,
                fps,
                lyrics.as_ref(),
            );
            let _ = std::fs::remove_file(video_path);
            cache_result
                .map_err(|error| format!("could not build the .crestvid cache: {error}"))?;
            if !cache_path
                .metadata()
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
            {
                return Err("the cache builder did not create a .crestvid file".to_string());
            }
        }
        Ok(path.to_path_buf())
    } else {
        let _ = std::fs::remove_file(path);
        Err(format!("yt-dlp exited with {status}"))
    }
}
