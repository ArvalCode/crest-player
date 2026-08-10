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

## Performance Benchmark

Measured on an **Intel Core Ultra 9 386H** with 16 logical CPUs using a release
build, 80×24 terminal, ASCII Fast, High color precision, 60 FPS, hardware
acceleration, and autoplay. Results average 30 one-second samples and include all
media child processes. Here, 100% equals one fully occupied logical CPU.

| Playback mode | Crest Player | `ffplay` | `ffmpeg` | `yt-dlp` | Combined average | Combined peak | Average total capacity | Peak total capacity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Downloaded MP3 + 8.6 MB `.crestvid` | 2.57% | 0.65% | 11.51% | 0.00% | **14.73%** | 22.10% | **0.92%** | 1.38% |
| Streamed song + live video | 1.36% | 2.76% | 46.89% | 7.90% | **58.90%** | 346.50% | **3.68%** | 21.66% |

Downloaded playback used **Liza – PARALLEL feat. 7** and its cache; streaming used
**Daft Punk – One More Time**. Streaming averaged about 4× more CPU. Its peak was
a brief resolution/prebuffering burst. GPU usage was not measured; results vary
by terminal, network, settings, and FFmpeg build.

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

Remove the installed binary and cloned repository. On Windows, also remove any
copied executable. Settings live in the platform configuration directory;
downloaded songs remain in the Music folder unless deleted separately.

## License
MIT
