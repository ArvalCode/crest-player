# Crest Player

A lightweight Rust application to search and play music from YouTube. Designed for efficiency and minimal resource usage on Linux systems (e.g., Arch Linux).

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
4. Run the application:
   ```sh
   cargo run --release
   ```

## Planned
- Command-line interface for searching and playing music
- Efficient audio streaming using minimal libraries

## License
MIT
