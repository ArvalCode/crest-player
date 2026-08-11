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

Search, stream resolution, permanent downloads, lyrics, and video-cache builds
run outside the input loop. Active jobs appear in a temporary **Download Queue**
panel, which closes when the final job finishes. Streamed tracks resolve
YouTube's best audio URL and play progressively through `ffplay`; they are not
downloaded as complete temporary MP3s. Permanent `Ctrl+L` downloads still save
an MP3 and build its reusable video cache.

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

## Active Video CPU Benchmark

Measured on an **Intel Core Ultra 9 386H** with 16 logical CPUs using a release
build, 80×24 terminal, ASCII Detailed, High color precision, 60 FPS, hardware
acceleration, and autoplay. Results were rerun on August 10, 2026, average 30
samples taken 750 ms apart, and include all media child processes. The video
overlay remained active throughout both runs, including decode-ahead,
audio-clock scheduling, ASCII conversion, synchronized lyrics, and terminal
rendering. Here, 100% equals one fully occupied logical CPU.

| Playback mode | Crest Player | `ffplay` | `ffmpeg` | `yt-dlp` | Combined average | Combined peak | Average total capacity | Peak total capacity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Downloaded MP3 + 8.6 MB `.crestvid` | 1.18% | 1.01% | 22.25% | 0.00% | **24.44%** | 28.92% | **1.53%** | 1.81% |
| Streamed song + live video | 1.01% | 1.05% | 18.13% | 6.44% | **26.63%** | 115.72% | **1.66%** | 7.23% |

Downloaded playback used **Liza – PARALLEL feat. 7** and its 8.6 MB cache;
streaming used **Daft Punk – One More Time** with live video processing.
Downloaded video averaged **24.44% of one core** (**1.53%** of the full CPU),
while streamed video averaged **26.63% of one core** (**1.66%** overall). The
streaming peak was a brief URL-resolution and prebuffering burst, not sustained
load. GPU usage was not measured; results vary by terminal, network, settings,
and FFmpeg build.

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

Optional system-wide install:

```sh
sudo cp target/release/crest-player /usr/local/bin/crest-player
```

### Linux application launcher

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

### Windows data locations

- Settings: `%APPDATA%\crest-player\settings.json`
- Captured Home wallpaper: `%APPDATA%\crest-player\home-wallpaper.rgb`
- Downloaded library and its index: the current user's Music folder

### Current Windows limitation

Native Windows supports playback, search, downloads, video, seeking, and skipping.
`Ctrl+P` pause/resume relies on Unix signals and does not currently suspend
`ffplay` natively; WSL uses the Linux behavior.

## Uninstalling

Uninstalling the executable does **not** delete downloaded music automatically.
If you want to remove the downloaded library too, do that first while Crest
Player is still installed: open **Settings**, choose **Delete All Downloaded
Media**, and confirm. This removes only the MP3 and `.crestvid` files recorded in
Crest Player's library index. Then quit the player.

### Arch Linux package

Remove the AUR package and dependencies that are no longer required by another
installed package:

```sh
sudo pacman -Rns crest-player-git
```

This removes the packaged executable, launcher, and desktop entry. Remove the
per-user settings and captured wallpaper with:

```sh
rm -r ~/.config/crest-player
```

If `XDG_CONFIG_HOME` is set to a custom location, remove its `crest-player`
directory instead. Finally, remove `ytmusic_library.csv` from your configured
Music directory. The usual Linux location is:

```sh
rm ~/Music/ytmusic_library.csv
```

If you chose to keep your downloaded library, leave its `*_ytmusic.mp3` and
matching `.crestvid` files in the Music directory. The index can be removed
without deleting those media files.

### Manual Linux installation

If you followed the manual commands in this README, remove all three installed
launcher files:

```sh
sudo rm /usr/local/bin/crest-player
sudo rm /usr/local/bin/crest-player-launch
sudo rm /usr/share/applications/io.github.ArvalCode.CrestPlayer.desktop
```

Older installations may have only `/usr/local/bin/crest-player`; a "No such
file" message for either newer launcher file is harmless. Remove settings,
wallpaper, and the Music-library index as described in the Arch section above.
You may then delete the cloned `crest-player` repository from wherever you
cloned it. Rust build output under `target/` is contained in that repository.

Do not remove shared tools such as FFmpeg, `yt-dlp`, Rust, or Git unless you know
that no other program or project uses them.

### Windows

First use **Settings > Delete All Downloaded Media** if you do not want to keep
downloaded songs. Delete the cloned `crest-player` folder, including its
`target` build directory, and delete any copy of `crest-player.exe` that you
manually placed elsewhere. Remove settings and the captured wallpaper in
PowerShell with:

```powershell
Remove-Item -Recurse -Force "$env:APPDATA\crest-player"
```

Lastly, delete `ytmusic_library.csv` from the current user's Music folder. Keep
the `*_ytmusic.mp3` and matching `.crestvid` files there if you chose to retain
the downloaded library. Crest Player does not create registry entries or a
Windows service.

## License
MIT
