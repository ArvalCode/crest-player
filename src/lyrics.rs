use crate::security::{
    MAX_LYRICS_BYTES, MAX_METADATA_BYTES, bounded_output, external_command, read_response_limited,
    sanitize_display_text_limited,
};
use ib_romaji::HepburnRomanizer;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct LyricLine {
    pub timestamp: Option<Duration>,
    pub text: String,
    pub romaji: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub synced: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LyricsResult {
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

pub fn fetch_lyrics(title: &str) -> Result<Lyrics, String> {
    let response = reqwest::blocking::Client::new()
        .get("https://lrclib.net/api/search")
        .query(&[("q", title)])
        .header(reqwest::header::USER_AGENT, "crest-player/0.1.0")
        .timeout(Duration::from_secs(10))
        .send()
        .map_err(|error| format!("Could not load lyrics: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("Lyrics service returned {}.", response.status()));
    }

    let response = read_response_limited(response, MAX_LYRICS_BYTES)?;
    let results: Vec<LyricsResult> = serde_json::from_slice(&response)
        .map_err(|error| format!("Invalid lyrics response: {error}"))?;
    let result = results
        .into_iter()
        .find(|result| result.synced_lyrics.is_some() || result.plain_lyrics.is_some())
        .ok_or_else(|| "No lyrics found for this song.".to_string())?;

    if let Some(synced) = result.synced_lyrics {
        let lines = parse_synced_lyrics(&synced);
        if !lines.is_empty() {
            return Ok(Lyrics {
                lines,
                synced: true,
            });
        }
    }

    let plain = result
        .plain_lyrics
        .ok_or_else(|| "No readable lyrics found for this song.".to_string())?;
    Ok(Lyrics {
        lines: add_romaji(&plain, false),
        synced: false,
    })
}

pub fn fetch_lyrics_with_caption_fallback(
    title: &str,
    video_source: &str,
) -> Result<Lyrics, String> {
    fetch_embedded_lyrics(video_source)
        .or_else(|_| fetch_lyrics(title))
        .or_else(|lyrics_error| {
            fetch_video_captions(video_source).map_err(|caption_error| {
                format!("{lyrics_error} Caption fallback also failed: {caption_error}")
            })
        })
}

fn fetch_embedded_lyrics(video_source: &str) -> Result<Lyrics, String> {
    let path = std::path::Path::new(video_source);
    if !path.is_file()
        || path.extension().and_then(|extension| extension.to_str()) != Some("crestvid")
    {
        return Err("No embedded lyrics source.".to_string());
    }
    let (output, synced) = std::thread::scope(|scope| {
        let synced = scope.spawn(|| embedded_lyrics_are_synced(video_source));
        let mut command = external_command("ffmpeg");
        command.args([
            "-loglevel",
            "error",
            "-i",
            video_source,
            "-map",
            "0:s:0",
            "-f",
            "webvtt",
            "pipe:1",
        ]);
        let output = bounded_output(command, MAX_LYRICS_BYTES);
        (output, synced.join().unwrap_or(true))
    });
    let output = output.map_err(|error| format!("Could not read embedded lyrics: {error}"))?;
    if !output.status.success() {
        return Err("This cache has no embedded lyrics.".to_string());
    }
    let contents = String::from_utf8_lossy(&output.stdout);
    let mut lines = parse_webvtt(&contents);
    if lines.is_empty() {
        return Err("Embedded lyrics were empty.".to_string());
    }
    if !synced {
        for line in &mut lines {
            line.timestamp = None;
        }
    }
    Ok(Lyrics { lines, synced })
}

fn embedded_lyrics_are_synced(video_source: &str) -> bool {
    let mut command = external_command("ffprobe");
    command.args([
        "-v",
        "error",
        "-select_streams",
        "s:0",
        "-show_entries",
        "stream_tags=CREST_SYNCED",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
        video_source,
    ]);
    bounded_output(command, 1024)
        .ok()
        .filter(|output| output.status.success())
        .is_none_or(|output| String::from_utf8_lossy(&output.stdout).trim() != "0")
}

fn fetch_video_captions(video_source: &str) -> Result<Lyrics, String> {
    let mut command = external_command("yt-dlp");
    command.args([
        "--socket-timeout",
        "10",
        "--retries",
        "2",
        "-J",
        "--playlist-items",
        "1",
        "--no-warnings",
        video_source,
    ]);
    let output = bounded_output(command, MAX_METADATA_BYTES)
        .map_err(|error| format!("Could not inspect video captions: {error}"))?;
    if !output.status.success() {
        return Err("yt-dlp could not inspect video captions.".to_string());
    }
    let root: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid video metadata: {error}"))?;
    let video = root
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| entries.first())
        .unwrap_or(&root);
    let track = select_caption_track(video)
        .ok_or_else(|| "This video has no usable captions.".to_string())?;
    let contents = if let Some(data) = track.get("data").and_then(Value::as_str) {
        data.to_string()
    } else {
        let url = track
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| "Caption metadata did not contain a URL.".to_string())?;
        let response = reqwest::blocking::Client::new()
            .get(url)
            .timeout(Duration::from_secs(10))
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|error| format!("Could not download video captions: {error}"))?;
        String::from_utf8(read_response_limited(response, MAX_LYRICS_BYTES)?)
            .map_err(|error| format!("Video captions were not UTF-8: {error}"))?
    };
    let lines = parse_webvtt(&contents);
    if lines.is_empty() {
        Err("Video captions contained no readable timed lines.".to_string())
    } else {
        Ok(Lyrics {
            lines,
            synced: true,
        })
    }
}

fn select_caption_track(video: &Value) -> Option<&Value> {
    ["subtitles", "automatic_captions"]
        .into_iter()
        .filter_map(|field| video.get(field)?.as_object())
        .find_map(|languages| {
            let language = languages
                .get("en")
                .or_else(|| {
                    languages
                        .iter()
                        .find(|(key, _)| key.starts_with("en-"))
                        .map(|(_, value)| value)
                })
                .or_else(|| {
                    languages
                        .iter()
                        .find(|(key, _)| key.as_str() != "live_chat")
                        .map(|(_, value)| value)
                })?;
            language
                .as_array()?
                .iter()
                .find(|format| format.get("ext").and_then(Value::as_str) == Some("vtt"))
        })
}

fn parse_webvtt(captions: &str) -> Vec<LyricLine> {
    let japanese = captions.chars().any(is_japanese);
    let romanizer = japanese.then(HepburnRomanizer::default);
    let mut lines = Vec::new();
    let mut input = captions.lines().peekable();
    while let Some(line) = input.next() {
        if lines.len() >= 10_000 {
            break;
        }
        let Some((start, _)) = line.split_once(" --> ") else {
            continue;
        };
        let Some(timestamp) = parse_caption_timestamp(start.trim()) else {
            continue;
        };
        let mut text = String::new();
        while let Some(next) = input.peek() {
            if next.trim().is_empty() {
                input.next();
                break;
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(next.trim());
            input.next();
        }
        let text = clean_caption_text(&text);
        if text.is_empty()
            || lines
                .last()
                .is_some_and(|line: &LyricLine| line.text == text)
        {
            continue;
        }
        let romaji = text
            .chars()
            .any(is_japanese)
            .then(|| romanize(&text, romanizer.as_ref().unwrap()));
        lines.push(LyricLine {
            timestamp: Some(timestamp),
            text,
            romaji,
        });
    }
    lines
}

fn parse_caption_timestamp(value: &str) -> Option<Duration> {
    let parts: Vec<&str> = value.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (
            0,
            minutes.parse::<u64>().ok()?,
            seconds.parse::<f64>().ok()?,
        ),
        [hours, minutes, seconds] => (
            hours.parse::<u64>().ok()?,
            minutes.parse::<u64>().ok()?,
            seconds.parse::<f64>().ok()?,
        ),
        _ => return None,
    };
    let total = hours as f64 * 3600.0 + minutes as f64 * 60.0 + seconds;
    (total.is_finite() && total >= 0.0)
        .then(|| Duration::try_from_secs_f64(total).ok())
        .flatten()
}

fn clean_caption_text(value: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }
    sanitize_display_text_limited(&output, 4096)
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_synced_lyrics(lyrics: &str) -> Vec<LyricLine> {
    let japanese_song = lyrics.chars().any(is_japanese);
    let romanizer = japanese_song.then(HepburnRomanizer::default);
    lyrics
        .lines()
        .filter_map(|line| {
            let end = line.find(']')?;
            let timestamp = parse_timestamp(line.get(1..end)?)?;
            let text = sanitize_display_text_limited(line.get(end + 1..)?.trim(), 4096);
            if japanese_song && !text.chars().any(is_japanese) {
                return None;
            }
            let romaji = text
                .chars()
                .any(is_japanese)
                .then(|| romanize(&text, romanizer.as_ref().unwrap()));
            Some(LyricLine {
                timestamp: Some(timestamp),
                text,
                romaji,
            })
        })
        .take(10_000)
        .collect()
}

fn parse_timestamp(value: &str) -> Option<Duration> {
    let (minutes, seconds) = value.split_once(':')?;
    let minutes: u64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.parse().ok()?;
    let total = minutes as f64 * 60.0 + seconds;
    (total.is_finite() && total >= 0.0)
        .then(|| Duration::try_from_secs_f64(total).ok())
        .flatten()
}

fn add_romaji(lyrics: &str, timestamps: bool) -> Vec<LyricLine> {
    let romanizer = HepburnRomanizer::default();
    let japanese_song = lyrics.chars().any(is_japanese);
    lyrics
        .lines()
        .filter(|line| !japanese_song || line.chars().any(is_japanese))
        .map(|line| {
            let text = sanitize_display_text_limited(line, 4096);
            let romaji = text
                .chars()
                .any(is_japanese)
                .then(|| romanize(&text, &romanizer));
            LyricLine {
                timestamp: timestamps.then(Duration::default),
                text,
                romaji,
            }
        })
        .take(10_000)
        .collect()
}

fn romanize(text: &str, romanizer: &HepburnRomanizer) -> String {
    let mut remaining = text;
    let mut output = String::new();
    let mut previous_script: Option<u8> = None;

    while !remaining.is_empty() {
        let choice = romanizer.romanize_and_try_for_each(remaining, |length, romaji| {
            Some((length, romaji.to_string()))
        });
        if let Some((length, romaji)) = choice {
            let script = japanese_script(remaining.chars().next().unwrap());
            if previous_script.is_some() && previous_script != Some(script) {
                output.push(' ');
            }
            output.push_str(&romaji);
            remaining = &remaining[length..];
            previous_script = Some(script);
        } else {
            let character = remaining.chars().next().unwrap();
            output.push(character);
            remaining = &remaining[character.len_utf8()..];
            previous_script = None;
        }
    }
    output
}

fn japanese_script(character: char) -> u8 {
    if matches!(character, '\u{3040}'..='\u{30ff}') {
        1 // hiragana or katakana
    } else {
        2 // kanji
    }
}

fn is_japanese(character: char) -> bool {
    matches!(character,
        '\u{3040}'..='\u{30ff}' |
        '\u{3400}'..='\u{4dbf}' |
        '\u{4e00}'..='\u{9fff}' |
        '\u{f900}'..='\u{faff}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_non_japanese_lyrics_unchanged() {
        let result = add_romaji("Hello\nworld", false);
        assert!(result.iter().all(|line| line.romaji.is_none()));
    }

    #[test]
    fn adds_romaji_below_japanese_lines() {
        let result = add_romaji("こんにちは世界", false);
        let romaji = result[0].romaji.as_ref().unwrap();
        assert!(romaji.contains("konnichi"), "{romaji}");
        assert!(romaji.split_whitespace().count() >= 2);
    }

    #[test]
    fn parses_lrc_timestamps() {
        let result = parse_synced_lyrics("[01:02.50]Translated line\n[01:05.00]世界");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].timestamp, Some(Duration::from_millis(65_000)));
        assert!(result[0].romaji.is_some());
    }

    #[test]
    fn parses_and_cleans_webvtt_captions() {
        let captions = "WEBVTT\n\n00:00:01.250 --> 00:00:03.000\n<c>Hello &amp; welcome</c>\n\n00:00:03.000 --> 00:00:04.000\n<c>Hello &amp; welcome</c>\n\n00:00:04.500 --> 00:00:06.000\nNext line\n";
        let result = parse_webvtt(captions);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].timestamp, Some(Duration::from_millis(1_250)));
        assert_eq!(result[0].text, "Hello & welcome");
        assert_eq!(result[1].text, "Next line");
    }

    #[test]
    fn rejects_non_finite_timestamps_and_terminal_controls() {
        assert!(parse_caption_timestamp("NaN").is_none());
        assert!(parse_timestamp("01:NaN").is_none());
        let result =
            parse_webvtt("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nSafe\u{1b}]52;payload\u{7}\n");
        assert_eq!(result[0].text, "Safe]52;payload");
    }
}
