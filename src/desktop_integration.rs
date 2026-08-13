#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
#[cfg(any(target_os = "linux", windows))]
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
const PATH_BLOCK_START: &str = "# >>> crest-player command >>>";
#[cfg(target_os = "linux")]
const PATH_BLOCK_END: &str = "# <<< crest-player command <<<";

#[cfg(target_os = "linux")]
const ICON: &[u8] = include_bytes!("../packaging/linux/icons/io.github.ArvalCode.CrestPlayer.png");

#[cfg(target_os = "linux")]
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
    ensure_command_on_path(&home)?;

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
    let legacy_icon =
        home.join(".local/share/icons/hicolor/scalable/apps/io.github.ArvalCode.CrestPlayer.svg");
    match std::fs::remove_file(legacy_icon) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("could not remove legacy icon: {error}")),
    }

    println!("Crest Player desktop integration installed.");
    println!("Executable:    {}", installed_executable.display());
    println!("Desktop entry: {}", desktop.display());
    println!("Icon:          {}", icon.display());
    println!("The crest-player command is available in newly opened terminals.");
    println!("If it does not appear immediately, log out and back in once.");
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn remove() -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "could not locate the home directory".to_string())?;
    for path in user_integration_paths(&home) {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("could not remove {}: {error}", path.display())),
        }
    }
    remove_command_path_settings(&home)?;
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn paths() -> Vec<std::path::PathBuf> {
    dirs::home_dir()
        .map(|home| user_integration_paths(&home).into_iter().collect())
        .unwrap_or_default()
}

#[cfg(windows)]
pub fn install() -> Result<(), String> {
    let [installed_executable, start_menu_shortcut, desktop_shortcut] =
        windows_integration_paths()?;
    let executable = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("could not locate the Crest Player executable: {error}"))?;

    create_parent(&installed_executable)?;
    create_parent(&start_menu_shortcut)?;
    create_parent(&desktop_shortcut)?;
    install_executable(&executable, &installed_executable)?;
    create_windows_shortcut(&installed_executable, &start_menu_shortcut)?;
    create_windows_shortcut(&installed_executable, &desktop_shortcut)?;
    ensure_windows_command_on_path(&installed_executable)?;

    println!("Crest Player desktop integration installed for Windows.");
    println!("Executable:          {}", installed_executable.display());
    println!("Start menu shortcut: {}", start_menu_shortcut.display());
    println!("Desktop shortcut:    {}", desktop_shortcut.display());
    println!("The crest-player command is available in newly opened terminals.");
    Ok(())
}

#[cfg(windows)]
pub fn remove() -> Result<(), String> {
    let [installed_executable, start_menu_shortcut, desktop_shortcut] =
        windows_integration_paths()?;
    // The running installed executable is removed by uninstall.rs after this
    // process exits. Trying to delete it here fails on Windows due to file
    // locking and aborts the rest of the removal flow.
    for path in [start_menu_shortcut, desktop_shortcut] {
        remove_file_if_present(&path)?;
    }
    remove_windows_command_from_path(&installed_executable)?;
    Ok(())
}

#[cfg(windows)]
pub fn paths() -> Vec<PathBuf> {
    windows_integration_paths()
        .map(|paths| paths.into_iter().collect())
        .unwrap_or_default()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn install() -> Result<(), String> {
    Err(format!(
        "--install-desktop is not supported on {}",
        std::env::consts::OS
    ))
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn remove() -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn paths() -> Vec<std::path::PathBuf> {
    Vec::new()
}

#[cfg(windows)]
fn windows_integration_paths() -> Result<[PathBuf; 3], String> {
    let local_data = dirs::data_local_dir()
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "could not locate the local application data directory".to_string())?;
    let roaming_data = dirs::data_dir()
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "could not locate the roaming application data directory".to_string())?;
    let desktop = dirs::desktop_dir()
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "could not locate the Desktop directory".to_string())?;
    Ok([
        local_data.join("Programs/Crest Player/crest-player.exe"),
        roaming_data.join("Microsoft/Windows/Start Menu/Programs/Crest Player.lnk"),
        desktop.join("Crest Player.lnk"),
    ])
}

#[cfg(windows)]
fn install_executable(source: &Path, destination: &Path) -> Result<(), String> {
    if destination
        .canonicalize()
        .ok()
        .as_deref()
        .is_some_and(|installed| installed == source)
    {
        return Ok(());
    }

    let parent = destination
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", destination.display()))?;
    let temporary = parent.join(format!(".crest-player-install-{}.tmp", std::process::id()));
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
        remove_file_if_present(destination)?;
        std::fs::rename(&temporary, destination)
            .map_err(|error| format!("could not install {}: {error}", destination.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn create_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))
}

#[cfg(windows)]
fn create_windows_shortcut(target: &Path, shortcut: &Path) -> Result<(), String> {
    const SCRIPT: &str = concat!(
        "$ErrorActionPreference='Stop';",
        "$shell=New-Object -ComObject WScript.Shell;",
        "$link=$shell.CreateShortcut($env:CREST_SHORTCUT);",
        "$link.TargetPath=$env:CREST_TARGET;",
        "$link.WorkingDirectory=$env:CREST_WORKING_DIRECTORY;",
        "$link.Description='Terminal music player';",
        "$link.IconLocation=$env:CREST_TARGET + ',0';",
        "$link.Save()"
    );
    let working_directory = target
        .parent()
        .ok_or_else(|| "installed executable has no parent directory".to_string())?;
    let status = crate::security::external_command("powershell")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .env("CREST_TARGET", target)
        .env("CREST_SHORTCUT", shortcut)
        .env("CREST_WORKING_DIRECTORY", working_directory)
        .status()
        .map_err(|error| format!("could not start the Windows shortcut installer: {error}"))?;
    if !status.success() || !shortcut.is_file() {
        return Err(format!("could not create {}", shortcut.display()));
    }
    Ok(())
}

#[cfg(windows)]
fn ensure_windows_command_on_path(executable: &Path) -> Result<(), String> {
    let directory = executable
        .parent()
        .ok_or_else(|| "installed executable has no parent directory".to_string())?;
    update_windows_user_path(directory, false)
}

#[cfg(windows)]
fn remove_windows_command_from_path(executable: &Path) -> Result<(), String> {
    let directory = executable
        .parent()
        .ok_or_else(|| "installed executable has no parent directory".to_string())?;
    update_windows_user_path(directory, true)
}

#[cfg(windows)]
fn update_windows_user_path(directory: &Path, remove: bool) -> Result<(), String> {
    const SCRIPT: &str = concat!(
        "$ErrorActionPreference='Stop';",
        "function Normalize-PathEntry($value){try{return [IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($value)).TrimEnd('\\')}catch{return $value.TrimEnd('\\')}};",
        "$target=Normalize-PathEntry $env:CREST_PATH_ENTRY;",
        "$current=[Environment]::GetEnvironmentVariable('Path','User');",
        "$parts=@($current -split ';' | Where-Object { $_ -and ((Normalize-PathEntry $_) -ine $target) });",
        "if($env:CREST_PATH_REMOVE -ne '1'){$parts += $target};",
        "$updated=($parts -join ';');",
        "if($updated.Length -gt 32767){throw 'The user PATH is too long'};",
        "[Environment]::SetEnvironmentVariable('Path',$updated,'User')"
    );
    let status = crate::security::external_command("powershell")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .env("CREST_PATH_ENTRY", directory)
        .env("CREST_PATH_REMOVE", if remove { "1" } else { "0" })
        .status()
        .map_err(|error| format!("could not update the Windows user PATH: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "could not update the Windows user PATH".to_string())
}

#[cfg(windows)]
fn remove_file_if_present(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("could not remove {}: {error}", path.display())),
    }
}

#[cfg(target_os = "linux")]
fn user_integration_paths(home: &Path) -> [std::path::PathBuf; 4] {
    [
        home.join(".local/bin/crest-player"),
        home.join(".local/bin/crest-player-launch"),
        home.join(".local/share/applications/io.github.ArvalCode.CrestPlayer.desktop"),
        home.join(".local/share/icons/hicolor/1024x1024/apps/io.github.ArvalCode.CrestPlayer.png"),
    ]
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn temporary_install_path(destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", destination.display()))?;
    Ok(parent.join(format!(".crest-player.install-{}.tmp", std::process::id())))
}

#[cfg(target_os = "linux")]
fn create_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))
}

#[cfg(target_os = "linux")]
fn ensure_command_on_path(home: &Path) -> Result<(), String> {
    let shell_block = format!(
        "{PATH_BLOCK_START}\ncase \":$PATH:\" in\n  *:\"$HOME/.local/bin\":*) ;;\n  *) export PATH=\"$HOME/.local/bin:$PATH\" ;;\nesac\n{PATH_BLOCK_END}\n"
    );
    let fish_block =
        format!("{PATH_BLOCK_START}\nfish_add_path --path $HOME/.local/bin\n{PATH_BLOCK_END}\n");

    // .profile covers POSIX login shells. Existing interactive-shell files are
    // also updated because many terminal emulators start non-login shells.
    append_managed_block(&home.join(".profile"), &shell_block)?;
    for relative in [".bash_profile", ".bashrc", ".zshrc"] {
        let path = home.join(relative);
        if path.is_file() {
            append_managed_block(&path, &shell_block)?;
        }
    }
    let fish_config = home.join(".config/fish/config.fish");
    if fish_config.is_file() {
        append_managed_block(&fish_config, &fish_block)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn append_managed_block(path: &Path, block: &str) -> Result<(), String> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if existing.contains(PATH_BLOCK_START) {
        return Ok(());
    }
    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("could not update {}: {error}", path.display()))?;
    write!(file, "{separator}{block}")
        .map_err(|error| format!("could not update {}: {error}", path.display()))
}

#[cfg(target_os = "linux")]
fn remove_command_path_settings(home: &Path) -> Result<(), String> {
    for path in [
        home.join(".profile"),
        home.join(".bash_profile"),
        home.join(".bashrc"),
        home.join(".zshrc"),
        home.join(".config/fish/config.fish"),
    ] {
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(start) = contents.find(PATH_BLOCK_START) else {
            continue;
        };
        let Some(relative_end) = contents[start..].find(PATH_BLOCK_END) else {
            continue;
        };
        let mut end = start + relative_end + PATH_BLOCK_END.len();
        if contents.as_bytes().get(end) == Some(&b'\n') {
            end += 1;
        }
        let mut updated = contents;
        updated.replace_range(start..end, "");
        std::fs::write(&path, updated)
            .map_err(|error| format!("could not update {}: {error}", path.display()))?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "linux")]
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{
        PATH_BLOCK_START, desktop_exec_path, ensure_command_on_path, remove_command_path_settings,
        shell_single_quote,
    };

    #[test]
    fn safely_quotes_launcher_paths() {
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
        assert_eq!(desktop_exec_path("a b"), "\"a b\"");
    }

    #[test]
    fn command_path_setup_is_idempotent_and_removable() {
        let home =
            std::env::temp_dir().join(format!("crest-player-path-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".bashrc"), "# existing settings\n").unwrap();

        ensure_command_on_path(&home).unwrap();
        ensure_command_on_path(&home).unwrap();
        let bashrc = std::fs::read_to_string(home.join(".bashrc")).unwrap();
        assert_eq!(bashrc.matches(PATH_BLOCK_START).count(), 1);
        assert!(bashrc.contains("$HOME/.local/bin"));

        remove_command_path_settings(&home).unwrap();
        let bashrc = std::fs::read_to_string(home.join(".bashrc")).unwrap();
        assert_eq!(bashrc, "# existing settings\n");
        std::fs::remove_dir_all(home).unwrap();
    }
}
