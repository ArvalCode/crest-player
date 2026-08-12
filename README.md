# Crest Player

A lightweight terminal music player written in Rust. Crest Player searches YouTube,
plays local or downloaded music, displays synchronized lyrics, and turns into an
ASCII music-video display when left idle.

## Features

- Search and progressively stream YouTube audio without blocking the interface.
- Play, download, queue, and delete tracks from a local music library.
- Share one queue across streaming and downloaded-only modes.
- Show synchronized lyrics, YouTube-caption fallback, and optional Japanese romaji.
- Overlay current and upcoming lyrics during Ambient and Cinema playback.
- Render music videos as fast ASCII, detailed dithered ASCII, or true-color pixels.
- Choose 15, 24, 30, or 60 FPS, or adaptive **AUTO** mode.
- Predecode ten seconds of video, retain ten seconds of history, and drop late frames.
- Use optional hardware decoding with automatic software fallback.
- Save compact `.crestvid` caches with embedded lyrics for downloaded tracks.
- Capture a video frame as the Home wallpaper.
- Optionally prefetch YouTube Mix recommendations when the queue is empty.
- Persist settings in the platform configuration directory; on Linux this is
  normally `~/.config/crest-player/settings.json`.

## How It Works

### Non-blocking downloads and progressive streams

Search, temporary playback downloads, permanent downloads, lyrics, and
video-cache builds run outside the input loop. Active jobs appear in a temporary
**Download Queue** panel, which closes when the final job finishes. Selected
tracks finish downloading as temporary MP3s before `ffplay` starts, preventing
live network stalls from interrupting a song. The temporary file is deleted
after playback. Permanent `Ctrl+L` downloads save an MP3 in the music library
and build its reusable video cache.

### Compact `.crestvid` caches

New `.crestvid` files are Matroska containers tuned for terminal playback:

- H.264 video at 700 kbps, capped at 900 kbps
- YUV420 color and Lanczos scaling at terminal resolution
- ten-second keyframe boundaries for bounded seeking
- the x264 `slow` preset for better quality at the target size
- no audio duplication; the library MP3 remains separate
- embedded synchronized or plain lyrics when available

A four-minute cache typically targets roughly 20–27 MB, although simple videos
may be smaller. Crest Player also reads older V1/V2 indexed zstd caches. Cache
deletion is limited to songs and sidecar videos recorded in Crest Player's
library index.

### Smooth, clock-driven video

Video decoding starts when audio playback begins, before Ambient appears. A
background FFmpeg worker decodes up to ten seconds ahead while Crest Player keeps
up to ten seconds of recent frames for backward seeking. Both sides are bounded
by 64 MiB memory limits. Presentation follows the audio clock: obsolete frames
are dropped instead of rendered in a catch-up burst, and future frames wait for
their timestamp. Synchronized terminal updates reduce tearing.

Fixed 15, 24, 30, and 60 FPS modes are available. **AUTO** starts at 30 FPS and
adapts to measured terminal-render cost. FFmpeg can use available CPU threads;
optional hardware decoding retries in software if acceleration fails.

### Lyrics, recommendations, and presentation

Lyrics come from LRCLIB with manual or automatic YouTube captions as fallback.
Japanese text can be romanized locally, and downloaded caches carry their lyric
subtitle stream plus a marker indicating whether timing is genuine. Playback
checks embedded lyrics before using the network.

Autoplay resolves a YouTube Mix recommendation in the background and never
jumps ahead of manually queued tracks. Home keeps audio and queue progression
active without opening the video overlay. A visible video frame can also be
captured into the persisted Home wallpaper format without storing another full
video.

## ASCII Music-Video Playback

![A music video rendered as colored ASCII characters in Crest Player](docs/video-playback-ascii.png)

Video frames are sampled at terminal resolution and mapped to a colored ASCII
ramp. **ASCII Detailed** adds dithering and texture; **ASCII Fast** favors speed.
Color Precision balances terminal bandwidth against fidelity. Synchronized
lyrics remain overlaid.

Adjust terminal font size to trade speed for detail: smaller text increases video
resolution; larger text renders faster. Common shortcuts are `Ctrl+Shift++` and
`Ctrl+Shift+-`, though terminal bindings vary.

Press `` ` `` during video playback to capture the frame as the Home wallpaper.
Use **Reset Home Wallpaper** in Settings to restore the default mascot.

![A captured music-video frame used as the ASCII wallpaper on Crest Player's Home screen](docs/home-wallpaper.png)

## Video Playback Disclaimer

Terminal dimensions, render mode, color precision, FPS, and emulator performance
all affect smoothness. Audio stays synchronized when late video frames are
dropped. A GPU-accelerated terminal such as [Kitty](https://sw.kovidgoyal.net/kitty/)
is recommended.

## Four-Scenario CPU and GPU Benchmark

Measured August 11, 2026 on an **Intel Core Ultra 9 386H** with 16 logical CPUs
and integrated **Intel Panther Lake Xe graphics**. A release build ran in a
dedicated 80×24 Kitty window using ASCII Detailed, High color precision, fixed
60 FPS, hardware acceleration enabled, lyrics disabled, and autoplay disabled.
Each result averages 20 one-second steady-state samples after a 12-second
warm-up. Audio-only runs stayed on Home with the video screensaver disabled;
audio+video runs kept the active video view visible for the entire sample.

CPU figures include Crest Player and its `ffplay`, `ffmpeg`, and `yt-dlp`
descendants. CPU 100% means one fully occupied logical CPU. **Full CPU** divides
the combined value by all 16 logical CPUs. GPU figures are the sum of Xe render,
video, video-enhance, blitter, and compute engine busy percentages reported by
`gputop` for the dedicated Kitty window and Crest Player media processes; this
is an aggregate engine-utilization value, not percent of total system GPU power.

| Scenario | Crest Player CPU | `ffplay` CPU | `ffmpeg` CPU | `yt-dlp` CPU | Combined CPU avg | Combined CPU peak | Full CPU avg | GPU engine avg | GPU engine peak |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Downloaded audio only | 1.25% | 1.30% | 0.00% | 0.00% | **2.55%** | 4.00% | **0.16%** | 4.85% | 12.20% |
| Streamed audio only | 1.40% | 1.55% | 0.00% | 0.00% | **2.95%** | 4.00% | **0.18%** | 4.17% | 10.60% |
| Downloaded audio + cached video | 2.30% | 1.55% | 15.85% | 0.00% | **19.70%** | 22.00% | **1.23%** | 4.41% | 6.60% |
| Streamed audio + live video | 2.20% | 2.20% | 62.00% | 0.00% | **66.40%** | 85.00% | **4.15%** | 4.13% | 12.50% |

Downloaded runs used **Liza – PARALLEL feat. 7** with its 8.6 MB `.crestvid`
cache. The live search selected **IZA – Brisa** for both streamed runs. `yt-dlp`
completed URL resolution before the steady-state sample window, so its measured
CPU is zero; startup/search bursts are intentionally excluded.

GPU results include terminal rendering and vary with terminal, background
activity, drivers, and FFmpeg build.

## Controls

| Input | Action |
| --- | --- |
| Arrow keys | Navigate results and library entries |
| `Enter` | Search, play, queue, or activate a Home option |
| `Backspace` | Delete the previous character while entering a search |
| `Ctrl+L` | Download/save the selected track |
| `Delete` | Permanently remove the selected song from the library |
| `Ctrl+P` | Pause or resume |
| `Ctrl+N` | Skip to the next queued track |
| `Alt++` / `Alt+-` | Seek forward/backward five seconds |
| `V` | Toggle the library panel |
| `` ` `` | Capture the visible music-video frame as the Home wallpaper |
| `Esc` | Clear results and return to search |
| `Ctrl+Left Arrow` | Return to Home |
| `Ctrl+Q` | Quit |

### Downloaded-library commands

Enter these in **Downloaded Music Only** mode:

| Command | Action |
| --- | --- |
| `:shuffle queue` | Randomize the current playback queue |
| `:shuffle all` | Add every downloaded library song to the queue, then randomize it |
| `:clear` | Empty the playback queue without stopping the current song |

`Esc` clears the command bar. The first input wakes the screensaver and is
consumed; playback shortcuts continue to work in Ambient and Cinema.

## Getting Started

### Client requirements

To run a prebuilt Crest Player executable, the client computer needs:

- `yt-dlp` available on `PATH` for YouTube search, stream resolution, downloads,
  captions, and autoplay recommendations.
- FFmpeg available on `PATH`, including both `ffmpeg` and `ffplay`, for audio and
  video playback and cache creation.
- A terminal with ANSI color, Unicode, and true-color support. A GPU-accelerated
  terminal is recommended for smoother video rendering.
- Internet access to YouTube for search and streaming, and to LRCLIB for online
  lyrics. Previously downloaded songs remain playable without network access.
- Write access to the current user's configuration and Music directories so the
  app can save settings, wallpaper data, the library index, MP3 files, and
  `.crestvid` video caches.

Rust, Cargo, Git, and platform compiler tools are needed only when building from
source; they are not runtime requirements for a prebuilt executable.

Confirm the external runtime tools before starting Crest Player:

```sh
yt-dlp --version
ffmpeg -version
ffplay -version
```

The application currently uses a terminal-native Ratatui interface. A Linux
desktop launcher makes it appear in the application menu with its own name and
icon, but opening it still creates a terminal window; it is not yet a native GUI
window.

### Security model

Crest Player treats YouTube titles, identifiers, URLs, captions, lyrics,
subprocess output, cache files, and the local library index as potentially
untrusted. Its main defenses are:

- **Terminal-injection resistance:** remote text is stripped of control
  characters and bidirectional-override/isolate characters before display.
  Length limits also prevent an attacker-controlled title or lyric from growing
  without bound.
- **Command-injection resistance:** media tools are started with structured
  process arguments rather than by building a shell command from remote text.
  Executables are resolved only through absolute `PATH` entries and then
  canonicalized, preventing an empty or relative `PATH` entry from selecting a
  planted executable in the current working directory.
- **Identifier and URL validation:** YouTube IDs accept only a bounded set of
  ASCII letters, digits, `_`, and `-`. Playback URLs must parse as HTTP or HTTPS,
  have a host, and contain no embedded username or password. Local-file and
  credential-bearing URL schemes returned as network media are rejected.
- **Path containment:** remote titles are converted into a single bounded
  filename component. Separators and platform-sensitive punctuation are
  replaced, and the resulting download path is checked to remain inside the
  Music directory.
- **Resource-exhaustion limits:** remote responses, subprocess output, settings,
  the library index, wallpaper data, cache dimensions, frame sizes, frame counts,
  compressed blocks, and live video buffers have explicit size or time bounds.
  Oversized or malformed inputs are rejected instead of being fully allocated.
- **Restricted deletion:** normal and bulk media deletion canonicalize paths and
  refuse targets outside the current user's Music directory. Bulk deletion acts
  only on files recorded in Crest Player's library index. Uninstallation accepts
  only recognized installation locations, explains its scope, and requires the
  user to type `REMOVE` before changing files.
- **Least ambient activity:** Crest Player installs no background daemon,
  service, scheduled task, registry entry, or inbound network listener. Media
  helpers run with the same operating-system account and permissions as Crest
  Player and end when playback or the application ends.

These controls reduce attacks delivered through malicious metadata, URLs,
filenames, oversized responses, altered cache/index files, or a hostile working
directory. They do not make Crest Player a sandbox, antivirus product, or
privilege boundary. A malicious replacement for `yt-dlp`, FFmpeg, Crest Player,
or another executable in an absolute `PATH` directory still runs with the
user's permissions. Install software from trusted sources, keep dependencies
updated, avoid running the player as root/Administrator, and protect writable
`PATH`, configuration, and Music directories from other untrusted accounts.

### Build from source

Install Rust, Cargo, `yt-dlp`, and FFmpeg (`ffplay` must be included).

Arch Linux:

```sh
sudo pacman -S yt-dlp ffmpeg
```

Ubuntu/Debian:

```sh
sudo apt update
sudo apt install yt-dlp ffmpeg
```

Build and run:

```sh
cargo build --release
./target/release/crest-player
```

Running `--install-desktop` copies the executable to
`~/.local/bin/crest-player`, so the source checkout can be moved or deleted
afterward without breaking the installed command or application launcher.

Optional system-wide install:

```sh
sudo cp target/release/crest-player /usr/local/bin/crest-player
```

### Command-line options

Display the available commands without opening the interface:

```sh
crest-player --help
```

| Option | Action |
| --- | --- |
| `-h`, `--help` | Display command-line help and exit |
| `--install-desktop` | Install or refresh the per-user executable, application launcher, and icon |
| `--remove` | Interactively remove Crest Player and its data |
| `--storage` | Display executable, shared runtime dependencies, and downloaded-media storage usage |

Run `crest-player` without an option to open the player normally.

On Arch Linux, `--storage` reads the installed package database and includes the
deduplicated recursive runtime package graph for FFmpeg, `ffplay`, `yt-dlp`, and
Crest Player's native shared libraries. It reports the executable, shared
runtime packages, downloaded media, and combined overall total separately.
Shared runtime packages may already be used by the operating system or other
applications, so this is a complete environment footprint rather than the disk
space uniquely owned by Crest Player. Platforms without supported package
metadata report the executable and media totals and mark shared runtime storage
as unavailable.

Example from the benchmark system:

```text
Crest Player storage

Application executable: 24.03 MiB
Shared runtime packages: 856.81 MiB across 207 package(s)
Application + runtime:  880.84 MiB
Downloaded music:      16.84 MiB across 6 file(s)
Downloaded video:      81.33 MiB across 6 file(s)
Music + video total:   98.17 MiB
Overall total:         979.00 MiB
```

The executable is therefore about 24 MiB, but the complete runtime environment
on that machine is about 881 MiB before downloaded media. These figures vary by
operating system, package versions, enabled FFmpeg features, and dependencies
already present on the client.

### Linux application launcher

For a per-user launcher that does not require `sudo`, run:

```sh
./target/release/crest-player --install-desktop
```

This installs the executable at `~/.local/bin/crest-player` along with the
launcher, desktop entry, and icon under `~/.local`, without `sudo` or writes to
`/usr/local`. The installer adds a marked, removable `~/.local/bin` PATH block
to the current user's standard shell startup files, so `crest-player` is
available in every newly opened terminal. Re-running the command updates the
installed executable without duplicating that configuration. The desktop entry
requests a dedicated terminal window for the player. If Crest Player does not
appear in the application menu immediately, log out and back in.

The Arch package installs a **Crest Player** desktop entry that opens the app in
a terminal. When a systemd user session is available, the launcher places Crest
Player and its `ffplay`, `ffmpeg`, and `yt-dlp` helpers in one named scope. System
monitors that display cgroups can therefore show the process tree as one Crest
Player application. On other Linux sessions, the launcher falls back to running
the player normally.

For a manual, system-wide installation, copy the launcher and desktop entry too:

```sh
sudo cp packaging/linux/crest-player-launch /usr/local/bin/
sudo cp packaging/linux/io.github.ArvalCode.CrestPlayer.desktop /usr/share/applications/
sudo install -Dm0644 packaging/linux/icons/io.github.ArvalCode.CrestPlayer.svg \
  /usr/share/icons/hicolor/scalable/apps/io.github.ArvalCode.CrestPlayer.svg
```

## Windows (PowerShell)

Install Microsoft C++ Build Tools with **Desktop development with C++**, then
install Rust, Git, `yt-dlp`, and FFmpeg. With WinGet:

```powershell
winget install --id Rustlang.Rustup --exact
winget install --id Git.Git --exact
winget install yt-dlp
winget install --id Gyan.FFmpeg --exact
```

Restart PowerShell so the tools are on `PATH`, then build:

```powershell
git clone https://github.com/ArvalCode/crest-player.git
Set-Location crest-player
cargo build --release
.\target\release\crest-player.exe
```

Windows Terminal is recommended for ANSI color and Unicode rendering.

### Playback troubleshooting

- If Crest Player reports that it cannot start `ffplay`, verify that `ffplay` is
  installed and available in the same environment's `PATH`.
- If search or stream resolution fails, update `yt-dlp`; YouTube changes can make
  older releases stop working.
- If a stream is interrupted, leave the player running while it shows
  **Reconnecting audio...**. It retries the unfinished song instead of advancing
  the queue.
- If video is choppy but audio remains synchronized, use **AUTO** or a lower FPS,
  select **ASCII Fast**, lower Color Precision, enlarge the terminal font, or
  disable hardware decoding if the local FFmpeg build does not support it.
- Downloaded songs work offline, but lyrics or video may be unavailable unless
  their data was embedded in the matching `.crestvid` cache.

### Windows data locations

- Settings: `%APPDATA%\crest-player\settings.json`
- Captured Home wallpaper: `%APPDATA%\crest-player\home-wallpaper.rgb`
- Downloaded library and its index: the current user's Music folder

### Current Windows limitation

Native Windows supports playback, search, downloads, video, seeking, and skipping.
`Ctrl+P` pause/resume relies on Unix signals and does not currently suspend
`ffplay` natively; WSL uses the Linux behavior.

## Uninstalling

For a complete interactive uninstall, run:

```sh
crest-player --remove
```

The command offers three choices: remove only the application while keeping
music and settings, remove only indexed music/video while keeping the application
and settings, or remove everything. It explains the selected scope and requires
typing `REMOVE` before changing anything. Package installations are removed
through `pacman`; on Linux, system package and `/usr/local` removals require
`sudo`, while a per-user installation and personal data are removed as the
current user.
Application removal also deletes the per-user executable, launcher, desktop
entry, and icon created by `--install-desktop`.
After successful removal, it reports the total amount of Crest Player storage
removed in MiB.
The command refuses to uninstall when run from a development checkout or an
unrecognized location, preventing accidental source-tree deletion.

The platform-specific manual steps below are provided for auditing or recovery
if the executable no longer runs.

The following steps remove the program and every file owned by Crest Player.
Uninstalling only the executable intentionally leaves settings and downloaded
music behind.

Before uninstalling, quit Crest Player. Its systemd scope is transient and
disappears when the player and its media helpers exit; Crest Player does not
install a system service or background daemon.

If the program still runs, remove downloaded media safely from inside it first:
open **Settings**, choose **Delete All Known Songs/Videos**, and confirm. This
uses the library index to delete only MP3 and `.crestvid` files known to Crest
Player. Quit the player afterward. If you want to keep downloaded music, skip
this step.

### Arch Linux package

Remove the AUR package. The `-s` option also removes its dependencies only when
no other installed package needs them, while `-n` removes package backup files:

```sh
sudo pacman -Rns crest-player-git
```

This removes `/usr/bin/crest-player`, `/usr/bin/crest-player-launch`, the desktop
entry, icon, license, and packaged documentation. Remove per-user settings and
the captured Home wallpaper with:

```sh
rm -rf ~/.config/crest-player
```

If `XDG_CONFIG_HOME` points somewhere other than `~/.config`, remove the
`crest-player` directory there instead. Remove the library index from the Music
directory selected by your desktop environment (normally `~/Music`):

```sh
rm -f ~/Music/ytmusic_library.csv
```

If in-app deletion was skipped but you want complete data removal, inspect that
Music directory and delete Crest Player's `*_ytmusic.mp3` files and their
matching `*_ytmusic.crestvid` files. An interrupted download may also leave a
matching `.part` or `*_ytmusic.video.cache` file. Check filenames before removal
instead of applying a broad wildcard to a Music directory containing unrelated
files.

An AUR helper may retain a build checkout after the package is removed. These
common cache directories are not installed program files, but can be deleted if
present:

```sh
rm -rf ~/.cache/yay/crest-player-git
rm -rf ~/.cache/paru/clone/crest-player-git
```

### Manual Linux installation

If you followed the manual commands in this README, remove the installed player,
launcher, desktop entry, and icon:

```sh
sudo rm /usr/local/bin/crest-player
sudo rm /usr/local/bin/crest-player-launch
sudo rm /usr/share/applications/io.github.ArvalCode.CrestPlayer.desktop
sudo rm /usr/share/icons/hicolor/scalable/apps/io.github.ArvalCode.CrestPlayer.svg
```

Older installations may have only `/usr/local/bin/crest-player`; a "No such
file" message for a newer launcher or icon is harmless. Remove the configuration
directory, library index, optional media, and any interrupted-download remnants
as described in the Arch section above. Delete the cloned `crest-player`
repository from the exact location where you cloned it; its `target/` directory
contains all project-local Rust build output.

Desktop environments normally notice removed launchers automatically. If a
stale Crest Player entry or icon remains after logging out and back in, refresh
the standard caches when those utilities are installed:

```sh
update-desktop-database ~/.local/share/applications 2>/dev/null || true
gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
```

The package does not install either file under `~/.local`; these commands only
refresh the user's cached application menu and icons.

### Optional dependency removal on Linux

The Arch `pacman -Rns` command above already removes dependencies that became
unused. For a manual installation, FFmpeg and `yt-dlp` are runtime dependencies;
Rust and Git are build tools. Remove them through the same package manager used
to install them only if no other application or project needs them. System
libraries such as glibc, OpenSSL, zlib, zstd, and Brotli are shared components
and should not be removed manually.

Do not remove shared tools such as FFmpeg, `yt-dlp`, Rust, or Git unless you know
that no other program or project uses them.

### Windows

First use **Settings > Delete All Known Songs/Videos** if you do not want to keep
downloaded songs, then quit the player. Delete the cloned `crest-player` folder,
including its `target` build directory, and delete any copy of
`crest-player.exe` that you manually placed elsewhere. Remove settings and the
captured Home wallpaper in PowerShell with:

```powershell
Remove-Item -Recurse -Force "$env:APPDATA\crest-player" -ErrorAction SilentlyContinue
```

Lastly, remove `ytmusic_library.csv` from the current user's actual Music folder.
If in-app deletion was skipped, remove Crest Player's `*_ytmusic.mp3` and
matching `*_ytmusic.crestvid` files there, plus any matching `.part` or
`*_ytmusic.video.cache` remnants from interrupted work. Keep those media files
if you want to retain the downloaded library. Crest Player does not create
registry entries, scheduled tasks, Start Menu shortcuts, or a Windows service.

FFmpeg, `yt-dlp`, Rust, Git, and Microsoft C++ Build Tools are separate shared
installations. They are not part of Crest Player and should be uninstalled only
if you installed them solely for this project and no longer need them.

After completing the applicable steps, no Crest Player executable, launcher,
icon, service, settings, wallpaper, library index, downloaded media, source
checkout, or project-local build output remains on the computer.

## License
MIT
