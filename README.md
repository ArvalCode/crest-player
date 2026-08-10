# Crest Player

A lightweight terminal music player written in Rust. Crest Player searches YouTube,
plays local or downloaded music, displays synchronized lyrics, and turns into an
ASCII music-video display when left idle.

## Features

- Search YouTube and queue tracks without blocking the terminal interface.
- Share one continuous playback queue between streaming and downloaded-only modes.
- Keep audio and queue advancement running on Home without showing the video overlay.
- Keep all configuration on a dedicated Settings page and persist changes between
  launches in the platform user configuration directory.
  On Linux this is normally `~/.config/crest-player/settings.json`.
- Stream audio through `yt-dlp` and `ffplay` or play a downloaded-only library.
- Download and save favorite tracks locally.
- Permanently remove selected downloaded songs from the library with `Delete`.
- Display synchronized lyrics with optional Japanese romaji.
- Fall back to timed manual or automatic YouTube video captions when the lyrics
  service has no result.
- Show available Latin-letter pronunciation lines alongside original lyrics with
  the default-on **English Pronunciations** Home setting.
- Overlay the current synchronized lyric and a preview of the next line throughout
  Ambient and Cinema without resizing the video area.
- Seek, pause, resume, and skip queued tracks.
- Enter a staged YouTube music-video screensaver while music is playing:
  - **Ambient** after 5 seconds.
  - **Cinema** after 8 seconds.
- Render video as fast color ASCII, detailed dithered ASCII, or ANSI true-color half-block pixels.
- Select low, medium, or high color precision. Lower precision reduces terminal
  color changes and output bandwidth; high precision preserves the full RGB frame.
- Decode and present video at a fixed 15, 30, or 60 FPS, or select **AUTO** to
  adapt presentation speed to terminal performance while staying on the audio clock.
- Optionally try FFmpeg hardware video decoding from Settings, with automatic
  software fallback when acceleration is unavailable.
- Prebuffer decoded video frames in memory to absorb network/decode stalls, discard
  late frames, and release the buffer immediately when the screensaver ends.
- Skip missed presentation deadlines instead of issuing burst catch-up renders.
- Download a temporary video-only cache in parallel with streamed audio, prewarm
  decoded frames before the overlay appears, and delete cached media after use.
- Disable the YouTube screensaver, select its rendering style, or change its FPS from Home.
- Wake on keyboard input, mouse clicks, or scrolling. Mouse movement alone is ignored.
- Fall back to a procedural terminal animation while video loads or is unavailable.
- Optionally prefetch a related YouTube Mix track when the queue is empty with
  **Autoplay**. Manually queued tracks always take priority.

## ASCII Music-Video Playback

![A music video rendered as colored ASCII characters in Crest Player](docs/video-playback-ascii.png)

While a track is playing, Crest Player can turn its YouTube music video into a
terminal-native visualizer. Each decoded frame is sampled at the terminal's
resolution, and the brightness of each sample is mapped to a character in an
ASCII ramp. The sampled video color is retained, producing the colored ASCII
image shown above. **ASCII Detailed** adds ordered dithering and a larger
character ramp for extra texture, while **ASCII Fast** uses a shorter ramp for
lighter-weight rendering. The **Color Precision** setting cycles between Low,
Medium, and High to balance terminal performance against color fidelity.
Synchronized lyrics remain overlaid on the video.

Use `Ctrl+Shift++` or `Ctrl+Shift+-` in supported terminal emulators to increase
or decrease the terminal font size. A smaller font provides Crest Player with
more rows and columns, increasing the apparent resolution and detail of the
ASCII video; a larger font produces a coarser image that may render faster.
These shortcuts are controlled by the terminal emulator and may vary if its key
bindings have been customized.

Press `` ` `` while the video is visible to capture the current frame as the Home
wallpaper, replacing the default Crest mascot. It fills the space inside the
main white border above the separate, opaque menu and now-playing/navigation row
at the bottom. The lower interface remains clean and easy to read. The captured
frame is stored in the platform configuration directory and retains the
rendering style that was active when it was captured. Choose **Reset Home
Wallpaper** in Settings to delete it and restore the default mascot.

![A captured music-video frame used as the ASCII wallpaper on Crest Player's Home screen](docs/home-wallpaper.png)

## Video Playback Disclaimer

Terminal rendering can become a bottleneck during TUI video playback and may
cause the displayed frame rate to fluctuate or stutter, even when audio remains
synchronized. Performance depends on terminal dimensions, rendering style,
color precision, configured FPS, and the terminal emulator itself. A
GPU-accelerated terminal such as [Kitty](https://sw.kovidgoyal.net/kitty/) is
recommended when a smoother playback experience is needed.

## Performance Benchmark

The following playback benchmark was recorded on an **Intel Core Ultra 9 386H**
system with 16 logical CPUs. Crest Player was compiled in release mode and run
in an 80×24 terminal using ASCII Fast, High color precision,
60 FPS, hardware acceleration enabled, and autoplay enabled. Each result is the
average of 30 one-second steady-state samples. The measurements include Crest
Player and its `ffplay`, `ffmpeg`, and `yt-dlp` child processes.

In this table, 100% CPU means one fully occupied logical CPU. The total-capacity
columns normalize the measurements across all 16 logical CPUs.

| Playback mode | Crest Player | `ffplay` | `ffmpeg` | `yt-dlp` | Combined average | Combined peak | Average total capacity | Peak total capacity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Downloaded MP3 + 8.6 MB `.crestvid` | 2.57% | 0.65% | 11.51% | 0.00% | **14.73%** | 22.10% | **0.92%** | 1.38% |
| Streamed song + live video | 1.36% | 2.76% | 46.89% | 7.90% | **58.90%** | 346.50% | **3.68%** | 21.66% |

The downloaded test played **Liza – PARALLEL feat. 7** from the local library
with its `.crestvid` cache. The streamed test played **Daft Punk – One More
Time** using progressive audio and live video processing. Streamed playback used
about four times the average CPU of cached playback, primarily because FFmpeg
decoded and scaled the remote video while `yt-dlp` handled URL resolution and
autoplay prefetching. The high streamed peak was a short multi-core startup and
prebuffering burst rather than sustained utilization. GPU utilization was not
measured, and results will vary with terminal size, FPS, render mode, network,
FFmpeg build, and terminal emulator.

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

In **Downloaded Music Only** mode, type commands into the command bar and press
`Enter` to run them:

| Command | Action |
| --- | --- |
| `:shuffle queue` | Randomize the current playback queue |
| `:shuffle all` | Add every downloaded library song to the queue, then randomize it |
| `:clear` | Empty the playback queue without stopping the current song |

Command names and arguments are separated by spaces. `Esc` clears the command
bar without running it.

While the screensaver is visible, the first intentional input restores the normal
interface and is consumed so it cannot accidentally activate a control. Playback
shortcuts (`Ctrl+P`, `Ctrl+N`, `Alt++`, and `Alt+-`) operate without leaving the
Ambient or Cinema view.

## Getting Started
1. Ensure you have Rust and Cargo installed.
2. Install required system dependencies:
   - yt-dlp (YouTube downloader)
   - ffmpeg (audio/video processing)

   On Arch Linux:
   ```sh
   sudo pacman -S yt-dlp ffmpeg
   ```
   On Ubuntu/Debian:
   ```sh
   sudo apt update
   sudo apt install yt-dlp ffmpeg
   ```

3. Build the project:
   ```sh
   cargo build --release
   ```
4. Install the application system-wide (optional, for running from anywhere):
   ```sh
   sudo cp target/release/crest-player /usr/local/bin/crest-player
   ```

5. Run the application:
   ```sh
   crest-player
   ```

## Windows (PowerShell)

These steps build and run Crest Player natively on Windows 10 or 11.

1. Install the Microsoft C++ Build Tools with the **Desktop development with
   C++** workload, then install Rust through `rustup`. Rust's Windows toolchain
   requires the MSVC build tools. See the official [Rust installation guide](https://rust-lang.org/tools/install/)
   and [Microsoft's Rust setup guide](https://learn.microsoft.com/windows/dev-environment/rust/setup).

   If WinGet is available, install `rustup` from PowerShell:

   ```powershell
   winget install --id Rustlang.Rustup --exact
   winget install --id Git.Git --exact
   ```

2. Install `yt-dlp` and FFmpeg. Crest Player calls `yt-dlp`, `ffmpeg`, and
   `ffplay` by name, so all three executables must be available on `PATH`.

   ```powershell
   winget install yt-dlp
   winget install --id Gyan.FFmpeg --exact
   ```

   The `yt-dlp` project documents WinGet as a supported Windows installation
   method in its [installation guide](https://github.com/yt-dlp/yt-dlp/wiki/Installation).
   Close and reopen PowerShell after installing packages so `PATH` changes take
   effect.

3. Verify the required commands:

   ```powershell
   git --version
   cargo --version
   yt-dlp --version
   ffmpeg -version
   ffplay -version
   ```

4. Clone, build, and run Crest Player:

   ```powershell
   git clone https://github.com/ArvalCode/crest-player.git
   Set-Location crest-player
   cargo build --release
   .\target\release\crest-player.exe
   ```

   Windows Terminal is recommended for the best ANSI true-color and Unicode
   rendering. Increase the terminal window size if the wallpaper or video looks
   cramped.

### Windows data locations

- Settings: `%APPDATA%\crest-player\settings.json`
- Captured Home wallpaper: `%APPDATA%\crest-player\home-wallpaper.rgb`
- Downloaded library and its index: the current user's Music folder
- Temporary streamed audio and video: the current user's temporary directory

### Current Windows limitation

Native Windows playback, searching, downloading, video rendering, seeking, and
skipping are supported. Pause/resume with `Ctrl+P` currently relies on Unix
process signals, so that shortcut does not suspend `ffplay` correctly in a
native Windows session. Running Crest Player inside WSL with Linux audio support
uses the Linux behavior.

## Uninstalling

To completely remove Crest Player from your system:

1. Remove the installed binary:
   ```sh
   sudo rm /usr/local/bin/crest-player
   ```
2. (Optional) Remove the build directory and source code if you no longer need them:
   ```sh
   rm -rf /home/arval/Documents/VSProjects/crest-player
   ```

On Windows, remove the cloned repository (and any copied
`crest-player.exe`). To reset application data as well, remove
`%APPDATA%\crest-player`; downloaded songs remain in the user's Music folder
unless removed separately.

## License
MIT
