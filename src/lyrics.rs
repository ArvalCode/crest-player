use ib_romaji::HepburnRomanizer;
use serde::Deserialize;
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

    let results: Vec<LyricsResult> = response
        .json()
        .map_err(|error| format!("Invalid lyrics response: {error}"))?;
    let result = results
        .into_iter()
        .find(|result| result.synced_lyrics.is_some() || result.plain_lyrics.is_some())
        .ok_or_else(|| "No lyrics found for this song.".to_string())?;

    if let Some(synced) = result.synced_lyrics {
        let lines = parse_synced_lyrics(&synced);
        if !lines.is_empty() {
            return Ok(Lyrics { lines, synced: true });
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

fn parse_synced_lyrics(lyrics: &str) -> Vec<LyricLine> {
    let japanese_song = lyrics.chars().any(is_japanese);
    let romanizer = japanese_song.then(HepburnRomanizer::default);
    lyrics
        .lines()
        .filter_map(|line| {
            let end = line.find(']')?;
            let timestamp = parse_timestamp(line.get(1..end)?)?;
            let text = line.get(end + 1..)?.trim().to_string();
            if japanese_song && !text.chars().any(is_japanese) {
                return None;
            }
            let romaji = text
                .chars()
                .any(is_japanese)
                .then(|| romanize(&text, romanizer.as_ref().unwrap()));
            Some(LyricLine { timestamp: Some(timestamp), text, romaji })
        })
        .collect()
}

fn parse_timestamp(value: &str) -> Option<Duration> {
    let (minutes, seconds) = value.split_once(':')?;
    let minutes: u64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.parse().ok()?;
    Some(Duration::from_secs_f64(minutes as f64 * 60.0 + seconds))
}

fn add_romaji(lyrics: &str, timestamps: bool) -> Vec<LyricLine> {
    let romanizer = HepburnRomanizer::default();
    let japanese_song = lyrics.chars().any(is_japanese);
    lyrics
        .lines()
        .filter(|line| !japanese_song || line.chars().any(is_japanese))
        .map(|line| {
            let romaji = line.chars().any(is_japanese).then(|| romanize(line, &romanizer));
            LyricLine {
                timestamp: timestamps.then(Duration::default),
                text: line.to_string(),
                romaji,
            }
        })
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
}
