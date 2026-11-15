//! Still images: IS-Net (DIS, general use) on one frame, stored as frame 0.

use super::*;

const INPUT_SIDE: usize = 1024;
/// Longest side of the stored matte: finer than the model, so the edges
/// it interpolates stay smooth when the image is shown large.
const MATTE_MAX_SIDE: u32 = 2048;
/// Samples per axis averaged into one input pixel of a large image.
const BOX_TAPS: usize = 3;

/// The model sees the image stretched to a square, flattened over white
/// where it is transparent; mean 0.5, std 1 as in its training.
fn image_tensor(frame: &FrameYuv420) -> Tensor {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let rgb = RgbReader::new(frame);
    let plane = INPUT_SIDE * INPUT_SIDE;
    let mut data = vec![0f32; 3 * plane];
    let (sx, sy) = (w as f32 / INPUT_SIDE as f32, h as f32 / INPUT_SIDE as f32);
    for py in 0..INPUT_SIDE {
        for px in 0..INPUT_SIDE {
            let mut sum = [0f32; 3];
            for ty in 0..BOX_TAPS {
                let y =
                    (((py as f32 + (ty as f32 + 0.5) / BOX_TAPS as f32) * sy) as usize).min(h - 1);
                for tx in 0..BOX_TAPS {
                    let x = (((px as f32 + (tx as f32 + 0.5) / BOX_TAPS as f32) * sx) as usize)
                        .min(w - 1);
                    let a = frame
                        .alpha
                        .as_ref()
                        .map_or(1.0, |a| f32::from(a[y * w + x]) / 255.0);
                    for (s, c) in sum.iter_mut().zip(rgb.at(x, y)) {
                        *s += c * a + (1.0 - a);
                    }
                }
            }
            let i = py * INPUT_SIDE + px;
            for (c, s) in sum.into_iter().enumerate() {
                data[c * plane + i] = s / (BOX_TAPS * BOX_TAPS) as f32 - 0.5;
            }
        }
    }
    tract_ndarray::Array4::from_shape_vec((1, 3, INPUT_SIDE, INPUT_SIDE), data)
        .expect("the shape matches the data")
        .into_tensor()
}

pub(crate) fn stored_size(width: u32, height: u32) -> (u32, u32) {
    let scale = (MATTE_MAX_SIDE as f64 / width.max(height).max(1) as f64).min(1.0);
    (
        ((width as f64 * scale).round() as u32).max(1),
        ((height as f64 * scale).round() as u32).max(1),
    )
}

/// `prediction` (square, side `side`) stretched back over `size`, rescaled
/// to its own min..max and cut by the image's own transparency.
pub(crate) fn resample(
    prediction: &[f32],
    side: usize,
    size: (u32, u32),
    frame: &FrameYuv420,
) -> matte::Matte {
    let (lo, hi) = prediction
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let range = (hi - lo).max(1e-6);
    let at = |x: usize, y: usize| (prediction[y * side + x] - lo) / range;
    let (mw, mh) = (size.0 as usize, size.1 as usize);
    let (fw, fh) = (frame.width as usize, frame.height as usize);
    let mut data = Vec::with_capacity(mw * mh);
    for y in 0..mh {
        let fy = ((y as f32 + 0.5) / mh as f32 * side as f32 - 0.5).clamp(0.0, (side - 1) as f32);
        let (y0, ty) = (fy as usize, fy.fract());
        let y1 = (y0 + 1).min(side - 1);
        for x in 0..mw {
            let fx =
                ((x as f32 + 0.5) / mw as f32 * side as f32 - 0.5).clamp(0.0, (side - 1) as f32);
            let (x0, tx) = (fx as usize, fx.fract());
            let x1 = (x0 + 1).min(side - 1);
            let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
            let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
            let mut value = top * (1.0 - ty) + bottom * ty;
            if let Some(alpha) = &frame.alpha {
                let (ax, ay) = ((x * fw / mw).min(fw - 1), (y * fh / mh).min(fh - 1));
                value *= f32::from(alpha[ay * fw + ax]) / 255.0;
            }
            data.push((value.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    matte::Matte {
        width: size.0,
        height: size.1,
        data,
    }
}

/// Mattes the image at `path` into frame 0 of the cache of `content_hash`.
pub fn matte_image(
    path: &Path,
    content_hash: u64,
    models_dir: &Path,
    progress: &Progress,
    cancel: &AtomicBool,
) -> Result<(), Error> {
    if matte::mattes_complete(content_hash) {
        return Ok(());
    }
    let model_path = ensure_model(models_dir, &IMAGE_MODEL, progress, cancel)?;
    let media = |e: vv_media::MediaError| Error::Media(e.to_string());
    let (_, frame) = Decoder::open_image(path)
        .and_then(|mut d| d.next_frame())
        .map_err(media)?
        .ok_or_else(|| Error::Media("no image".into()))?;
    progress.enter(Stage::Matting, 1);
    let fail = |e: TractError| Error::Model(e.to_string());
    let mut model = tract_onnx::onnx()
        .model_for_path(&model_path)
        .map_err(fail)?;
    model
        .set_input_fact(0, f32::fact([1, 3, INPUT_SIDE, INPUT_SIDE]).into())
        .map_err(fail)?;
    let plan = model
        .into_optimized()
        .and_then(|m| m.into_runnable())
        .map_err(fail)?;
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let outputs = plan.run(tvec![image_tensor(&frame).into()]).map_err(fail)?;
    let prediction = outputs[0].to_plain_array_view::<f32>().map_err(fail)?;
    let prediction: Vec<f32> = prediction.iter().copied().collect();
    let size = stored_size(frame.width, frame.height);
    let matte = resample(&prediction, INPUT_SIDE, size, &frame);
    let io = |e: std::io::Error| Error::Media(e.to_string());
    matte::write_matte(content_hash, 0, &matte).map_err(io)?;
    progress.advance(1);
    matte::mark_mattes_complete(content_hash).map_err(io)
}

#[cfg(test)]
#[path = "tests/image.rs"]
mod tests;
