//! The first media inserted in the timeline defines the project's format:
//! native size (4K at most), effective orientation and a standard frame rate.

use std::path::Path;
use vv_core::{MediaMeta, Rational};

const MAX_LANDSCAPE: (u32, u32) = (3840, 2160);
pub(crate) const STANDARD_FPS: [i32; 8] = [24, 25, 30, 50, 60, 120, 144, 240];
const PHOTO_FPS: i32 = 30;
/// Relative slack of a ratio match: 1080x1088 still counts as 1:1.
const RATIO_TOLERANCE: f64 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ratio {
    Original,
    R16x9,
    R9x16,
    R1x1,
    R4x3,
}

impl Ratio {
    const PRESETS: [(Ratio, u32, u32); 4] = [
        (Ratio::R16x9, 16, 9),
        (Ratio::R9x16, 9, 16),
        (Ratio::R1x1, 1, 1),
        (Ratio::R4x3, 4, 3),
    ];

    pub(crate) const MENU: [Ratio; 5] = [
        Ratio::Original,
        Ratio::R16x9,
        Ratio::R9x16,
        Ratio::R1x1,
        Ratio::R4x3,
    ];

    pub(crate) fn label(self) -> Option<&'static str> {
        match self {
            Ratio::Original => None,
            Ratio::R16x9 => Some("16:9"),
            Ratio::R9x16 => Some("9:16"),
            Ratio::R1x1 => Some("1:1"),
            Ratio::R4x3 => Some("4:3"),
        }
    }

    /// New frame size for this ratio, keeping the longer side of `current`.
    /// `original` is the first media's size, used by `Original`.
    pub(crate) fn resolution(
        self,
        current: (u32, u32),
        original: Option<(u32, u32)>,
    ) -> (u32, u32) {
        let (w, h) = match self {
            Ratio::Original => return fit_to_max(original.unwrap_or(current)),
            Ratio::R16x9 => (16, 9),
            Ratio::R9x16 => (9, 16),
            Ratio::R1x1 => (1, 1),
            Ratio::R4x3 => (4, 3),
        };
        let long = current.0.max(current.1).max(2);
        let (num_long, num_short) = (w.max(h), w.min(h));
        let short = (f64::from(long) * f64::from(num_short) / f64::from(num_long)).round() as u32;
        let (long, short) = (long & !1, (short & !1).max(2));
        if w >= h { (long, short) } else { (short, long) }
    }

    pub(crate) fn of(size: (u32, u32)) -> Self {
        let actual = f64::from(size.0) / f64::from(size.1.max(1));
        Self::PRESETS
            .into_iter()
            .find(|&(_, w, h)| {
                let preset = f64::from(w) / f64::from(h);
                (actual - preset).abs() / preset <= RATIO_TOLERANCE
            })
            .map_or(Ratio::Original, |(ratio, _, _)| ratio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectFormat {
    pub resolution: (u32, u32),
    pub fps: Rational,
    #[allow(dead_code)]
    pub ratio: Ratio,
}

/// `rotation` is the clockwise turn the player must apply to show the media
/// upright: 0, 90, 180 or 270.
pub(crate) fn project_format(meta: &MediaMeta, rotation: u16) -> ProjectFormat {
    let resolution = fit_to_max(effective_size((meta.width, meta.height), rotation));
    let fps = if meta.is_image() {
        Rational::new(PHOTO_FPS, 1)
    } else {
        round_fps(meta.fps.as_f64())
    };
    ProjectFormat {
        resolution,
        fps,
        ratio: Ratio::of(resolution),
    }
}

pub(crate) fn effective_size(size: (u32, u32), rotation: u16) -> (u32, u32) {
    if rotation == 90 || rotation == 270 {
        (size.1, size.0)
    } else {
        size
    }
}

/// The nearest of `STANDARD_FPS`; the lower one on a tie.
pub(crate) fn round_fps(fps: f64) -> Rational {
    let nearest = STANDARD_FPS
        .into_iter()
        .min_by(|a, b| {
            let (da, db) = ((f64::from(*a) - fps).abs(), (f64::from(*b) - fps).abs());
            da.total_cmp(&db)
        })
        .unwrap_or(PHOTO_FPS);
    Rational::new(nearest, 1)
}

/// Scales down to fit 4K in the same orientation; sides stay even for the
/// encoders.
pub(crate) fn fit_to_max(size: (u32, u32)) -> (u32, u32) {
    let (w, h) = (size.0.max(2), size.1.max(2));
    let (max_w, max_h) = if w >= h {
        MAX_LANDSCAPE
    } else {
        (MAX_LANDSCAPE.1, MAX_LANDSCAPE.0)
    };
    let scale = (f64::from(max_w) / f64::from(w))
        .min(f64::from(max_h) / f64::from(h))
        .min(1.0);
    let even = |v: u32| (((f64::from(v) * scale).round() as u32) & !1).max(2);
    (even(w), even(h))
}

/// Clockwise turn and extra zoom that show a `source` frame upright in a
/// `canvas`, which the engine composes unrotated and fitted.
pub(crate) fn rotation_fit(source: (u32, u32), canvas: (u32, u32), rotation: u16) -> (f32, f32) {
    if rotation != 90 && rotation != 270 {
        return (f32::from(rotation), 1.0);
    }
    let (sw, sh) = (f64::from(source.0.max(1)), f64::from(source.1.max(1)));
    let (cw, ch) = (f64::from(canvas.0), f64::from(canvas.1));
    let fit = (cw / sw).min(ch / sh);
    let (fitted_w, fitted_h) = (sw * fit, sh * fit);
    let zoom = (cw / fitted_h).min(ch / fitted_w);
    (f32::from(rotation), zoom as f32)
}

/// Clockwise rotation recorded in the file (display matrix or `rotate` tag),
/// 0 if there is none or ffprobe is unavailable. The engine ignores it.
pub(crate) fn probe_rotation(path: &Path) -> u16 {
    let output = crate::spawn::no_console(&mut std::process::Command::new("ffprobe"))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream_tags=rotate:stream_side_data=rotation",
            "-of",
            "json",
        ])
        .arg(path)
        .output();
    match output {
        Ok(out) if out.status.success() => parse_rotation(&String::from_utf8_lossy(&out.stdout)),
        _ => 0,
    }
}

pub(crate) fn parse_rotation(json: &str) -> u16 {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return 0;
    };
    let Some(stream) = value["streams"].get(0) else {
        return 0;
    };
    // The display matrix counts counter-clockwise, the tag clockwise.
    let from_matrix = stream["side_data_list"]
        .as_array()
        .and_then(|list| list.iter().find_map(|d| d["rotation"].as_f64()))
        .map(|degrees| -degrees);
    let from_tag = stream["tags"]["rotate"]
        .as_str()
        .and_then(|s| s.trim().parse::<f64>().ok());
    match from_matrix.or(from_tag) {
        Some(degrees) => ((degrees / 90.0).round() as i32).rem_euclid(4) as u16 * 90,
        None => 0,
    }
}

#[cfg(test)]
#[path = "tests/first_media.rs"]
mod tests;
