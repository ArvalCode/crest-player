#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
const ICON: &str = include_str!("../packaging/linux/icons/io.github.ArvalCode.CrestPlayer.svg");

#[cfg(unix)]
pub fn install() -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "could not locate the home directory".to_string())?;
    let executable = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("could not locate the Crest Player executable: {error}"))?;

    let [launcher, desktop, icon] = user_integration_paths(&home);

    create_parent(&launcher)?;
    create_parent(&desktop)?;
    create_parent(&icon)?;

    let quoted_executable = shell_single_quote(&executable.to_string_lossy());
    let launcher_contents = format!(
        "#!/bin/sh\n\napplication={quoted_executable}\nif command -v systemd-run >/dev/null 2>&1 \\\n+    && systemctl --user show-environment >/dev/null 2>&1; then\n    exec systemd-run --user --scope --quiet --unit=\"crest-player-$$\" \\\n+        --description=\"Crest Player\" \"$application\" \"$@\"\nfi\nexec \"$application\" \"$@\"\n"
    );
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
fn user_integration_paths(home: &Path) -> [std::path::PathBuf; 3] {
    [
        home.join(".local/bin/crest-player-launch"),
        home.join(".local/share/applications/io.github.ArvalCode.CrestPlayer.desktop"),
        home.join(".local/share/icons/hicolor/scalable/apps/io.github.ArvalCode.CrestPlayer.svg"),
    ]
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
