use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    process::{Command, Stdio},
};

const MAGIC: &[u8; 8] = b"CRESTV1\0";
const DELTA_MAGIC: &[u8; 8] = b"CRESTV2\0";
pub struct VideoCache {
    file: File,
    pub width: u16,
    pub height: u16,
    pub fps: u16,
    frames: Vec<CachedFrame>,
    delta_encoded: bool,
    decoded_index: Option<usize>,
    decoded_frame: Vec<u8>,
}

struct CachedFrame {
    offset: u64,
    keyframe: bool,
}

pub fn build_video_cache(
    video_path: &str,
    cache_path: &str,
    width: u16,
    height: u16,
    fps: u16,
) -> io::Result<()> {
    if width == 0 || height == 0 || fps == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid cache dimensions",
        ));
    }
    let temporary_path = format!("{cache_path}.part");
    let result =
        build_video_cache_inner(video_path, cache_path, &temporary_path, width, height, fps);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
        let _ = std::fs::remove_file(cache_path);
    }
    result
}

fn build_video_cache_inner(
    video_path: &str,
    cache_path: &str,
    temporary_path: &str,
    width: u16,
    height: u16,
    fps: u16,
) -> io::Result<()> {
    let filter = format!(
        "fps={fps},scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:black"
    );
    let keyframe_interval = u32::from(fps) * 10;
    let keyframe_interval = keyframe_interval.to_string();
    // V3 caches use a conventional inter-frame codec on disk. Ten-second GOPs
    // keep seeking bounded while the screensaver decodes ahead into its frame
    // ring before presentation. At 700 kbps, a four-minute cache is about 21 MB.
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-i",
            video_path,
            "-an",
            "-vf",
            &filter,
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-tune",
            "fastdecode",
            "-b:v",
            "700k",
            "-maxrate",
            "900k",
            "-bufsize",
            "1400k",
            "-g",
            &keyframe_interval,
            "-keyint_min",
            &keyframe_interval,
            "-sc_threshold",
            "0",
            "-bf",
            "0",
            "-pix_fmt",
            "yuv420p",
            "-f",
            "matroska",
            temporary_path,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success()
        || std::fs::metadata(temporary_path)
            .map(|metadata| metadata.len() == 0)
            .unwrap_or(true)
    {
        return Err(io::Error::other("ffmpeg could not preprocess video"));
    }
    std::fs::rename(temporary_path, cache_path)
}

impl VideoCache {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        let delta_encoded = &magic == DELTA_MAGIC;
        if &magic != MAGIC && !delta_encoded {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a Crest video cache",
            ));
        }
        let width = read_u16(&mut file)?;
        let height = read_u16(&mut file)?;
        let fps = read_u16(&mut file)?;
        let frame_count = read_u64(&mut file)?;
        let mut frames = Vec::with_capacity(frame_count.min(1_000_000) as usize);
        for _ in 0..frame_count {
            let keyframe = if delta_encoded {
                let mut flag = [0];
                file.read_exact(&mut flag)?;
                flag[0] != 0
            } else {
                true
            };
            let offset = file.stream_position()?;
            let length = read_u32(&mut file)? as i64;
            frames.push(CachedFrame { offset, keyframe });
            file.seek(SeekFrom::Current(length))?;
        }
        Ok(Self {
            file,
            width,
            height,
            fps,
            frames,
            delta_encoded,
            decoded_index: None,
            decoded_frame: Vec::new(),
        })
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn read_frame(&mut self, index: usize) -> io::Result<Vec<u8>> {
        self.frames.get(index).ok_or_else(|| {
            io::Error::new(io::ErrorKind::UnexpectedEof, "cache frame is unavailable")
        })?;
        if !self.delta_encoded {
            return self.decode_payload(index);
        }

        let start = if self
            .decoded_index
            .is_some_and(|decoded| decoded + 1 == index)
        {
            index
        } else {
            self.decoded_frame.clear();
            self.decoded_index = None;
            (0..=index)
                .rev()
                .find(|candidate| self.frames[*candidate].keyframe)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing keyframe"))?
        };
        for frame_index in start..=index {
            let payload = self.decode_payload(frame_index)?;
            if self.frames[frame_index].keyframe {
                self.decoded_frame = payload;
            } else {
                if self.decoded_frame.len() != payload.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid delta frame",
                    ));
                }
                for (pixel, difference) in self.decoded_frame.iter_mut().zip(payload) {
                    *pixel = pixel.wrapping_add(difference);
                }
            }
            self.decoded_index = Some(frame_index);
        }
        Ok(self.decoded_frame.clone())
    }

    fn decode_payload(&mut self, index: usize) -> io::Result<Vec<u8>> {
        let offset = self.frames[index].offset;
        self.file.seek(SeekFrom::Start(offset))?;
        let length = read_u32(&mut self.file)? as usize;
        let mut compressed = vec![0; length];
        self.file.read_exact(&mut compressed)?;
        zstd::bulk::decompress(&compressed, self.width as usize * self.height as usize * 3)
    }
}

fn read_u16(reader: &mut impl Read) -> io::Result<u16> {
    let mut bytes = [0; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::{DELTA_MAGIC, MAGIC, VideoCache};
    use std::io::Write;

    #[test]
    fn reads_indexed_compressed_frames() {
        let path =
            std::env::temp_dir().join(format!("crest-cache-test-{}.crestvid", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(MAGIC).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&15u16.to_le_bytes()).unwrap();
        file.write_all(&1u64.to_le_bytes()).unwrap();
        let compressed = zstd::bulk::compress(&[10, 20, 30], 3).unwrap();
        file.write_all(&(compressed.len() as u32).to_le_bytes())
            .unwrap();
        file.write_all(&compressed).unwrap();
        drop(file);

        let mut cache = VideoCache::open(&path).unwrap();
        assert_eq!(cache.frame_count(), 1);
        assert_eq!(cache.read_frame(0).unwrap(), vec![10, 20, 30]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reconstructs_delta_frames_and_supports_seeking() {
        let path = std::env::temp_dir().join(format!(
            "crest-delta-cache-test-{}.crestvid",
            std::process::id()
        ));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(DELTA_MAGIC).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&15u16.to_le_bytes()).unwrap();
        file.write_all(&2u64.to_le_bytes()).unwrap();
        for (keyframe, payload) in [(true, [10, 20, 30]), (false, [1, 2, 3])] {
            let compressed = zstd::bulk::compress(&payload, 3).unwrap();
            file.write_all(&[u8::from(keyframe)]).unwrap();
            file.write_all(&(compressed.len() as u32).to_le_bytes())
                .unwrap();
            file.write_all(&compressed).unwrap();
        }
        drop(file);

        let mut cache = VideoCache::open(&path).unwrap();
        assert_eq!(cache.read_frame(1).unwrap(), vec![11, 22, 33]);
        assert_eq!(cache.read_frame(0).unwrap(), vec![10, 20, 30]);
        assert_eq!(cache.read_frame(1).unwrap(), vec![11, 22, 33]);
        let _ = std::fs::remove_file(path);
    }
}
