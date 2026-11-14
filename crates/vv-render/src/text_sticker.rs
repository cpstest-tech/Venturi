use crate::text::{TextMask, TitleLayer, TitleRender, title_metrics};
use crate::text_anim_render::{Part, SpriteMask, Xf};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use vv_core::{HAnchor, Rgba, TitleParams, VAnchor};

type Frame = Vec<(Arc<TextMask>, Rgba)>;

fn fire_frames() -> &'static [Frame] {
    static FRAMES: OnceLock<Vec<Frame>> = OnceLock::new();
    FRAMES.get_or_init(|| {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!(
            "../assets/text-stickers/onfire.png"
        )));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().expect("bundled sticker PNG");
        let mut pixels = vec![0; reader.output_buffer_size().expect("sticker size")];
        let info = reader
            .next_frame(&mut pixels)
            .expect("bundled sticker pixels");
        let metadata: serde_json::Value =
            serde_json::from_str(include_str!("../assets/text-stickers/onfire.json"))
                .expect("bundled sticker frames");
        metadata["frames"]
            .as_array()
            .expect("frame list")
            .iter()
            .map(|frame| {
                let rect = &frame["frame"];
                let number = |key: &str| rect[key].as_f64().expect("frame coordinate") as u32;
                let (w, h, ox, oy) = (number("w"), number("h"), number("x"), number("y"));
                let mut masks = BTreeMap::new();
                for y in 0..h {
                    for x in 0..w {
                        let at = (((y + oy) * info.width + x + ox) * 4) as usize;
                        let pixel = &pixels[at..at + 4];
                        if pixel[3] == 0 {
                            continue;
                        }
                        let key = (pixel[0] / 64, pixel[1] / 64, pixel[2] / 128);
                        let mask = masks.entry(key).or_insert_with(|| TextMask::empty(w, h));
                        mask.data[(y * w + x) as usize] = pixel[3];
                    }
                }
                masks
                    .into_iter()
                    .map(|((r, g, b), mask)| {
                        (
                            Arc::new(mask),
                            Rgba {
                                r: (f32::from(r) * 64.0 + 31.5) / 255.0,
                                g: (f32::from(g) * 64.0 + 31.5) / 255.0,
                                b: (f32::from(b) * 128.0 + 63.5) / 255.0,
                                a: 1.0,
                            },
                        )
                    })
                    .collect()
            })
            .collect()
    })
}

pub(crate) fn append(
    render: &mut TitleRender,
    params: &TitleParams,
    timeline: (u32, u32),
    output: (u32, u32),
) {
    let Some(sticker) = &params.style.sticker else {
        return;
    };
    if sticker.asset != "capcut_onfire" {
        return;
    }
    let elapsed = params.anim_state.map_or(1.5, |s| s.t_loop);
    if elapsed < sticker.start {
        return;
    }
    let frames = fire_frames();
    let frame = &frames[((elapsed - sticker.start) * 30.0) as usize % frames.len()];
    let scale = (output.0 as f32 / timeline.0.max(1) as f32)
        .min(output.1 as f32 / timeline.1.max(1) as f32);
    let font = params.size * scale;
    let (bw, bh) = title_metrics(params).block;
    let mut center = (
        output.0 as f32 * 0.5 + params.position[0] * scale,
        output.1 as f32 * 0.5 - params.position[1] * scale,
    );
    center.0 += match params.anchor.0 {
        HAnchor::Left => bw * 0.5 * scale,
        HAnchor::Right => -bw * 0.5 * scale,
        HAnchor::Center => 0.0,
    };
    center.1 += match params.anchor.1 {
        VAnchor::Top => bh * 0.5 * scale,
        VAnchor::Bottom => -bh * 0.5 * scale,
        VAnchor::Middle => 0.0,
    };
    let target = (
        center.0 + sticker.offset[0] * font,
        center.1 - sticker.offset[1] * font,
    );
    for (mask, color) in frame {
        let pivot = (mask.width as f32 * 0.5, mask.height as f32 * 0.5);
        let part = Part {
            src: SpriteMask {
                mask: mask.clone(),
                origin: (0, 0),
                step: 1,
            },
            xf: Xf {
                scale: [
                    sticker.size[0] * font / mask.width as f32,
                    sticker.size[1] * font / mask.height as f32,
                ],
                rotation: sticker.rotation.to_radians(),
                shift: (target.0 - pivot.0, target.1 - pivot.1),
                pivot,
            },
            alpha: params.color.a,
            sigma: 0.0,
            reveal: None,
            ramp: None,
        };
        render.layers.push(TitleLayer::Parts(vec![part], *color));
    }
}

#[cfg(test)]
#[path = "tests/text_sticker.rs"]
mod tests;
