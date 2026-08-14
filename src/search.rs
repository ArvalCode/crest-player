//
use crate::lyrics::fetch_lyrics_with_caption_fallback;
use crate::security::{
    MAX_METADATA_BYTES, bounded_output, cancellable_status, external_command,
    sanitize_display_text_limited, valid_youtube_id,
};
use crate::video_cache::build_video_cache_cancellable;
use dirs::audio_dir;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn search_youtube(query: &str) -> Result<Vec<(String, String)>, String> {
    let mut command = external_command("yt-dlp");
    command.args([
        "--ignore-config".to_string(),
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
    cancelled: &AtomicBool,
) -> Result<PathBuf, String> {
    let dir = audio_dir().ok_or_else(|| "the Music directory is unavailable".to_string())?;
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("could not create the Music directory: {error}"))?;
    if path.parent() != Some(dir.as_path()) {
        return Err("the queued output path is outside the Music directory".to_string());
    }
    let (width, height, fps) = video_cache_plan
        .ok_or_else(|| "a .crestvid cache plan is required for every download".to_string())?;
    let source_path = path.with_extension("download.mkv");
    let audio_part_path = path.with_extension("mp3.part");
    let cache_path = path.with_extension("crestvid");
    let source = source_path
        .to_str()
        .ok_or_else(|| "the temporary source path is not valid UTF-8".to_string())?;
    let audio_part = audio_part_path
        .to_str()
        .ok_or_else(|| "the temporary MP3 path is not valid UTF-8".to_string())?;
    let cache = cache_path
        .to_str()
        .ok_or_else(|| "the video cache path is not valid UTF-8".to_string())?;

    let _ = std::fs::remove_file(&source_path);
    let _ = std::fs::remove_file(&audio_part_path);
    let result = (|| {
        let mut source_command = external_command("yt-dlp");
        source_command.args([
                "--ignore-config",
                "--socket-timeout",
                "10",
                "--retries",
                "3",
                "--fragment-retries",
                "10",
                "--force-overwrites",
                "--no-playlist",
                "-f",
                "bestvideo[vcodec^=avc1][height<=720]+bestaudio/bestvideo[height<=720]+bestaudio/best[height<=720]/best",
                "--merge-output-format",
                "mkv",
                "--remux-video",
                "mkv",
                "-o",
                source,
                url,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let source_status = cancellable_status(source_command, cancelled)
            .map_err(|error| format!("could not start the source download: {error}"))?;
        if !source_status.success() || !source_path.is_file() {
            return Err(format!("the source download exited with {source_status}"));
        }

        let mut audio_command = external_command("ffmpeg");
        audio_command
            .args([
                "-y",
                "-nostdin",
                "-loglevel",
                "error",
                "-i",
                source,
                "-vn",
                "-c:a",
                "libmp3lame",
                "-q:a",
                "2",
                "-f",
                "mp3",
                audio_part,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let audio_status = cancellable_status(audio_command, cancelled)
            .map_err(|error| format!("could not start MP3 conversion: {error}"))?;
        if !audio_status.success() || !playable_audio_file(&audio_part_path) {
            return Err(format!("MP3 conversion failed with {audio_status}"));
        }

        if cancelled.load(Ordering::Acquire) {
            return Err("download cancelled".to_string());
        }
        let lyrics = fetch_lyrics_with_caption_fallback(title, url).ok();
        build_video_cache_cancellable(
            source,
            cache,
            width,
            height,
            fps,
            lyrics.as_ref(),
            cancelled,
        )
        .map_err(|error| format!("could not build the .crestvid cache: {error}"))?;
        if !cache_path
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        {
            return Err("the cache builder did not create a .crestvid file".to_string());
        }

        if cancelled.load(Ordering::Acquire) {
            return Err("download cancelled".to_string());
        }
        let _ = std::fs::remove_file(path);
        std::fs::rename(&audio_part_path, path)
            .map_err(|error| format!("could not publish the completed MP3: {error}"))?;
        Ok(path.to_path_buf())
    })();
    let _ = std::fs::remove_file(&source_path);
    if result.is_err() {
        let _ = std::fs::remove_file(&audio_part_path);
        let _ = std::fs::remove_file(&cache_path);
        let _ = std::fs::remove_file(format!("{cache}.part"));
    }
    result
}

pub fn playable_audio_file(path: &std::path::Path) -> bool {
    if !path
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
    {
        return false;
    }
    let Some(path) = path.to_str() else {
        return false;
    };
    let mut command = external_command("ffprobe");
    command.args([
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
        path,
    ]);
    bounded_output(command, 1024).is_ok_and(|output| {
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<f64>()
                .is_ok_and(|duration| duration.is_finite() && duration > 0.0)
    })
}
