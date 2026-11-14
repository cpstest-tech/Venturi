//! Built-in text animations, expressed as `AnimDescriptor` data: picking one
//! copies the descriptor into the title, so templates and projects keep it
//! even if these defaults change.

use crate::easing::Easing;
use crate::model::Rgba;
use crate::text_anim::{
    AnimDescriptor, AnimProp as P, Decor, DecorShape, Granularity as G, PropTrack as T,
    UnitOrder, Wave,
};

pub const IN_IDS: [&str; 14] = [
    "fade",
    "slide",
    "pop",
    "bounce",
    "typewriter",
    "slam",
    "mask_reveal",
    "wave",
    "blur_in",
    "flip_3d",
    "glitch",
    "underline",
    "scribble",
    "bar_expand",
];

/// The Out list mirrors the In list, plus these two.
pub const OUT_ONLY_IDS: [&str; 2] = ["dissolve", "fall"];

pub const LOOP_IDS: [&str; 8] = [
    "pulse",
    "float",
    "loop_wave",
    "shake",
    "glow_pulse",
    "shimmer",
    "flicker",
    "gradient_flow",
];

pub fn out_ids() -> Vec<&'static str> {
    IN_IDS.iter().chain(&OUT_ONLY_IDS).copied().collect()
}

const CYAN: Rgba = Rgba {
    r: 0.0,
    g: 0.9,
    b: 1.0,
    a: 1.0,
};
const ACCENT: Rgba = Rgba {
    r: 1.0,
    g: 0.85,
    b: 0.1,
    a: 1.0,
};

fn base(id: &str, duration: f32, granularity: G, easing: Easing) -> AnimDescriptor {
    AnimDescriptor {
        id: id.into(),
        duration,
        granularity,
        easing,
        ..AnimDescriptor::default()
    }
}

fn staggered(mut d: AnimDescriptor, stagger: f32) -> AnimDescriptor {
    d.stagger = stagger;
    d
}

fn with(mut d: AnimDescriptor, tracks: &[T]) -> AnimDescriptor {
    d.tracks = tracks.to_vec();
    d
}

fn decor(shape: DecorShape, color: Rgba, thickness: f32, offset: f32, from_center: bool) -> Decor {
    Decor {
        shape,
        color,
        thickness,
        offset,
        from_center,
    }
}

/// The In version of `id`; the Out version is the same descriptor with
/// `mirror` set (see `out_preset`).
pub fn in_preset(id: &str) -> Option<AnimDescriptor> {
    Some(match id {
        "fade" => with(
            base(id, 0.5, G::Block, Easing::QuadOut),
            &[T::new(P::Opacity, 0.0, 1.0)],
        ),
        "slide" => staggered(
            with(
                base(id, 0.7, G::Line, Easing::CubicOut),
                &[T::new(P::PosY, -0.6, 0.0), T::new(P::Opacity, 0.0, 1.0)],
            ),
            0.4,
        ),
        "pop" => staggered(
            with(
                base(id, 0.6, G::Word, Easing::BackOut),
                &[T::new(P::Scale, 0.2, 1.0), T::new(P::Opacity, 0.0, 1.0)],
            ),
            0.45,
        ),
        "bounce" => staggered(
            with(
                base(id, 1.0, G::Char, Easing::BounceOut),
                &[T::new(P::PosY, 1.4, 0.0), T::new(P::Opacity, 0.0, 1.0)],
            ),
            0.6,
        ),
        "typewriter" => staggered(
            with(
                base(id, 1.0, G::Char, Easing::Linear),
                &[T::new(P::Opacity, 0.0, 1.0)],
            ),
            0.92,
        ),
        "slam" => staggered(
            with(
                base(id, 0.45, G::Word, Easing::CubicIn),
                &[
                    T::new(P::Scale, 3.0, 1.0),
                    T::new(P::Opacity, 0.0, 1.0),
                    T::new(P::Blur, 0.25, 0.0),
                ],
            ),
            0.4,
        ),
        "mask_reveal" => staggered(
            with(
                base(id, 0.8, G::Line, Easing::CubicOut),
                &[T::new(P::Reveal, 0.0, 1.0), T::new(P::PosX, -0.15, 0.0)],
            ),
            0.35,
        ),
        "wave" => staggered(
            with(
                base(id, 1.1, G::Char, Easing::BackOut),
                &[T::new(P::PosY, -0.7, 0.0), T::new(P::Opacity, 0.0, 1.0)],
            ),
            0.7,
        ),
        "blur_in" => staggered(
            with(
                base(id, 0.9, G::Word, Easing::QuadOut),
                &[
                    T::new(P::Blur, 0.35, 0.0),
                    T::new(P::Opacity, 0.0, 1.0),
                    T::new(P::Scale, 1.2, 1.0),
                ],
            ),
            0.4,
        ),
        "flip_3d" => staggered(
            with(
                base(id, 0.9, G::Char, Easing::BackOut),
                &[T::new(P::TurnX, 90.0, 0.0), T::new(P::Opacity, 0.0, 1.0)],
            ),
            0.55,
        ),
        "glitch" => {
            let mut d = staggered(
                with(
                    base(id, 0.6, G::Char, Easing::Linear),
                    &[
                        T::new(P::PosX, 0.0, 0.0).jitter(0.25),
                        T::new(P::Opacity, 0.0, 1.0).jitter(0.6),
                        T::new(P::Tint, 1.0, 0.0).jitter(0.5),
                    ],
                ),
                0.6,
            );
            d.order = UnitOrder::Random;
            d.tint = CYAN;
            d
        }
        "underline" => {
            let mut d = with(
                base(id, 0.7, G::Block, Easing::CubicOut),
                &[T::new(P::Opacity, 0.0, 1.0)],
            );
            d.decor = Some(decor(DecorShape::Underline, ACCENT, 0.07, 0.12, false));
            d
        }
        "scribble" => {
            let mut d = with(
                base(id, 0.9, G::Block, Easing::QuadInOut),
                &[T::new(P::Opacity, 0.0, 1.0)],
            );
            d.decor = Some(decor(DecorShape::Scribble, ACCENT, 0.08, 0.2, false));
            d
        }
        "bar_expand" => {
            let mut d = with(
                base(id, 0.8, G::Line, Easing::CubicOut),
                &[T::new(P::Reveal, 0.0, 1.0)],
            );
            d.stagger = 0.4;
            d.decor = Some(decor(DecorShape::Bar, Rgba::gray(0.08), 0.35, 0.0, false));
            d
        }
        _ => return None,
    })
}

pub fn out_preset(id: &str) -> Option<AnimDescriptor> {
    match id {
        "dissolve" => {
            let mut d = staggered(
                with(
                    base(id, 0.8, G::Char, Easing::QuadIn),
                    &[T::new(P::Opacity, 1.0, 0.0), T::new(P::Blur, 0.0, 0.3)],
                ),
                0.7,
            );
            d.order = UnitOrder::Random;
            Some(d)
        }
        "fall" => Some(staggered(
            with(
                base(id, 0.9, G::Char, Easing::CubicIn),
                &[
                    T::new(P::PosY, 0.0, -2.5),
                    T::new(P::Rotation, 0.0, 25.0).jitter(20.0),
                    T::new(P::Opacity, 1.0, 0.0),
                ],
            ),
            0.6,
        )),
        _ => {
            let mut d = in_preset(id)?;
            d.mirror = true;
            Some(d)
        }
    }
}

pub fn loop_preset(id: &str) -> Option<AnimDescriptor> {
    Some(match id {
        "pulse" => with(
            base(id, 1.2, G::Block, Easing::Linear),
            &[T::new(P::Scale, 1.0, 1.08)],
        ),
        "float" => staggered(
            with(
                base(id, 2.4, G::Word, Easing::Linear),
                &[T::new(P::PosY, -0.05, 0.05)],
            ),
            0.5,
        ),
        "loop_wave" => staggered(
            with(
                base(id, 1.4, G::Char, Easing::Linear),
                &[T::new(P::PosY, -0.1, 0.1)],
            ),
            1.0,
        ),
        "shake" => with(
            base(id, 0.5, G::Block, Easing::Linear),
            &[
                T::new(P::PosX, 0.0, 0.0).jitter(0.03),
                T::new(P::PosY, 0.0, 0.0).jitter(0.03),
                T::new(P::Rotation, 0.0, 0.0).jitter(1.5),
            ],
        ),
        "glow_pulse" => {
            let mut d = with(
                base(id, 1.6, G::Block, Easing::Linear),
                &[T::new(P::Glow, 0.1, 1.0)],
            );
            d.tint = CYAN;
            d
        }
        "shimmer" => {
            staggered(
                with(
                    base(id, 1.8, G::Char, Easing::Linear),
                    &[T::new(P::Tint, 0.0, 0.9).wave(Wave::Triangle)],
                ),
                0.5,
            )
        }
        "flicker" => {
            let mut d = staggered(
                with(
                    base(id, 0.8, G::Char, Easing::Linear),
                    &[T::new(P::Opacity, 1.0, 0.35).wave(Wave::Flicker)],
                ),
                1.0,
            );
            d.order = UnitOrder::Random;
            d
        }
        "gradient_flow" => with(
            base(id, 3.0, G::Block, Easing::Linear),
            &[T::new(P::GradientOffset, 0.0, 1.0).wave(Wave::Saw)],
        ),
        _ => return None,
    })
}

pub fn karaoke() -> crate::text_anim::Karaoke {
    crate::text_anim::Karaoke::default()
}

#[cfg(test)]
#[path = "tests/text_anim_presets.rs"]
mod tests;
