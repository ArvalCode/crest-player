# Crest Player

A lightweight Rust application to search and play music from YouTube. Designed for efficiency and minimal resource usage on Linux systems (e.g., Arch Linux).
Only uses ~350MB of Ram comparing to the ~1.3 GB of Ram that Spotify uses!

## Features
- Search for music on YouTube
- Stream and play audio directly
- Minimal dependencies for lightweight performance

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

## Planned
- Command-line interface for searching and playing music
- Efficient audio streaming using minimal libraries

## License
MIT
