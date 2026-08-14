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
- Optionally publish the current track, playback state, and elapsed time through
  Discord Rich Presence.
- Optionally send audio to AirPlay and Sonos speakers on the local network.
- Persist settings in the platform configuration directory; on Linux this is
  normally `~/.config/crest-player/settings.json`.

## How It Works

### Non-blocking downloads and progressive streams

Search, stream resolution, permanent downloads, lyrics, and video-cache builds
run outside the input loop. Active jobs appear in a temporary **Download Queue**
panel, which closes when the final job finishes. Streamed tracks resolve
YouTube's best audio URL and play progressively through `ffplay`; they are not
downloaded as complete temporary MP3s. Permanent `Ctrl+L` downloads still save
an MP3 and build its reusable video cache.

Crest Player records the duration reported by YouTube when it resolves a stream.
If `ffplay` exits before that duration because of a temporary network or media
server interruption, the player displays **Reconnecting audio...** and resumes
the same track from its last known position. It advances the queue only after
the track reaches its expected end. A connection interruption can therefore
produce a short pause, but should not skip the unfinished song.

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

### AirPlay, Sonos, and Bluetooth speakers

> **AirPlay compatibility is experimental and has not yet been tested with a
> physical AirPlay speaker.** Sonos support and the common playback path do not
> guarantee that every AirPlay receiver will work. Bug reports with the receiver
> model and `atvremote` version are welcome.

Casting's lightweight integration layer is enabled in normal builds. It can be
excluded for a minimal build with:

```bash
cargo build --release --no-default-features
```

The integration delegates the network protocols to their established command-line
clients. Install [pyatv](https://pyatv.dev/) for AirPlay and
[SoCo-CLI](https://github.com/avantrec/soco-cli) for Sonos, for example in isolated
Python environments:

```bash
pipx install pyatv
pipx install soco-cli
```

When the Speakers page first scans, Crest Player checks for SoCo-CLI and installs it
through an existing `pipx` automatically if needed. It performs the same check for
pyatv/`atvremote` before scanning AirPlay and RAOP receivers. On Arch Linux it also attempts
to install `python-pipx` when sudo authorization is already cached or passwordless.
It never leaves an interactive password prompt running behind the terminal UI; if
authorization is required, the page shows the one command that must be run manually.
Dependency checks and discovery run only for an uncached scan or an explicit `R`
rescan—there is no continuous discovery process.

Both tools must be available on `PATH`. Open **Settings → Speakers**; Crest Player
scans in the background and presents one combined device list ordered as AirPlay,
Sonos, then Bluetooth. On Linux, Bluetooth speaker discovery uses BlueZ's
`bluetoothctl`, scans only while the Speakers page requests discovery, and filters
results to audio-sink devices rather than listing keyboards and other peripherals.
Select speakers with the arrow keys and toggle each one with Enter. Selected devices
are marked with `●` and play as one speaker group; Sonos and AirPlay devices can be
mixed. Playback and transport commands are dispatched independently so a slow or
offline receiver does not block commands to the rest of the group. Results are
cached for the session; press `R` to rescan or `D` to disconnect the whole group.
Use `+` and `-` to adjust one shared group volume in five-percent steps. Crest Player
sends the level concurrently through AirPlay and Sonos protocol controls; Bluetooth
uses the operating system's current audio-sink volume (`wpctl`/`pactl` on Linux,
system output volume on macOS, and Windows multimedia volume controls).

The equivalent command-bar controls remain available for scripting or manual
fallback:

```text
:cast airplay Living Room
:cast sonos Kitchen
:cast sonos 192.168.1.40
:cast status
:cast off
```

The selected output takes effect on the next track. Crest Player keeps a real-time
FFmpeg null-output clock so queue progression, lyrics, and video synchronization
continue without opening the local audio device. AirPlay names are resolved by
`atvremote`; Sonos accepts a room name or IPv4
address. Local Sonos files are served by SoCo-CLI only for the duration of playback.
For online tracks, Crest Player uses FFmpeg to convert the active stream to MP3 in
real time and exposes it through an ephemeral local relay restricted to the selected
Sonos IP. This avoids sending Sonos YouTube CDN URLs and formats it may reject. The
SoCo-CLI server may require inbound TCP ports 54000–54099 through the host firewall;
the streaming relay uses a temporary operating-system-assigned TCP port, and Sonos
discovery uses SSDP multicast on UDP port 1900.

For streamed Sonos playback, the relay records when the speaker makes its first
audio request. Crest Player holds playback time at zero until that event, then uses
the same monotonic clock for synchronized lyrics, video frame presentation, seeking,
and queue progression. Frames ahead of audio are held and obsolete frames are
dropped, matching local playback's synchronization behavior.

AirPlay playback sends downloaded files directly through `atvremote`. For online
tracks, Crest Player converts the active audio to a 192 kbps MP3 stream with FFmpeg
and pipes it to pyatv's RAOP sender; no temporary media file is created. The converter
and sender are terminated and reaped on stop, output change, or application exit.
Receivers requiring pairing or a password must be configured once with `atvremote`;
its credentials are then reused from pyatv's per-user storage.

Pause, resume, stop, and seeking are forwarded to the selected speaker. AirPlay
devices that require pairing or a password must first be configured with
`atvremote`. On quit, Crest Player waits for the receiver to acknowledge Stop,
then terminates and reaps the temporary casting helper and local media server.

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

### Discord Rich Presence

Start the Discord desktop client and enable **Discord Rich Presence** in Crest
Player's Settings. Crest Player includes its public Discord Application ID, so
users do not need to create their own application or provide credentials.
Developers can optionally override the ID with `CREST_DISCORD_CLIENT_ID`. If
Discord is closed or unavailable, playback continues normally.

Rich Presence uses the Crest Player application icon through Discord's CDN and
includes a **View on GitHub** button. Discord hides custom activity buttons from
the person broadcasting the activity; they are visible to other users viewing
that person's profile.

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
sudo install -Dm0644 packaging/linux/icons/io.github.ArvalCode.CrestPlayer-512.png \
  /usr/share/icons/hicolor/512x512/apps/io.github.ArvalCode.CrestPlayer.png
sudo update-desktop-database /usr/share/applications
sudo gtk-update-icon-cache -f -t /usr/share/icons/hicolor
```

## Windows (PowerShell)

Install Microsoft C++ Build Tools with **Desktop development with C++**, then
install Rust, Git, `yt-dlp`, and FFmpeg. Python is optional and is needed only
for AirPlay/Sonos casting. With WinGet:

```powershell
winget install --id Rustlang.Rustup --exact
winget install --id Git.Git --exact
winget install yt-dlp
winget install --id Gyan.FFmpeg --exact
```

For casting, also install Python:

```powershell
winget install --id Python.Python.3.13 --exact
```

Restart PowerShell so the tools are on `PATH`, then build:

```powershell
git clone https://github.com/ArvalCode/crest-player.git
Set-Location crest-player
cargo build --release
.\target\release\crest-player.exe
```

Windows Terminal is recommended for ANSI color and Unicode rendering.

Install Crest Player for the current Windows user without administrator rights:

```powershell
.\target\release\crest-player.exe --install-desktop
```

This copies the executable to `%LOCALAPPDATA%\Programs\Crest Player`, adds a
shortcut to the current user's Start Menu, and adds a shortcut to the current
user's Desktop. It also adds that installation directory to the current user's
`PATH`, making `crest-player` available in newly opened terminals. Re-running
the command safely refreshes the installed copy and shortcuts without adding a
duplicate `PATH` entry. Linux is detected separately and continues to use the per-user
`~/.local` integration described above; unsupported operating systems return a
clear error.

### Windows AirPlay and Sonos setup

Python is needed only for casting. In a regular, non-Administrator PowerShell
window, install `pipx` and the isolated casting helpers:

```powershell
py -m pip install --user pipx
py -m pipx ensurepath
```

Close and reopen PowerShell so its updated `PATH` is loaded, then run:

```powershell
pipx install soco-cli
pipx install pyatv
sonos-discover
atvremote scan
```

`sonos-discover` should list each Sonos room. `atvremote scan` should list
AirPlay/RAOP receivers; AirPlay support remains experimental and is not yet
hardware-tested by the Crest Player project.

Start Crest Player, open **Settings → Speakers**, and wait for the initial scan.
Use the arrow keys and Enter to select a speaker, `R` to rescan, and `D` to
disconnect. Selecting a speaker affects the next track. The helpers run only
during discovery, control operations, or casting; Crest Player installs no
Windows service or background startup task.

If a helper is installed but not found, verify the newly opened PowerShell sees
it before starting Crest Player:

```powershell
Get-Command sonos
Get-Command sonos-discover
Get-Command atvremote
```

Keep the computer and speaker on the same LAN and mark trusted home Wi-Fi as a
**Private** network in Windows Settings. Windows Defender Firewall may ask
whether Python can communicate on the network; allow **Private networks** only.
SoCo-CLI uses SSDP discovery on UDP 1900 and temporarily serves downloaded tracks
on TCP 54000–54099. Online playback uses a temporary dynamic TCP port selected by
Windows for Crest Player's MP3 relay. Do not expose casting ports on a Public
profile or forward them through the router. If playback fails after discovery succeeds,
check **Windows Security → Firewall & network protection → Allow an app through
firewall** and permit both Crest Player and the `soco-cli` Python environment on
Private networks.

On quit, Crest Player stops the receiver and closes the temporary HTTP server.
You can verify that no casting listener remains:

```powershell
Get-NetTCPConnection -LocalPort (54000..54099) -State Listen -ErrorAction SilentlyContinue
```

No output means the temporary Sonos server is closed.

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
- If Sonos appears but remains silent, run `sonos-discover`, verify the speaker
  is reachable, allow the helper on Private networks, select the speaker again,
  and start the next track.
- If speaker discovery returns nothing, confirm the Wi-Fi profile is Private,
  client/AP isolation is disabled on the router, and the computer and speaker
  are on the same subnet. Guest Wi-Fi commonly blocks local-device discovery.

### Windows data locations

- Settings: `%APPDATA%\crest-player\settings.json`
- Captured Home wallpaper: `%APPDATA%\crest-player\home-wallpaper.rgb`
- Downloaded library and its index: the current user's Music folder

### Current Windows limitation

Native Windows supports playback, search, downloads, video, seeking, skipping,
and optional Sonos/AirPlay discovery and casting.
`Ctrl+P` pause/resume relies on Unix signals and does not currently suspend
local `ffplay` natively; network-speaker pause/resume is forwarded through its
casting protocol. WSL uses the Linux behavior.

## Uninstalling

For a complete interactive uninstall, run:

```sh
crest-player --remove
```

The command detects Linux, macOS, and Windows separately before removing any
application files. Linux package installs use `pacman`, Linux manual installs
remove only their recognized system or per-user paths, macOS recognizes
`/usr/local/bin`, `/opt/homebrew/bin`, and the per-user `~/.local/bin` install,
and Windows removes its per-user executable, shortcuts, and `PATH` entry using
Windows-specific handling. Unrecognized application copies are refused rather
than applying another operating system's removal rules.

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
sudo rm /usr/share/icons/hicolor/512x512/apps/io.github.ArvalCode.CrestPlayer.png
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

Run `crest-player.exe --remove` and select the desired scope. An application
removal also removes the executable and shortcuts created by
`--install-desktop`. Delete the cloned `crest-player` folder, including its
`target` build directory, if it is no longer needed. Remove settings and the
captured Home wallpaper manually in PowerShell only if they were retained:

```powershell
Remove-Item -Recurse -Force "$env:APPDATA\crest-player" -ErrorAction SilentlyContinue
```

Lastly, remove `ytmusic_library.csv` from the current user's actual Music folder.
If in-app deletion was skipped, remove Crest Player's `*_ytmusic.mp3` and
matching `*_ytmusic.crestvid` files there, plus any matching `.part` or
`*_ytmusic.video.cache` remnants from interrupted work. Keep those media files
if you want to retain the downloaded library. Crest Player does not create
registry entries, scheduled tasks, or a Windows service.

FFmpeg, `yt-dlp`, Rust, Git, and Microsoft C++ Build Tools are separate shared
installations. They are not part of Crest Player and should be uninstalled only
if you installed them solely for this project and no longer need them.

After completing the applicable steps, no Crest Player executable, launcher,
icon, service, settings, wallpaper, library index, downloaded media, source
checkout, or project-local build output remains on the computer.

## License
MIT
