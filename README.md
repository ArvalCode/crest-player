# Crest Player

A lightweight terminal music player written in Rust. Crest Player searches YouTube,
plays local or downloaded music, displays synchronized lyrics, and turns into an
ASCII music-video display when left idle.

## Features

- Search YouTube and queue tracks without blocking the terminal interface.
- Stream audio through `yt-dlp` and `ffplay` or play a downloaded-only library.
- Download and save favorite tracks locally.
- Display synchronized lyrics with optional Japanese romaji.
- Seek, pause, resume, and skip queued tracks.
- Enter a staged YouTube music-video screensaver while music is playing:
  - **Idle** after 5 seconds.
  - **Ambient** after 15 seconds.
  - **Cinema** after 30 seconds.
- Render video as fast color ASCII, detailed dithered ASCII, or ANSI true-color half-block pixels.
- Decode and present video at a selectable steady 15, 30, or 60 FPS, synchronized to the audio clock.
- Disable the YouTube screensaver, select its rendering style, or change its FPS from Home.
- Wake on keyboard input, mouse clicks, or scrolling. Mouse movement alone is ignored.
- Fall back to a procedural terminal animation while video loads or is unavailable.

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
