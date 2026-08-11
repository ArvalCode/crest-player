use crate::app::load_library;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn display_storage() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the Crest Player executable: {error}"))?;
    let application_bytes = file_size(&executable).unwrap_or(0);

    let library = load_library();
    let mut music_paths = HashSet::new();
    let mut video_paths = HashSet::new();
    for (_, path) in &library {
        let music_path = PathBuf::from(path);
        video_paths.insert(music_path.with_extension("crestvid"));
        music_paths.insert(music_path);
    }

    let (music_bytes, available_music) = total_existing_size(&music_paths);
    let (video_bytes, available_videos) = total_existing_size(&video_paths);
    let media_bytes = music_bytes.saturating_add(video_bytes);
    let overall_bytes = application_bytes.saturating_add(media_bytes);
    let missing_music = music_paths.len().saturating_sub(available_music);

    println!("Crest Player storage");
    println!();
    println!(
        "Application executable: {}",
        format_bytes(application_bytes)
    );
    println!(
        "Downloaded music:      {} across {} file(s)",
        format_bytes(music_bytes),
        available_music
    );
    println!(
        "Downloaded video:      {} across {} file(s)",
        format_bytes(video_bytes),
        available_videos
    );
    println!("Music + video total:   {}", format_bytes(media_bytes));
    println!("Overall total:         {}", format_bytes(overall_bytes));
    println!();
    println!("Indexed tracks:        {}", library.len());
    if missing_music > 0 {
        println!("Missing music files:   {missing_music}");
    }
    Ok(())
}

fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
}

fn total_existing_size(paths: &HashSet<PathBuf>) -> (u64, usize) {
    paths.iter().fold((0u64, 0usize), |(bytes, count), path| {
        if let Some(size) = file_size(path) {
            (bytes.saturating_add(size), count + 1)
        } else {
            (bytes, count)
        }
    })
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn formats_storage_sizes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MiB");
    }
}
