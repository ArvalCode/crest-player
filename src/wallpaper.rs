use crate::idle_mode::VideoRenderMode;
use crate::video_screensaver::VideoFrame;
use std::io;
use std::path::PathBuf;

const FILE_MAGIC: &[u8; 4] = b"CWP1";
const HEADER_LENGTH: usize = 9;
const FILE_NAME: &str = "home-wallpaper.rgb";

pub struct HomeWallpaper {
    pub frame: VideoFrame,
    pub render_mode: VideoRenderMode,
}

impl HomeWallpaper {
    pub fn capture(frame: &VideoFrame, render_mode: VideoRenderMode) -> Self {
        Self {
            frame: frame.clone(),
            render_mode,
        }
    }

    pub fn load() -> Option<Self> {
        let bytes = std::fs::read(wallpaper_path()?).ok()?;
        Self::decode(&bytes)
    }

    pub fn save(&self) -> io::Result<()> {
        let path = wallpaper_path()
            .ok_or_else(|| io::Error::other("configuration directory is unavailable"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.encode())
    }

    pub fn remove_saved() -> io::Result<()> {
        let Some(path) = wallpaper_path() else {
            return Ok(());
        };
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_LENGTH + self.frame.pixels.len());
        bytes.extend_from_slice(FILE_MAGIC);
        bytes.push(render_mode_id(self.render_mode));
        bytes.extend_from_slice(&self.frame.width.to_le_bytes());
        bytes.extend_from_slice(&self.frame.height.to_le_bytes());
        bytes.extend_from_slice(&self.frame.pixels);
        bytes
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER_LENGTH || &bytes[..FILE_MAGIC.len()] != FILE_MAGIC {
            return None;
        }
        let render_mode = render_mode_from_id(bytes[4]);
        let width = u16::from_le_bytes([bytes[5], bytes[6]]);
        let height = u16::from_le_bytes([bytes[7], bytes[8]]);
        let frame = VideoFrame::from_rgb(width, height, bytes[HEADER_LENGTH..].to_vec())?;
        Some(Self { frame, render_mode })
    }
}

fn wallpaper_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("crest-player").join(FILE_NAME))
}

fn render_mode_id(mode: VideoRenderMode) -> u8 {
    match mode {
        VideoRenderMode::AsciiFast => 0,
        VideoRenderMode::AsciiDetailed => 1,
        VideoRenderMode::ColorPixels => 2,
    }
}

fn render_mode_from_id(id: u8) -> VideoRenderMode {
    match id {
        1 => VideoRenderMode::AsciiDetailed,
        2 => VideoRenderMode::ColorPixels,
        _ => VideoRenderMode::AsciiFast,
    }
}

#[cfg(test)]
mod tests {
    use super::HomeWallpaper;
    use crate::idle_mode::VideoRenderMode;
    use crate::video_screensaver::VideoFrame;

    #[test]
    fn wallpaper_round_trips_through_its_binary_format() {
        let frame = VideoFrame::from_rgb(2, 1, vec![1, 2, 3, 4, 5, 6]).unwrap();
        let wallpaper = HomeWallpaper::capture(&frame, VideoRenderMode::AsciiDetailed);

        let decoded = HomeWallpaper::decode(&wallpaper.encode()).unwrap();

        assert_eq!(decoded.frame.width, 2);
        assert_eq!(decoded.frame.height, 1);
        assert_eq!(decoded.frame.pixels, frame.pixels);
        assert_eq!(decoded.render_mode, VideoRenderMode::AsciiDetailed);
    }

    #[test]
    fn wallpaper_rejects_invalid_or_truncated_data() {
        assert!(HomeWallpaper::decode(b"not a wallpaper").is_none());
        assert!(HomeWallpaper::decode(b"CWP1\0\x01\0\x01\0").is_none());
    }
}
