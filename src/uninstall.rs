use crate::app::{App, save_library};
use crate::security::{bounded_output, external_command};
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn remove_crest_player() -> Result<(), String> {
    if std::env::args_os().count() != 2 {
        return Err("usage: crest-player --remove".to_string());
    }

    let executable = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("could not identify the running executable: {error}"))?;
    let installation = Installation::detect(&executable)?;

    println!("Crest Player removal");
    println!();
    println!("This will permanently remove:");
    println!("  - every MP3 and .crestvid file in Crest Player's library index");
    println!("  - the library index, settings, and captured Home wallpaper");
    println!("  - the installed executable, launcher, desktop entry, and icon");
    println!();
    println!("Music files not recorded in Crest Player's index will not be touched.");
    #[cfg(unix)]
    println!("Removing the installed application files will require sudo.");
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

    remove_user_data()?;
    installation.remove()?;
    println!("Crest Player and its data were removed successfully.");
    Ok(())
}

fn remove_user_data() -> Result<(), String> {
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

    if let Some(config_directory) = dirs::config_dir() {
        let directory = config_directory.join("crest-player");
        match std::fs::remove_dir_all(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("could not remove {}: {error}", directory.display()));
            }
        }
    }
    Ok(())
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
                        "/usr/share/icons/hicolor/scalable/apps/io.github.ArvalCode.CrestPlayer.svg",
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
                            ".local/share/icons/hicolor/scalable/apps/io.github.ArvalCode.CrestPlayer.svg",
                        ),
                    ];
                    let mut arguments: Vec<&OsStr> = vec![OsStr::new("rm"), OsStr::new("-f")];
                    arguments.extend(paths.iter().map(|path| path.as_os_str()));
                    run_privileged(&arguments)
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
