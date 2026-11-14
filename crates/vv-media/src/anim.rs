//! Animated stickers: reading an animated WebP into RGBA frames and writing
//! frames as a looping WebM (VP9 with alpha), which the decoder already plays
//! like any other clip.

use ffmpeg::Dictionary;
use ffmpeg::codec::{self, encoder};
use ffmpeg::format::{self, Pixel};
use ffmpeg::software::scaling::{context::Context as Scaler, flag::Flags};
use ffmpeg_next as ffmpeg;
use std::path::Path;

/// Frames of an animation, straight RGBA, with how long each one shows.
pub struct Animation {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Vec<u8>>,
    pub delays: Vec<f64>,
}

fn rgba_frame(
    frame: &ffmpeg::frame::Video,
    scaler: &mut Scaler,
) -> Result<Vec<u8>, crate::MediaError> {
    let mut rgba = ffmpeg::frame::Video::empty();
    scaler.run(frame, &mut rgba)?;
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let stride = rgba.stride(0);
    let data = rgba.data(0);
    let mut out = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        out.extend_from_slice(&data[row * stride..row * stride + w * 4]);
    }
    Ok(out)
}

/// Decodes every frame of an animated image (WebP, GIF).
pub fn decode_animation(path: &Path) -> Result<Animation, crate::MediaError> {
    crate::probe::ensure_init();
    let mut input = format::input(&path)?;
    let stream = crate::probe::video_stream(&input)
        .ok_or_else(|| crate::MediaError::NoStream(path.display().to_string()))?;
    let index = stream.index();
    let time_base = f64::from(stream.time_base());
    let mut decoder = codec::context::Context::from_parameters(stream.parameters())?
        .decoder()
        .video()?;
    let (width, height) = (decoder.width(), decoder.height());
    let mut scaler: Option<Scaler> = None;
    let mut frames = Vec::new();
    let mut times = Vec::new();
    let mut take = |decoder: &mut ffmpeg::decoder::Video| -> Result<(), crate::MediaError> {
        let mut raw = ffmpeg::frame::Video::empty();
        while decoder.receive_frame(&mut raw).is_ok() {
            let scaler = match &mut scaler {
                Some(s) => s,
                None => scaler.insert(Scaler::get(
                    raw.format(),
                    raw.width(),
                    raw.height(),
                    Pixel::RGBA,
                    raw.width(),
                    raw.height(),
                    Flags::BILINEAR,
                )?),
            };
            frames.push(rgba_frame(&raw, scaler)?);
            times.push(
                raw.pts()
                    .map_or(frames.len() as f64 - 1.0, |p| p as f64 * time_base),
            );
        }
        Ok(())
    };
    for (packet_stream, packet) in input.packets() {
        if packet_stream.index() == index {
            decoder.send_packet(&packet)?;
            take(&mut decoder)?;
        }
    }
    decoder.send_eof()?;
    take(&mut decoder)?;
    if frames.is_empty() {
        return Err(crate::MediaError::NoStream(path.display().to_string()));
    }
    let mut delays: Vec<f64> = times.windows(2).map(|w| (w[1] - w[0]).max(0.001)).collect();
    let last = delays.last().copied().unwrap_or(0.1);
    delays.push(last);
    Ok(Animation {
        width,
        height,
        frames,
        delays,
    })
}

/// The animation frame to show at each of the output frames of a loop at
/// `fps` repeated until it lasts at least `min_secs`.
pub fn loop_indices(delays: &[f64], fps: u32, min_secs: f64) -> Vec<usize> {
    let total: f64 = delays.iter().sum();
    if delays.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let frame_secs = 1.0 / f64::from(fps);
    let per_loop = (total / frame_secs).round().max(1.0) as usize;
    let loops = (min_secs / (per_loop as f64 * frame_secs)).ceil().max(1.0) as usize;
    let mut ends = Vec::with_capacity(delays.len());
    let mut acc = 0.0;
    for d in delays {
        acc += d;
        ends.push(acc);
    }
    let one_loop = (0..per_loop).map(|i| {
        let t = (i as f64 + 0.5) * frame_secs * total / (per_loop as f64 * frame_secs);
        ends.iter().position(|&e| t < e).unwrap_or(delays.len() - 1)
    });
    let one_loop: Vec<usize> = one_loop.collect();
    (0..loops).flat_map(|_| one_loop.iter().copied()).collect()
}

/// `rgba` (`width` wide) with transparent pixels added on the right and below.
fn padded(rgba: &[u8], width: u32, to: (u32, u32)) -> Vec<u8> {
    let (w, h) = (width as usize, rgba.len() / 4 / width.max(1) as usize);
    let mut out = vec![0u8; to.0 as usize * to.1 as usize * 4];
    for y in 0..h {
        let row = &rgba[y * w * 4..(y + 1) * w * 4];
        out[y * to.0 as usize * 4..y * to.0 as usize * 4 + w * 4].copy_from_slice(row);
    }
    out
}

/// Writes straight RGBA frames (`width` x `height`) as a WebM with alpha. An
/// odd size gets a transparent row or column: 4:2:0 needs even sizes.
pub fn encode_alpha_webm(
    path: &Path,
    width: u32,
    height: u32,
    fps: u32,
    frames: impl Iterator<Item = Vec<u8>>,
) -> Result<(), crate::MediaError> {
    crate::probe::ensure_init();
    let (source_width, source_height) = (width, height);
    let (width, height) = ((width + 1) & !1, (height + 1) & !1);
    let missing = |what: &str| crate::MediaError::NoStream(format!("{what} not available"));
    let mut octx = format::output_as(path, "webm")?;
    let global_header = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);
    let codec = encoder::find_by_name("libvpx-vp9").ok_or_else(|| missing("libvpx-vp9"))?;
    let ost = octx.add_stream(codec)?;
    let stream_index = ost.index();
    let time_base = ffmpeg::Rational::new(1, fps as i32);
    let mut ctx = codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()?;
    ctx.set_width(width);
    ctx.set_height(height);
    ctx.set_format(Pixel::YUVA420P);
    ctx.set_time_base(time_base);
    ctx.set_frame_rate(Some(ffmpeg::Rational::new(fps as i32, 1)));
    ctx.set_colorspace(ffmpeg::color::Space::SMPTE170M);
    ctx.set_color_range(ffmpeg::color::Range::MPEG);
    if global_header {
        ctx.set_flags(codec::Flags::GLOBAL_HEADER);
    }
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(8));
    ctx.set_threading(ffmpeg::threading::Config {
        kind: ffmpeg::threading::Type::Slice,
        count: threads,
    });
    let mut opts = Dictionary::new();
    opts.set("tile-columns", "2");
    opts.set("crf", "28");
    opts.set("b", "0");
    // Required for the alpha plane.
    opts.set("auto-alt-ref", "0");
    opts.set("row-mt", "1");
    opts.set("deadline", "good");
    opts.set("cpu-used", "3");
    opts.set("g", &fps.to_string());
    let mut encoder = ctx.open_with(opts)?;
    let mut ost = ost;
    ost.set_parameters(&encoder);
    octx.write_header()?;
    let ost_time_base = octx.stream(stream_index).unwrap().time_base();

    let mut scaler = Scaler::get(
        Pixel::RGBA,
        width,
        height,
        Pixel::YUVA420P,
        width,
        height,
        Flags::BILINEAR,
    )?;
    for (pts, rgba) in frames.enumerate() {
        let rgba = if (source_width, source_height) == (width, height) {
            rgba
        } else {
            padded(&rgba, source_width, (width, height))
        };
        let mut src = ffmpeg::frame::Video::new(Pixel::RGBA, width, height);
        crate::encode::fill_plane(&mut src, 0, &rgba, width as usize * 4);
        let mut yuva = ffmpeg::frame::Video::empty();
        scaler.run(&src, &mut yuva)?;
        yuva.set_pts(Some(pts as i64));
        encoder.send_frame(&yuva)?;
        crate::encode::drain_packets(
            &mut octx,
            &mut encoder,
            stream_index,
            time_base,
            ost_time_base,
        )?;
    }
    encoder.send_eof()?;
    crate::encode::drain_packets(
        &mut octx,
        &mut encoder,
        stream_index,
        time_base,
        ost_time_base,
    )?;
    octx.write_trailer()?;
    Ok(())
}

/// One loop of an animated sticker, small, for the tiles of the library.
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Straight RGBA, one loop.
    pub frames: Vec<Vec<u8>>,
}

fn to_rgba(frame: &crate::decode::FrameYuv420) -> Vec<u8> {
    let (kr, kb) = match frame.matrix {
        crate::decode::ColorMatrix::Bt709 => (0.2126f32, 0.0722f32),
        crate::decode::ColorMatrix::Bt2020 => (0.2627, 0.0593),
        _ => (0.299, 0.114),
    };
    let kg = 1.0 - kr - kb;
    let (y_off, y_scale, c_scale) = if frame.full_range {
        (0.0, 1.0, 1.0)
    } else {
        (16.0, 255.0 / 219.0, 255.0 / 224.0)
    };
    let (w, h) = (frame.width as usize, frame.height as usize);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let luma = (f32::from(frame.y[y * w + x]) - y_off) * y_scale;
            let (u, v) = frame.chroma_at(x / 2, y / 2);
            let cb = (f32::from(u) - 128.0) * c_scale;
            let cr = (f32::from(v) - 128.0) * c_scale;
            let r = luma + 2.0 * (1.0 - kr) * cr;
            let b = luma + 2.0 * (1.0 - kb) * cb;
            let g = (luma - kr * r - kb * b) / kg;
            let alpha = frame.alpha.as_ref().map_or(255, |a| a[y * w + x]);
            out.extend_from_slice(&[
                r.clamp(0.0, 255.0) as u8,
                g.clamp(0.0, 255.0) as u8,
                b.clamp(0.0, 255.0) as u8,
                alpha,
            ]);
        }
    }
    out
}

/// Length in frames of the loop that `frames` repeat (all of them if none).
fn loop_length(frames: &[Vec<u8>]) -> usize {
    let n = frames.len();
    let differs = |a: &[u8], b: &[u8]| {
        let total: u64 = a
            .iter()
            .zip(b)
            .map(|(x, y)| u64::from(x.abs_diff(*y)))
            .sum();
        total as f64 / a.len().max(1) as f64 > 3.0
    };
    (6..=n / 2)
        .find(|&p| (0..n - p).all(|i| !differs(&frames[i], &frames[i + p])))
        .unwrap_or(n)
}

/// Decodes a sticker WebM at about `size` pixels, one loop of it.
pub fn decode_preview(path: &Path, size: u32) -> Result<Preview, crate::MediaError> {
    let mut decoder = crate::decode::Decoder::open(path)?;
    decoder.set_max_size(Some((size, size)));
    let fps = decoder.fps().as_f64();
    let (mut width, mut height) = (0, 0);
    let mut frames = Vec::new();
    while let Some((_, frame)) = decoder.next_frame()? {
        (width, height) = (frame.width, frame.height);
        frames.push(to_rgba(&frame));
    }
    if frames.is_empty() {
        return Err(crate::MediaError::NoStream(path.display().to_string()));
    }
    frames.truncate(loop_length(&frames));
    Ok(Preview {
        width,
        height,
        fps,
        frames,
    })
}

#[cfg(test)]
#[path = "tests/anim.rs"]
mod tests;
