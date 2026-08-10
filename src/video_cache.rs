use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
};

const MAGIC: &[u8; 8] = b"CRESTV1\0";
pub struct VideoCache {
    file: File,
    pub width: u16,
    pub height: u16,
    pub fps: u16,
    offsets: Vec<u64>,
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
    let mut child = Command::new("ffmpeg")
        .args([
            "-loglevel",
            "error",
            "-i",
            video_path,
            "-an",
            "-vf",
            &filter,
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut output = File::create(temporary_path)?;
    output.write_all(MAGIC)?;
    output.write_all(&width.to_le_bytes())?;
    output.write_all(&height.to_le_bytes())?;
    output.write_all(&fps.to_le_bytes())?;
    output.write_all(&0u64.to_le_bytes())?;

    let frame_size = width as usize * height as usize * 3;
    let mut frame = vec![0; frame_size];
    let mut frame_count = 0u64;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("ffmpeg has no output"))?;
    loop {
        match stdout.read_exact(&mut frame) {
            Ok(()) => {
                let compressed = zstd::bulk::compress(&frame, 3)?;
                let length = u32::try_from(compressed.len())
                    .map_err(|_| io::Error::other("compressed frame is too large"))?;
                output.write_all(&length.to_le_bytes())?;
                output.write_all(&compressed)?;
                frame_count += 1;
            }
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error),
        }
    }
    let status = child.wait()?;
    if !status.success() || frame_count == 0 {
        return Err(io::Error::other("ffmpeg could not preprocess video"));
    }
    output.seek(SeekFrom::Start(14))?;
    output.write_all(&frame_count.to_le_bytes())?;
    output.sync_all()?;
    std::fs::rename(temporary_path, cache_path)
}

impl VideoCache {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a Crest video cache",
            ));
        }
        let width = read_u16(&mut file)?;
        let height = read_u16(&mut file)?;
        let fps = read_u16(&mut file)?;
        let frame_count = read_u64(&mut file)?;
        let mut offsets = Vec::with_capacity(frame_count.min(1_000_000) as usize);
        for _ in 0..frame_count {
            let offset = file.stream_position()?;
            let length = read_u32(&mut file)? as i64;
            offsets.push(offset);
            file.seek(SeekFrom::Current(length))?;
        }
        Ok(Self {
            file,
            width,
            height,
            fps,
            offsets,
        })
    }

    pub fn frame_count(&self) -> usize {
        self.offsets.len()
    }

    pub fn read_frame(&mut self, index: usize) -> io::Result<Vec<u8>> {
        let offset = *self.offsets.get(index).ok_or_else(|| {
            io::Error::new(io::ErrorKind::UnexpectedEof, "cache frame is unavailable")
        })?;
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
    use super::{MAGIC, VideoCache};
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
}
