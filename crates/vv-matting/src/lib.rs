//! Automatic background removal: Robust Video Matting (MobileNetV3) on the
//! CPU through tract, writing one matte per media frame into
//! `vv_media::matte`. The model keeps a recurrent state from frame to frame,
//! so the clip is cut into stretches matted in parallel, each warmed up on a
//! few frames before its start.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use tract_onnx::prelude::*;
use vv_core::FrameIdx;
use vv_media::{ColorMatrix, Decoder, FrameYuv420, matte};

const MODEL_FILE: &str = "rvm_mobilenetv3_fp32.onnx";
const MODEL_URL: &str = "https://github.com/PeterL1n/RobustVideoMatting/releases/download/v1.0.0/rvm_mobilenetv3_fp32.onnx";
const MODEL_BYTES: u64 = 14_975_696;
pub const MODEL_MB: u32 = 15;
/// Longest side the frames are matted at: the model refines the edges at
/// this size from an encoder pass at half of it.
const MATTE_SIDE: u32 = 960;
const DOWNSAMPLE: f32 = 0.5;
/// Frames run before a stretch starts, so its state has settled.
const WARMUP_FRAMES: FrameIdx = 8;
const STATE_CHANNELS: [usize; 4] = [16, 20, 40, 64];

#[derive(Debug)]
pub enum Error {
    Cancelled,
    Download(String),
    Model(String),
    Media(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Download(e) | Self::Model(e) | Self::Media(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stage {
    Waiting = 0,
    Download = 1,
    Matting = 2,
}

/// Shared with the UI thread, which polls it.
#[derive(Default)]
pub struct Progress {
    stage: AtomicU8,
    done: AtomicU64,
    total: AtomicU64,
}

impl Progress {
    pub fn stage(&self) -> Stage {
        match self.stage.load(Ordering::Relaxed) {
            1 => Stage::Download,
            2 => Stage::Matting,
            _ => Stage::Waiting,
        }
    }

    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.done.load(Ordering::Relaxed) as f64 / total as f64).min(1.0) as f32
    }

    fn enter(&self, stage: Stage, total: u64) {
        self.stage.store(stage as u8, Ordering::Relaxed);
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }

    fn advance(&self, by: u64) {
        self.done.fetch_add(by, Ordering::Relaxed);
    }
}

pub fn model_present(dir: &Path) -> bool {
    std::fs::metadata(dir.join(MODEL_FILE)).is_ok_and(|m| m.len() == MODEL_BYTES)
}

/// Downloads the model into `dir` unless it is already there, under a
/// temporary name until complete.
pub fn ensure_model(
    dir: &Path,
    progress: &Progress,
    cancel: &AtomicBool,
) -> Result<PathBuf, Error> {
    let path = dir.join(MODEL_FILE);
    if model_present(dir) {
        return Ok(path);
    }
    progress.enter(Stage::Download, MODEL_BYTES);
    let fail = |e: &dyn std::fmt::Display| Error::Download(e.to_string());
    std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    let part = path.with_extension("part");
    let result = (|| {
        let mut response = ureq::get(MODEL_URL).call().map_err(|e| fail(&e))?;
        let mut reader = response.body_mut().as_reader();
        let mut file = std::fs::File::create(&part).map_err(|e| fail(&e))?;
        let mut buffer = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let read = reader.read(&mut buffer).map_err(|e| fail(&e))?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).map_err(|e| fail(&e))?;
            done += read as u64;
            progress.advance(read as u64);
        }
        file.sync_all().map_err(|e| fail(&e))?;
        if done != MODEL_BYTES {
            return Err(Error::Download(format!(
                "incomplete download ({done} of {MODEL_BYTES} bytes)"
            )));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result?;
    std::fs::rename(&part, &path).map_err(|e| fail(&e))?;
    Ok(path)
}

/// Size the frames are decoded at: the media's within `MATTE_SIDE`, aspect kept.
pub fn matte_size(width: u32, height: u32) -> (u32, u32) {
    let scale = (MATTE_SIDE as f64 / width.max(height).max(1) as f64).min(1.0);
    (
        ((width as f64 * scale).round() as u32).max(2),
        ((height as f64 * scale).round() as u32).max(2),
    )
}

/// The model's input: `size` padded to a multiple of 32 (16 at the
/// downsampled encoder, the depth of its strides).
fn padded(size: (u32, u32)) -> (usize, usize) {
    let up = |v: u32| (v as usize).div_ceil(32) * 32;
    (up(size.0), up(size.1))
}

type Plan = std::sync::Arc<TypedSimplePlan>;

fn state_shapes(input: (usize, usize)) -> Vec<[usize; 4]> {
    let (w, h) = (
        (input.0 as f32 * DOWNSAMPLE) as usize,
        (input.1 as f32 * DOWNSAMPLE) as usize,
    );
    STATE_CHANNELS
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let stride = 2usize << i;
            [1, c, h.div_ceil(stride), w.div_ceil(stride)]
        })
        .collect()
}

fn plan(model_path: &Path, input: (usize, usize)) -> Result<Plan, Error> {
    let fail = |e: TractError| Error::Model(e.to_string());
    let (w, h) = input;
    let mut model = tract_onnx::onnx()
        .model_for_path(model_path)
        .map_err(fail)?;
    model
        .set_input_fact(0, f32::fact([1, 3, h, w]).into())
        .map_err(fail)?;
    for (i, shape) in state_shapes(input).iter().enumerate() {
        model
            .set_input_fact(i + 1, f32::fact(*shape).into())
            .map_err(fail)?;
    }
    model
        .set_input_fact(5, InferenceFact::from(tensor1(&[DOWNSAMPLE])))
        .map_err(fail)?;
    let runnable = model
        .into_optimized()
        .and_then(|m| m.into_runnable())
        .map_err(fail)?;
    Ok(runnable)
}

/// RGB in 0..1, planar, `frame` copied into the top-left of `input` and its
/// last row and column repeated over the padding.
fn frame_tensor(frame: &FrameYuv420, input: (usize, usize)) -> Tensor {
    let (pw, ph) = input;
    let (w, h) = (frame.width as usize, frame.height as usize);
    let mut data = vec![0f32; 3 * pw * ph];
    let (kr, kb) = match frame.matrix {
        ColorMatrix::Bt709 => (0.2126f32, 0.0722f32),
        ColorMatrix::Bt2020 => (0.2627, 0.0593),
        _ => (0.299, 0.114),
    };
    let kg = 1.0 - kr - kb;
    let (y_scale, y_off, c_scale) = if frame.full_range {
        (1.0 / 255.0, 0.0, 1.0 / 255.0)
    } else {
        (1.0 / 219.0, 16.0, 1.0 / 224.0)
    };
    let plane = pw * ph;
    for py in 0..ph {
        let y = py.min(h - 1);
        for px in 0..pw {
            let x = px.min(w - 1);
            let luma = (f32::from(frame.y[y * w + x]) - y_off) * y_scale;
            let (u, v) = frame.chroma_at(
                (x / 2).min(frame.chroma_width as usize - 1),
                (y / 2).min(frame.chroma_height as usize - 1),
            );
            let cb = (f32::from(u) - 128.0) * c_scale;
            let cr = (f32::from(v) - 128.0) * c_scale;
            let r = luma + 2.0 * (1.0 - kr) * cr;
            let b = luma + 2.0 * (1.0 - kb) * cb;
            let g = (luma - kr * r - kb * b) / kg;
            let i = py * pw + px;
            data[i] = r.clamp(0.0, 1.0);
            data[plane + i] = g.clamp(0.0, 1.0);
            data[2 * plane + i] = b.clamp(0.0, 1.0);
        }
    }
    tract_ndarray::Array4::from_shape_vec((1, 3, ph, pw), data)
        .expect("the shape matches the data")
        .into_tensor()
}

struct Matter {
    plan: Plan,
    input: (usize, usize),
    states: Vec<Tensor>,
}

impl Matter {
    fn new(plan: Plan, input: (usize, usize)) -> Self {
        let mut matter = Self {
            plan,
            input,
            states: Vec::new(),
        };
        matter.reset();
        matter
    }

    fn reset(&mut self) {
        self.states = state_shapes(self.input)
            .iter()
            .map(|s| Tensor::zero::<f32>(s).expect("valid shape"))
            .collect();
    }

    /// The coverage of `frame`, at its own size.
    fn matte(&mut self, frame: &FrameYuv420) -> Result<matte::Matte, Error> {
        let mut inputs: TVec<TValue> = tvec![frame_tensor(frame, self.input).into()];
        inputs.extend(self.states.drain(..).map(TValue::from));
        inputs.push(tensor1(&[DOWNSAMPLE]).into());
        let outputs = self
            .plan
            .run(inputs)
            .map_err(|e| Error::Model(e.to_string()))?;
        self.states = outputs[2..6]
            .iter()
            .map(|t| t.clone().into_tensor())
            .collect();
        let alpha = outputs[1]
            .to_plain_array_view::<f32>()
            .map_err(|e| Error::Model(e.to_string()))?;
        let (w, h) = (frame.width as usize, frame.height as usize);
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                data.push((alpha[[0, 0, y, x]].clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        Ok(matte::Matte {
            width: frame.width,
            height: frame.height,
            data,
        })
    }
}

/// Mattes every frame of the video at `path` (`frames` of them at `fps`)
/// into the cache of `content_hash`. Frames already there are kept: an
/// interrupted job resumes.
pub fn matte_media(
    path: &Path,
    content_hash: u64,
    frames: FrameIdx,
    fps: f64,
    models_dir: &Path,
    progress: &Progress,
    cancel: &AtomicBool,
) -> Result<(), Error> {
    if matte::mattes_complete(content_hash) {
        return Ok(());
    }
    let model_path = ensure_model(models_dir, progress, cancel)?;
    let media = |e: vv_media::MediaError| Error::Media(e.to_string());
    let mut probe = Decoder::open(path).map_err(media)?;
    let size = matte_size(probe.width(), probe.height());
    probe.set_max_size(Some(size));
    let first = probe
        .next_frame()
        .map_err(media)?
        .ok_or_else(|| Error::Media("no video frames".into()))?;
    drop(probe);
    let input = padded((first.1.width, first.1.height));
    let plan = plan(&model_path, input)?;

    let frames = frames.max(1);
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get() / 2)
        .clamp(1, 8) as FrameIdx;
    let stretch = (frames + workers - 1) / workers;
    progress.enter(Stage::Matting, frames as u64);
    let failure: std::sync::Mutex<Option<Error>> = std::sync::Mutex::new(None);
    let failed = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for start in (0..frames).step_by(stretch.max(1) as usize) {
            let end = (start + stretch).min(frames);
            let (plan, failure, failed) = (plan.clone(), &failure, &failed);
            scope.spawn(move || {
                let stop = || cancel.load(Ordering::Relaxed) || failed.load(Ordering::Relaxed);
                let result = matte_stretch(
                    path,
                    content_hash,
                    (start, end),
                    fps,
                    size,
                    Matter::new(plan, input),
                    progress,
                    &stop,
                );
                if let Err(e) = result {
                    failed.store(true, Ordering::Relaxed);
                    failure.lock().unwrap().get_or_insert(e);
                }
            });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    if let Some(e) = failure.into_inner().unwrap() {
        return Err(e);
    }
    matte::mark_mattes_complete(content_hash).map_err(|e| Error::Media(e.to_string()))
}

fn matte_stretch(
    path: &Path,
    content_hash: u64,
    (start, end): (FrameIdx, FrameIdx),
    fps: f64,
    size: (u32, u32),
    mut matter: Matter,
    progress: &Progress,
    stop: &dyn Fn() -> bool,
) -> Result<(), Error> {
    let pending = (start..end)
        .filter(|&f| !matte::matte_frame_exists(content_hash, f))
        .count();
    progress.advance((end - start) as u64 - pending as u64);
    if pending == 0 {
        return Ok(());
    }
    let media = |e: vv_media::MediaError| Error::Media(e.to_string());
    let mut decoder = Decoder::open(path).map_err(media)?;
    decoder.set_max_size(Some(size));
    let warm_start = (start - WARMUP_FRAMES).max(0);
    if warm_start > 0 {
        decoder
            .seek_to_time(warm_start as f64 / fps.max(1e-9))
            .map_err(media)?;
    }
    matter.reset();
    while let Some((idx, frame)) = decoder.next_frame().map_err(media)? {
        if stop() {
            return Err(Error::Cancelled);
        }
        if idx >= end {
            break;
        }
        if idx < warm_start {
            continue;
        }
        let matte = matter.matte(&frame)?;
        if idx >= start && !matte::matte_frame_exists(content_hash, idx) {
            matte::write_matte(content_hash, idx, &matte)
                .map_err(|e| Error::Media(e.to_string()))?;
            progress.advance(1);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
