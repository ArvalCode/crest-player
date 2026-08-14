#[cfg(windows)]
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_LYRICS_BYTES: usize = 4 * 1024 * 1024;

/// Remove bytes that can be interpreted as terminal commands or visual spoofing.
pub fn sanitize_display_text(value: &str) -> String {
    sanitize_display_text_limited(value, usize::MAX)
}

pub fn sanitize_display_text_limited(value: &str, maximum_characters: usize) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_control()
                && !matches!(
                    *character,
                    '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                )
        })
        .take(maximum_characters)
        .collect()
}

pub fn valid_youtube_id(value: &str) -> bool {
    (6..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub fn valid_media_url(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && !url.host_str().unwrap_or_default().is_empty()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

/// Turn an untrusted media title into one filename component on every platform.
pub fn safe_media_filename(title: &str, suffix: &str) -> String {
    let mut output = String::with_capacity(title.len().min(120) + suffix.len());
    let mut previous_space = false;
    for character in sanitize_display_text(title).chars() {
        let allowed = character.is_alphanumeric() || matches!(character, '-' | '_' | '.' | ' ');
        let character = if allowed { character } else { '_' };
        if character == ' ' {
            if previous_space {
                continue;
            }
            previous_space = true;
        } else {
            previous_space = false;
        }
        if output.chars().count() >= 120 {
            break;
        }
        output.push(character);
    }
    let stem = output.trim_matches([' ', '.']).trim();
    let stem = if stem.is_empty() { "track" } else { stem };
    format!("{stem}{suffix}")
}

pub fn contained_media_path(root: &Path, title: &str, suffix: &str) -> io::Result<PathBuf> {
    let path = root.join(safe_media_filename(title, suffix));
    if path.parent() != Some(root)
        || path.strip_prefix(root).ok().is_none_or(|relative| {
            relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "media path escaped the library directory",
        ));
    }
    Ok(path)
}

/// Resolve tools only through absolute PATH entries. This prevents an empty or
/// relative PATH entry from executing a program planted in the working directory.
pub fn external_command(name: &str) -> Command {
    Command::new(external_command_path(name).unwrap_or_else(|| missing_executable_path(name)))
}

pub fn cancellable_status(mut command: Command, cancelled: &AtomicBool) -> io::Result<ExitStatus> {
    let mut child = command.spawn()?;
    loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::Interrupted, "job cancelled"));
        }
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn external_command_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .filter(|directory| directory.is_absolute())
        .flat_map(|directory| executable_candidates(directory, name))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
}

fn executable_candidates(directory: PathBuf, name: &str) -> Vec<PathBuf> {
    let candidates = vec![directory.join(name)];
    #[cfg(windows)]
    let candidates = {
        let mut candidates = candidates;
        let extensions = std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE"));
        for extension in extensions.to_string_lossy().split(';').filter(|extension| {
            extension.eq_ignore_ascii_case(".COM") || extension.eq_ignore_ascii_case(".EXE")
        }) {
            candidates.push(directory.join(format!("{name}{extension}")));
        }
        candidates
    };
    candidates
}

fn missing_executable_path(name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(format!(r"C:\__crest_missing__\{name}.exe"))
    }
    #[cfg(not(windows))]
    {
        Path::new("/").join("__crest_missing__").join(name)
    }
}

pub struct BoundedOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
}

pub fn bounded_output(mut command: Command, maximum: usize) -> io::Result<BoundedOutput> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = Vec::new();
    let read_result = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("subprocess stdout was unavailable"))?
        .take(maximum as u64 + 1)
        .read_to_end(&mut stdout);
    if let Err(error) = read_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    if stdout.len() > maximum {
        let _ = child.kill();
        let _ = child.wait();
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "subprocess output exceeded the safety limit",
        ));
    }
    Ok(BoundedOutput {
        status: child.wait()?,
        stdout,
    })
}

pub fn read_response_limited(
    response: reqwest::blocking::Response,
    maximum: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        return Err("remote response exceeded the safety limit".to_string());
    }
    let mut bytes = Vec::new();
    response
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read remote response: {error}"))?;
    if bytes.len() > maximum {
        return Err("remote response exceeded the safety limit".to_string());
    }
    Ok(bytes)
}

pub fn read_file_limited(path: impl AsRef<Path>, maximum: usize) -> io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > maximum as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "local data file exceeded the safety limit",
        ));
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "local data file exceeded the safety limit",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        contained_media_path, safe_media_filename, sanitize_display_text, valid_media_url,
        valid_youtube_id,
    };
    use std::path::Path;

    #[test]
    fn strips_terminal_control_and_bidi_sequences() {
        assert_eq!(
            sanitize_display_text("safe\u{1b}]52;payload\u{7}x\u{202e}y"),
            "safe]52;payloadxy"
        );
    }

    #[test]
    fn media_filename_cannot_create_path_components() {
        let filename = safe_media_filename(r"..\..\C:\Windows/<bad>|song", "_ytmusic.mp3");
        assert!(!filename.contains(['/', '\\', ':']));
        assert!(filename.ends_with("_ytmusic.mp3"));
        let root = Path::new("/music");
        assert_eq!(
            contained_media_path(root, "../../track", ".mp3")
                .unwrap()
                .parent(),
            Some(root)
        );
    }

    #[test]
    fn validates_youtube_identifiers() {
        assert!(valid_youtube_id("dQw4w9WgXcQ"));
        assert!(!valid_youtube_id("../../escape"));
        assert!(!valid_youtube_id("id&list=other"));
    }

    #[test]
    fn accepts_only_network_media_urls_without_credentials() {
        assert!(valid_media_url("https://media.example/video?id=1"));
        assert!(!valid_media_url("file:///etc/passwd"));
        assert!(!valid_media_url("https://user:secret@example.com/video"));
    }
}
