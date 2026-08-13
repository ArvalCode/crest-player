use crate::app::{App, save_library};
use crate::security::{bounded_output, external_command};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn remove_crest_player() -> Result<(), String> {
    if std::env::args_os().count() != 2 {
        return Err("usage: crest-player --remove".to_string());
    }

    remove_crest_player_interactively()
}

/// Runs the same complete removal flow from the in-application Settings page.
/// The caller must restore the terminal before invoking this so stdin, sudo,
/// and the confirmation prompts are visible to the user.
pub fn remove_crest_player_from_settings() -> Result<(), String> {
    remove_crest_player_interactively()
}

fn remove_crest_player_interactively() -> Result<(), String> {
    println!("Crest Player removal");
    println!();
    println!("Choose what to remove:");
    println!("  1. Application only (keep music and settings)");
    println!("  2. Music and video only (keep the application and settings)");
    println!("  3. Everything (application, media, and settings)");
    println!();
    print!("Enter 1, 2, or 3: ");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not display the removal menu: {error}"))?;
    let mut selection = String::new();
    io::stdin()
        .read_line(&mut selection)
        .map_err(|error| format!("could not read the removal selection: {error}"))?;
    let choice = RemovalChoice::parse(selection.trim())?;

    let installation = if choice.removes_application() {
        let executable = std::env::current_exe()
            .and_then(|path| path.canonicalize())
            .map_err(|error| format!("could not identify the running executable: {error}"))?;
        Some(Installation::detect(&executable)?)
    } else {
        None
    };
    let removal_bytes = estimated_removal_bytes(choice, installation.as_ref());

    println!();
    println!("Selected: {}", choice.label());
    println!("Music files not recorded in Crest Player's index will not be touched.");
    #[cfg(unix)]
    if installation
        .as_ref()
        .is_some_and(Installation::requires_privilege)
    {
        println!("Removing the installed application files will require sudo.");
    }
    print!("Type REMOVE to continue: ");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not display the confirmation prompt: {error}"))?;

    let mut confirmation = String::new();
    io::stdin()
        .read_line(&mut confirmation)
        .map_err(|error| format!("could not read confirmation: {error}"))?;
    if confirmation.trim() != "REMOVE" {
        println!("Removal cancelled. No files were changed.");
        return Ok(());
    }

    if choice.removes_media() {
        remove_media_data()?;
    }
    if choice.removes_configuration() {
        remove_configuration()?;
    }
    if let Some(installation) = installation {
        installation.remove()?;
        crate::desktop_integration::remove()?;
    }
    println!("{} removed successfully.", choice.label());
    println!(
        "Removed {:.2} MiB from this computer.",
        removal_bytes as f64 / (1024.0 * 1024.0)
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum RemovalChoice {
    Application,
    Media,
    Everything,
}

impl RemovalChoice {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "1" => Ok(Self::Application),
            "2" => Ok(Self::Media),
            "3" => Ok(Self::Everything),
            _ => Err("invalid selection; run crest-player --remove and choose 1, 2, or 3".into()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Application => "Crest Player application",
            Self::Media => "Crest Player music and video",
            Self::Everything => "Crest Player application and all data",
        }
    }

    fn removes_application(self) -> bool {
        matches!(self, Self::Application | Self::Everything)
    }

    fn removes_media(self) -> bool {
        matches!(self, Self::Media | Self::Everything)
    }

    fn removes_configuration(self) -> bool {
        matches!(self, Self::Everything)
    }
}

fn estimated_removal_bytes(choice: RemovalChoice, installation: Option<&Installation>) -> u64 {
    let mut total = installation
        .filter(|_| choice.removes_application())
        .map(installation_size)
        .unwrap_or(0);
    if choice.removes_media() {
        total = total.saturating_add(media_size());
    }
    if choice.removes_configuration() {
        total = total.saturating_add(configuration_size());
    }
    total
}

fn installation_size(installation: &Installation) -> u64 {
    let mut paths = installation.installed_paths();
    paths.extend(crate::desktop_integration::paths());
    paths
        .into_iter()
        .collect::<HashSet<_>>()
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .filter(|metadata| metadata.is_file())
        .fold(0u64, |total, metadata| total.saturating_add(metadata.len()))
}

fn media_size() -> u64 {
    let mut files = HashSet::new();
    for (_, path) in App::new().library {
        let music = PathBuf::from(path);
        files.insert(music.with_extension("crestvid"));
        files.insert(music.with_extension("video.cache"));
        let mut partial = music.as_os_str().to_os_string();
        partial.push(".part");
        files.insert(PathBuf::from(partial));
        files.insert(music);
    }
    if let Some(audio_directory) = dirs::audio_dir() {
        files.insert(audio_directory.join("ytmusic_library.csv"));
    }
    files
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .filter(|metadata| metadata.is_file())
        .fold(0u64, |total, metadata| total.saturating_add(metadata.len()))
}

fn configuration_size() -> u64 {
    dirs::config_dir()
        .map(|directory| directory.join("crest-player"))
        .map(|directory| directory_size(&directory))
        .unwrap_or(0)
}

fn directory_size(directory: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return 0;
    };
    entries.flatten().fold(0u64, |total, entry| {
        let path = entry.path();
        let size = match entry.file_type() {
            Ok(file_type) if file_type.is_file() => {
                entry.metadata().map(|meta| meta.len()).unwrap_or(0)
            }
            Ok(file_type) if file_type.is_dir() => directory_size(&path),
            _ => 0,
        };
        total.saturating_add(size)
    })
}

fn remove_media_data() -> Result<(), String> {
    let mut app = App::new();
    let indexed_paths: Vec<PathBuf> = app
        .library
        .iter()
        .map(|(_, path)| PathBuf::from(path))
        .collect();
    let errors = app.delete_all_library_media();
    if !errors.is_empty() {
        return Err(format!(
            "could not remove all indexed media; the installation was kept so removal can be retried:\n{}",
            errors.join("\n")
        ));
    }
    save_library(&app.library);

    let audio_directory =
        dirs::audio_dir().ok_or_else(|| "could not locate the Music directory".to_string())?;
    let canonical_audio_directory = audio_directory
        .canonicalize()
        .map_err(|error| format!("could not inspect the Music directory: {error}"))?;
    for path in indexed_paths {
        let Some(parent) = path.parent() else {
            continue;
        };
        if parent.canonicalize().ok().as_deref() != Some(canonical_audio_directory.as_path()) {
            continue;
        }
        remove_file_if_present(&path.with_extension("video.cache"))?;
        let mut partial_name = path.into_os_string();
        partial_name.push(".part");
        remove_file_if_present(Path::new(&partial_name))?;
    }
    remove_file_if_present(&audio_directory.join("ytmusic_library.csv"))?;

    Ok(())
}

fn remove_configuration() -> Result<(), String> {
    let Some(config_directory) = dirs::config_dir() else {
        return Ok(());
    };
    let directory = config_directory.join("crest-player");
    match std::fs::remove_dir_all(&directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("could not remove {}: {error}", directory.display())),
    }
}

fn remove_file_if_present(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("could not remove {}: {error}", path.display())),
    }
}

enum Installation {
    #[cfg(unix)]
    SystemPackage(String),
    #[cfg(unix)]
    ManualSystem,
    #[cfg(unix)]
    ManualUser(PathBuf),
    #[cfg(windows)]
    Windows(PathBuf),
}

impl Installation {
    #[cfg(unix)]
    fn requires_privilege(&self) -> bool {
        matches!(self, Self::SystemPackage(_) | Self::ManualSystem)
    }

    fn detect(executable: &Path) -> Result<Self, String> {
        #[cfg(unix)]
        {
            if executable == Path::new("/usr/bin/crest-player") {
                if let Some(package) = owning_crest_package(executable) {
                    return Ok(Self::SystemPackage(package));
                }
                return Err(
                    "/usr/bin/crest-player is not owned by a recognized Crest Player package; remove it through the package manager that installed it"
                        .to_string(),
                );
            }
            if executable == Path::new("/usr/local/bin/crest-player") {
                return Ok(Self::ManualSystem);
            }
            if let Some(home) = dirs::home_dir() {
                let local_binary = home.join(".local/bin/crest-player");
                if executable == local_binary {
                    return Ok(Self::ManualUser(home));
                }
            }
            Err(format!(
                "{} is a development or unrecognized copy, not a supported system installation; no files were changed",
                executable.display()
            ))
        }
        #[cfg(windows)]
        {
            Ok(Self::Windows(executable.to_path_buf()))
        }
    }

    fn remove(self) -> Result<(), String> {
        #[cfg(unix)]
        {
            match self {
                Self::SystemPackage(package) => run_privileged(&[
                    OsStr::new("pacman"),
                    OsStr::new("-Rns"),
                    OsStr::new("--noconfirm"),
                    OsStr::new(&package),
                ]),
                Self::ManualSystem => run_privileged(&[
                    OsStr::new("rm"),
                    OsStr::new("-f"),
                    OsStr::new("/usr/local/bin/crest-player"),
                    OsStr::new("/usr/local/bin/crest-player-launch"),
                    OsStr::new("/usr/share/applications/io.github.ArvalCode.CrestPlayer.desktop"),
                    OsStr::new(
                        "/usr/share/icons/hicolor/1024x1024/apps/io.github.ArvalCode.CrestPlayer.png",
                    ),
                ]),
                Self::ManualUser(home) => {
                    let paths = [
                        home.join(".local/bin/crest-player"),
                        home.join(".local/bin/crest-player-launch"),
                        home.join(
                            ".local/share/applications/io.github.ArvalCode.CrestPlayer.desktop",
                        ),
                        home.join(
                            ".local/share/icons/hicolor/1024x1024/apps/io.github.ArvalCode.CrestPlayer.png",
                        ),
                    ];
                    for path in paths {
                        remove_file_if_present(&path)?;
                    }
                    Ok(())
                }
            }
        }
        #[cfg(windows)]
        {
            schedule_windows_executable_removal(&match self {
                Self::Windows(path) => path,
            })
        }
    }

    fn installed_paths(&self) -> Vec<PathBuf> {
        #[cfg(unix)]
        {
            match self {
                Self::SystemPackage(_) => vec![
                    PathBuf::from("/usr/bin/crest-player"),
                    PathBuf::from("/usr/bin/crest-player-launch"),
                    PathBuf::from(
                        "/usr/share/applications/io.github.ArvalCode.CrestPlayer.desktop",
                    ),
                    PathBuf::from(
                        "/usr/share/icons/hicolor/1024x1024/apps/io.github.ArvalCode.CrestPlayer.png",
                    ),
                    PathBuf::from("/usr/share/licenses/crest-player/LICENSE"),
                    PathBuf::from("/usr/share/doc/crest-player/README.md"),
                ],
                Self::ManualSystem => vec![
                    PathBuf::from("/usr/local/bin/crest-player"),
                    PathBuf::from("/usr/local/bin/crest-player-launch"),
                    PathBuf::from(
                        "/usr/share/applications/io.github.ArvalCode.CrestPlayer.desktop",
                    ),
                    PathBuf::from(
                        "/usr/share/icons/hicolor/1024x1024/apps/io.github.ArvalCode.CrestPlayer.png",
                    ),
                ],
                Self::ManualUser(home) => vec![
                    home.join(".local/bin/crest-player"),
                    home.join(".local/bin/crest-player-launch"),
                    home.join(
                        ".local/share/applications/io.github.ArvalCode.CrestPlayer.desktop",
                    ),
                    home.join(
                        ".local/share/icons/hicolor/1024x1024/apps/io.github.ArvalCode.CrestPlayer.png",
                    ),
                ],
            }
        }
        #[cfg(windows)]
        {
            match self {
                Self::Windows(path) => vec![path.clone()],
            }
        }
    }
}

#[cfg(unix)]
fn owning_crest_package(executable: &Path) -> Option<String> {
    let mut command = external_command("pacman");
    command.args([OsStr::new("-Qoq"), executable.as_os_str()]);
    let output = bounded_output(command, 1024).ok()?;
    if !output.status.success() {
        return None;
    }
    let package = String::from_utf8(output.stdout).ok()?;
    let package = package.trim();
    matches!(package, "crest-player" | "crest-player-git").then(|| package.to_string())
}

#[cfg(unix)]
fn run_privileged(arguments: &[&OsStr]) -> Result<(), String> {
    let (program, arguments) = arguments
        .split_first()
        .ok_or_else(|| "internal uninstall command was empty".to_string())?;
    let mut command = if unsafe { libc::geteuid() } == 0 {
        external_command(program.to_string_lossy().as_ref())
    } else {
        let mut command = external_command("sudo");
        command.arg(program);
        command
    };
    let status = command
        .args(arguments)
        .status()
        .map_err(|error| format!("could not start the system uninstaller: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "the system uninstaller did not complete successfully".to_string())
}

#[cfg(windows)]
fn schedule_windows_executable_removal(executable: &Path) -> Result<(), String> {
    let script =
        std::env::temp_dir().join(format!("crest-player-remove-{}.cmd", std::process::id()));
    let quoted_executable = executable.to_string_lossy().replace('"', "\"\"");
    let quoted_script = script.to_string_lossy().replace('"', "\"\"");
    std::fs::write(
        &script,
        format!(
            "@echo off\r\ntimeout /t 2 /nobreak >nul\r\ndel /f /q \"{quoted_executable}\"\r\ndel /f /q \"{quoted_script}\"\r\n"
        ),
    )
    .map_err(|error| format!("could not create the removal helper: {error}"))?;
    external_command("cmd")
        .args(["/C", "start", "", "/B", script.to_str().unwrap_or_default()])
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not schedule executable removal: {error}"))
}
