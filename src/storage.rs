use crate::app::load_library;
use crate::security::{bounded_output, external_command};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

const MAX_PACKAGE_METADATA_BYTES: usize = 16 * 1024 * 1024;
const RUNTIME_PACKAGE_ROOTS: [&str; 8] = [
    "ffmpeg", "yt-dlp", "openssl", "zlib", "zstd", "brotli", "libgcc", "glibc",
];

pub fn display_storage() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the Crest Player executable: {error}"))?;
    let application_bytes = file_size(&executable).unwrap_or(0);
    let runtime = runtime_package_storage();
    let application_runtime_bytes = runtime
        .as_ref()
        .map(|runtime| application_bytes.saturating_add(runtime.bytes))
        .unwrap_or(application_bytes);

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
    let overall_bytes = application_runtime_bytes.saturating_add(media_bytes);
    let missing_music = music_paths.len().saturating_sub(available_music);

    println!("Crest Player storage");
    println!();
    println!(
        "Application executable: {}",
        format_bytes(application_bytes)
    );
    if let Some(runtime) = runtime {
        println!(
            "Shared runtime packages: {} across {} package(s)",
            format_bytes(runtime.bytes),
            runtime.packages
        );
        println!(
            "Application + runtime:  {}",
            format_bytes(application_runtime_bytes)
        );
    } else {
        println!("Shared runtime packages: unavailable on this platform");
        println!(
            "Application + runtime:  {}",
            format_bytes(application_bytes)
        );
    }
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

struct RuntimeStorage {
    bytes: u64,
    packages: usize,
}

#[derive(Default)]
struct PackageMetadata {
    bytes: u64,
    dependencies: Vec<String>,
}

fn runtime_package_storage() -> Option<RuntimeStorage> {
    let mut command = external_command("pacman");
    command.arg("-Qi");
    let output = bounded_output(command, MAX_PACKAGE_METADATA_BYTES).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let packages = parse_pacman_metadata(&text);
    let mut queue: VecDeque<&str> = RUNTIME_PACKAGE_ROOTS.into_iter().collect();
    let mut included = HashSet::new();
    let mut bytes = 0u64;

    while let Some(name) = queue.pop_front() {
        if !included.insert(name.to_string()) {
            continue;
        }
        let Some(package) = packages.get(name) else {
            included.remove(name);
            continue;
        };
        bytes = bytes.saturating_add(package.bytes);
        for dependency in &package.dependencies {
            queue.push_back(dependency);
        }
    }

    (!included.is_empty()).then_some(RuntimeStorage {
        bytes,
        packages: included.len(),
    })
}

fn parse_pacman_metadata(text: &str) -> HashMap<String, PackageMetadata> {
    let mut packages = HashMap::new();
    for block in text.split("\n\n") {
        let mut name = None;
        let mut size = None;
        let mut dependencies = Vec::new();
        let mut reading_dependencies = false;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("Name            : ") {
                name = Some(value.trim().to_string());
                reading_dependencies = false;
            } else if let Some(value) = line.strip_prefix("Installed Size  : ") {
                size = parse_package_size(value.trim());
                reading_dependencies = false;
            } else if let Some(value) = line.strip_prefix("Depends On      : ") {
                append_dependencies(value, &mut dependencies);
                reading_dependencies = true;
            } else if reading_dependencies && line.starts_with(' ') && !line.contains(" : ") {
                append_dependencies(line, &mut dependencies);
            } else if !line.starts_with(' ') {
                reading_dependencies = false;
            }
        }
        if let (Some(name), Some(bytes)) = (name, size) {
            packages.insert(
                name,
                PackageMetadata {
                    bytes,
                    dependencies,
                },
            );
        }
    }
    packages
}

fn append_dependencies(value: &str, dependencies: &mut Vec<String>) {
    dependencies.extend(value.split_whitespace().filter_map(|dependency| {
        let name = dependency.split(['<', '>', '=']).next().unwrap_or_default();
        (!name.is_empty() && name != "None" && !name.contains(".so")).then(|| name.to_string())
    }));
}

fn parse_package_size(value: &str) -> Option<u64> {
    let (amount, unit) = value.split_once(' ')?;
    let amount: f64 = amount.parse().ok()?;
    let multiplier = match unit {
        "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((amount * multiplier).round() as u64)
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
    use super::{format_bytes, parse_package_size, parse_pacman_metadata};

    #[test]
    fn formats_storage_sizes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MiB");
    }

    #[test]
    fn parses_pacman_packages_and_wrapped_dependencies() {
        let metadata = parse_pacman_metadata(
            "Name            : ffmpeg\nDepends On      : glibc  libva.so=2-64\n                  zlib>=1.3\nInstalled Size  : 48.44 MiB\n\nName            : glibc\nDepends On      : None\nInstalled Size  : 50.63 MiB\n",
        );
        let ffmpeg = metadata.get("ffmpeg").unwrap();
        assert_eq!(ffmpeg.dependencies, ["glibc", "zlib"]);
        assert_eq!(ffmpeg.bytes, parse_package_size("48.44 MiB").unwrap());
        assert!(metadata.get("glibc").unwrap().dependencies.is_empty());
    }
}
