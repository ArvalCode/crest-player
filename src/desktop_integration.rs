#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(unix)]
const ICON: &str = include_str!("../packaging/linux/icons/io.github.ArvalCode.CrestPlayer.svg");

#[cfg(unix)]
pub fn install() -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "could not locate the home directory".to_string())?;
    let executable = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("could not locate the Crest Player executable: {error}"))?;

    let [installed_executable, launcher, desktop, icon] = user_integration_paths(&home);

    create_parent(&installed_executable)?;
    create_parent(&launcher)?;
    create_parent(&desktop)?;
    create_parent(&icon)?;
    install_executable(&executable, &installed_executable)?;

    let quoted_executable = shell_single_quote(&installed_executable.to_string_lossy());
    let launcher_contents = format!(
        "#!/bin/sh\n\napplication={quoted_executable}\nif command -v systemd-run >/dev/null 2>&1 \\\n+    && systemctl --user show-environment >/dev/null 2>&1; then\n    exec systemd-run --user --scope --quiet --unit=\"crest-player-$$\" \\\n+        --description=\"Crest Player\" \"$application\" \"$@\"\nfi\nexec \"$application\" \"$@\"\n"
    )
    .replace("\n+", "\n");
    std::fs::write(&launcher, launcher_contents)
        .map_err(|error| format!("could not write {}: {error}", launcher.display()))?;
    let mut permissions = std::fs::metadata(&launcher)
        .map_err(|error| format!("could not inspect {}: {error}", launcher.display()))?
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&launcher, permissions)
        .map_err(|error| format!("could not make {} executable: {error}", launcher.display()))?;

    let desktop_contents = format!(
        "[Desktop Entry]\nType=Application\nName=Crest Player\nGenericName=Terminal Music Player\nComment=Play music and ASCII video in the terminal\nExec={}\nIcon=io.github.ArvalCode.CrestPlayer\nTerminal=true\nCategories=AudioVideo;Audio;Player;\nKeywords=music;audio;youtube;terminal;\nStartupNotify=false\n",
        desktop_exec_path(&launcher.to_string_lossy())
    );
    std::fs::write(&desktop, desktop_contents)
        .map_err(|error| format!("could not write {}: {error}", desktop.display()))?;
    std::fs::write(&icon, ICON)
        .map_err(|error| format!("could not write {}: {error}", icon.display()))?;

    println!("Crest Player desktop integration installed.");
    println!("Executable:    {}", installed_executable.display());
    println!("Desktop entry: {}", desktop.display());
    println!("Icon:          {}", icon.display());
    println!("If it does not appear immediately, log out and back in once.");
    Ok(())
}

#[cfg(unix)]
pub fn remove() -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "could not locate the home directory".to_string())?;
    for path in user_integration_paths(&home) {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("could not remove {}: {error}", path.display())),
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn paths() -> Vec<std::path::PathBuf> {
    dirs::home_dir()
        .map(|home| user_integration_paths(&home).into_iter().collect())
        .unwrap_or_default()
}

#[cfg(not(unix))]
pub fn install() -> Result<(), String> {
    Err("--install-desktop is currently available on Linux only".to_string())
}

#[cfg(not(unix))]
pub fn remove() -> Result<(), String> {
    Ok(())
}

#[cfg(not(unix))]
pub fn paths() -> Vec<std::path::PathBuf> {
    Vec::new()
}

#[cfg(unix)]
fn user_integration_paths(home: &Path) -> [std::path::PathBuf; 4] {
    [
        home.join(".local/bin/crest-player"),
        home.join(".local/bin/crest-player-launch"),
        home.join(".local/share/applications/io.github.ArvalCode.CrestPlayer.desktop"),
        home.join(".local/share/icons/hicolor/scalable/apps/io.github.ArvalCode.CrestPlayer.svg"),
    ]
}

#[cfg(unix)]
fn install_executable(source: &Path, destination: &Path) -> Result<(), String> {
    if destination
        .canonicalize()
        .ok()
        .as_deref()
        .is_some_and(|installed| installed == source)
    {
        return Ok(());
    }

    let temporary = temporary_install_path(destination)?;
    let result = (|| {
        let mut input = std::fs::File::open(source)
            .map_err(|error| format!("could not read {}: {error}", source.display()))?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("could not create {}: {error}", temporary.display()))?;
        std::io::copy(&mut input, &mut output)
            .map_err(|error| format!("could not copy Crest Player: {error}"))?;
        output.sync_all().map_err(|error| {
            format!("could not finish writing {}: {error}", temporary.display())
        })?;
        let mut permissions = output
            .metadata()
            .map_err(|error| format!("could not inspect {}: {error}", temporary.display()))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&temporary, permissions).map_err(|error| {
            format!("could not make {} executable: {error}", temporary.display())
        })?;
        std::fs::rename(&temporary, destination)
            .map_err(|error| format!("could not install {}: {error}", destination.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
fn temporary_install_path(destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", destination.display()))?;
    Ok(parent.join(format!(".crest-player.install-{}.tmp", std::process::id())))
}

#[cfg(unix)]
fn create_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))
}

#[cfg(unix)]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn desktop_exec_path(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::{desktop_exec_path, shell_single_quote};

    #[test]
    fn safely_quotes_launcher_paths() {
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
        assert_eq!(desktop_exec_path("a b"), "\"a b\"");
    }
}
