//! Background-removal mattes: one grayscale PNG per media frame, in a global
//! on-disk cache keyed by `content_hash` like the proxies. Written by the
//! matting job, read by the frame provider as the frame's coverage.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use vv_core::FrameIdx;

/// Coverage at a resolution of its own: the compositor samples it by uv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matte {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

const COMPLETE_MARKER: &str = "complete";
/// Frames kept decoded: preview and render-ahead ask for the same ones.
const MEMORY_FRAMES: usize = 48;

pub fn matte_dir(content_hash: u64) -> PathBuf {
    crate::cache_dir("mattes").join(format!("{content_hash:016x}"))
}

fn frame_path(content_hash: u64, frame: FrameIdx) -> PathBuf {
    matte_dir(content_hash).join(format!("{frame:07}.png"))
}

pub fn matte_frame_exists(content_hash: u64, frame: FrameIdx) -> bool {
    frame_path(content_hash, frame).is_file()
}

/// Every frame of the media has its matte.
pub fn mattes_complete(content_hash: u64) -> bool {
    matte_dir(content_hash).join(COMPLETE_MARKER).is_file()
}

pub fn mark_mattes_complete(content_hash: u64) -> std::io::Result<()> {
    std::fs::write(matte_dir(content_hash).join(COMPLETE_MARKER), b"")
}

/// Written beside and renamed: a reader never sees half a file.
pub fn write_matte(content_hash: u64, frame: FrameIdx, matte: &Matte) -> std::io::Result<()> {
    let dir = matte_dir(content_hash);
    std::fs::create_dir_all(&dir)?;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, matte.width, matte.height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
        writer
            .write_image_data(&matte.data)
            .map_err(std::io::Error::other)?;
    }
    let path = frame_path(content_hash, frame);
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

type MemoryCache = Mutex<VecDeque<((u64, FrameIdx), Arc<Matte>)>>;

fn memory() -> &'static MemoryCache {
    static CACHE: std::sync::OnceLock<MemoryCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// The matte of `frame`, or of the frame just before it: a frame the
/// decoder skipped while matting (a timestamp gap) borrows its neighbour's.
pub fn load_matte(content_hash: u64, frame: FrameIdx) -> Option<Arc<Matte>> {
    (0..3)
        .map(|back| frame - back)
        .filter(|&f| f >= 0)
        .find_map(|f| load_exact(content_hash, f))
}

fn load_exact(content_hash: u64, frame: FrameIdx) -> Option<Arc<Matte>> {
    let key = (content_hash, frame);
    if let Some(hit) = memory()
        .lock()
        .ok()?
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, m)| m.clone())
    {
        return Some(hit);
    }
    let bytes = std::fs::read(frame_path(content_hash, frame)).ok()?;
    let matte = Arc::new(decode_png(&bytes)?);
    if let Ok(mut cache) = memory().lock() {
        if cache.len() >= MEMORY_FRAMES {
            cache.pop_front();
        }
        cache.push_back((key, matte.clone()));
    }
    Some(matte)
}

fn decode_png(bytes: &[u8]) -> Option<Matte> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    Some(Matte {
        width: info.width,
        height: info.height,
        data: buf,
    })
}

#[cfg(test)]
#[path = "tests/matte.rs"]
mod tests;
