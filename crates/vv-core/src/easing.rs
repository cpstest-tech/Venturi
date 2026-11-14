//! Shared easing library: keyframe interpolation and text animations use
//! the same curves. Every curve maps 0 to 0 and 1 to 1; back, elastic,
//! bounce and spring may leave `0..=1` in between.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Easing {
    #[default]
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    BackIn,
    BackOut,
    BackInOut,
    ElasticIn,
    ElasticOut,
    ElasticInOut,
    BounceIn,
    BounceOut,
    BounceInOut,
    Spring,
}

impl Easing {
    pub const ALL: [Self; 17] = [
        Self::Linear,
        Self::QuadIn,
        Self::QuadOut,
        Self::QuadInOut,
        Self::CubicIn,
        Self::CubicOut,
        Self::CubicInOut,
        Self::BackIn,
        Self::BackOut,
        Self::BackInOut,
        Self::ElasticIn,
        Self::ElasticOut,
        Self::ElasticInOut,
        Self::BounceIn,
        Self::BounceOut,
        Self::BounceInOut,
        Self::Spring,
    ];

    /// Stable id, also the locale key suffix.
    pub fn id(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::QuadIn => "quad_in",
            Self::QuadOut => "quad_out",
            Self::QuadInOut => "quad_in_out",
            Self::CubicIn => "cubic_in",
            Self::CubicOut => "cubic_out",
            Self::CubicInOut => "cubic_in_out",
            Self::BackIn => "back_in",
            Self::BackOut => "back_out",
            Self::BackInOut => "back_in_out",
            Self::ElasticIn => "elastic_in",
            Self::ElasticOut => "elastic_out",
            Self::ElasticInOut => "elastic_in_out",
            Self::BounceIn => "bounce_in",
            Self::BounceOut => "bounce_out",
            Self::BounceInOut => "bounce_in_out",
            Self::Spring => "spring",
        }
    }

    /// `t` is clamped to 0-1.
    pub fn ease(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::QuadIn => t * t,
            Self::QuadOut => t * (2.0 - t),
            Self::QuadInOut => in_out(t, |x| x * x),
            Self::CubicIn => t * t * t,
            Self::CubicOut => 1.0 - (1.0 - t).powi(3),
            Self::CubicInOut => in_out(t, |x| x * x * x),
            Self::BackIn => back_in(t),
            Self::BackOut => 1.0 - back_in(1.0 - t),
            Self::BackInOut => in_out(t, back_in),
            Self::ElasticIn => elastic_in(t),
            Self::ElasticOut => 1.0 - elastic_in(1.0 - t),
            Self::ElasticInOut => in_out(t, elastic_in),
            Self::BounceIn => 1.0 - bounce_out(1.0 - t),
            Self::BounceOut => bounce_out(t),
            Self::BounceInOut => in_out(t, |x| 1.0 - bounce_out(1.0 - x)),
            Self::Spring => spring(t),
        }
    }
}

/// Mirrors an ease-in `f` into a symmetric in-out.
fn in_out(t: f32, f: impl Fn(f32) -> f32) -> f32 {
    if t < 0.5 {
        f(2.0 * t) / 2.0
    } else {
        1.0 - f(2.0 * (1.0 - t)) / 2.0
    }
}

const BACK_OVERSHOOT: f32 = 1.70158;

fn back_in(t: f32) -> f32 {
    t * t * ((BACK_OVERSHOOT + 1.0) * t - BACK_OVERSHOOT)
}

fn elastic_in(t: f32) -> f32 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    -(2.0f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * (2.0 * PI / 3.0)).sin()
}

fn bounce_out(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984375
    }
}

/// Damped oscillation around 1, rescaled so it lands exactly on 1.
fn spring(t: f32) -> f32 {
    const DECAY: f32 = 6.0;
    const TURNS: f32 = 2.5;
    let raw = |x: f32| 1.0 - (-DECAY * x).exp() * (2.0 * PI * TURNS * x).cos();
    raw(t) / raw(1.0)
}

#[cfg(test)]
#[path = "tests/easing.rs"]
mod tests;
