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
- Display synchronized lyrics with optional Japanese romaji.
- Fall back to timed manual or automatic YouTube video captions when the lyrics
  service has no result.
- Show available Latin-letter pronunciation lines alongside original lyrics with
  the default-on **English Pronunciations** Home setting.
- Overlay the current synchronized lyric and a preview of the next line throughout
  Idle, Ambient, and Cinema without resizing the video area.
- Seek, pause, resume, and skip queued tracks.
- Enter a staged YouTube music-video screensaver while music is playing:
  - **Idle** after 5 seconds.
  - **Ambient** after 15 seconds.
  - **Cinema** after 30 seconds.
- Render video as fast color ASCII, detailed dithered ASCII, or ANSI true-color half-block pixels.
- Decode and present video at a selectable steady 15, 30, or 60 FPS, synchronized to the audio clock.
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

## Controls

| Input | Action |
| --- | --- |
| Arrow keys | Navigate results and library entries |
| `Enter` | Search, play, or activate a Home option |
| `Backspace` | Delete the previous character while entering a search |
| `Ctrl+A` | Add the selected track to the queue |
| `Ctrl+L` | Download/save the selected track |
| `Ctrl+P` | Pause or resume |
| `Ctrl+N` | Skip to the next queued track |
| `Alt++` / `Alt+-` | Seek forward/backward five seconds |
| `V` | Toggle the library panel |
| `Esc` | Clear results and return to search |
| `Home` (`Fn+Left Arrow` on compact keyboards) | Return to Home |
| `Ctrl+Q` | Quit |

While the screensaver is visible, the first intentional input restores the normal
interface and is consumed so it cannot accidentally activate a control. Playback
shortcuts (`Ctrl+P`, `Ctrl+N`, `Alt++`, and `Alt+-`) operate without leaving the
Idle, Ambient, or Cinema view.

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

## License
MIT
