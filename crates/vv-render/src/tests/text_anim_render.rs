use crate::text::{TextMask, render_title};
use vv_core::{AnimState, TextAnimations, TitleParams, text_anim_presets as presets};

const SIZE: (u32, u32) = (640, 360);

fn title(id: &str) -> TitleParams {
    TitleParams {
        content: vv_core::Keyframed::constant("Hello brave world".into()),
        size: 48.0,
        anim: TextAnimations {
            enter: presets::in_preset(id),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn at(params: &TitleParams, state: AnimState) -> TitleParams {
    TitleParams {
        anim_state: Some(state),
        ..params.clone()
    }
}

fn ink(mask: &TextMask) -> u64 {
    mask.data.iter().map(|&v| u64::from(v)).sum()
}

fn flat(params: &TitleParams) -> Vec<Vec<u8>> {
    render_title(params, SIZE, SIZE)
        .layers
        .iter()
        .map(|(m, _)| m.data.clone())
        .collect()
}

#[test]
fn same_state_renders_the_same_pixels() {
    for id in presets::IN_IDS {
        let params = title(id);
        let state = AnimState {
            t_in: 0.23,
            ..Default::default()
        };
        let a = flat(&at(&params, state));
        // A different cache history must not matter.
        let _ = flat(&at(&params, AnimState { t_in: 0.4, ..state }));
        crate::text_anim_render::cache().lock().unwrap().clear();
        let b = flat(&at(&params, state));
        assert_eq!(a, b, "{id}");
    }
}

#[test]
fn finished_in_matches_the_static_title() {
    for id in ["fade", "pop", "typewriter", "flip_3d", "slide"] {
        let params = title(id);
        let done = AnimState {
            t_in: params.anim.enter.as_ref().unwrap().duration,
            ..Default::default()
        };
        let animated = render_title(&at(&params, done), SIZE, SIZE);
        let plain = render_title(&TitleParams { anim_state: None, ..params }, SIZE, SIZE);
        let diff: i64 = animated
            .text()
            .data
            .iter()
            .zip(&plain.text().data)
            .map(|(a, b)| (i64::from(*a) - i64::from(*b)).abs())
            .sum();
        assert!(
            diff * 100 < ink(plain.text()) as i64,
            "{id}: diff {diff} of {}",
            ink(plain.text())
        );
    }
}

#[test]
fn start_of_a_fade_in_is_invisible() {
    let params = title("fade");
    let render = render_title(&at(&params, AnimState::default()), SIZE, SIZE);
    assert_eq!(ink(render.text()), 0);
}

#[test]
fn typewriter_reveals_characters_progressively() {
    let params = title("typewriter");
    let dur = params.anim.enter.as_ref().unwrap().duration;
    let ink_at = |t: f32| {
        let state = AnimState {
            t_in: t,
            ..Default::default()
        };
        ink(render_title(&at(&params, state), SIZE, SIZE).text())
    };
    let (a, b, c) = (ink_at(dur * 0.3), ink_at(dur * 0.6), ink_at(dur));
    assert!(0 < a && a < b && b < c, "{a} {b} {c}");
}

#[test]
fn mask_reveal_shows_the_left_part_first() {
    let params = title("mask_reveal");
    let state = AnimState {
        t_in: params.anim.enter.as_ref().unwrap().duration * 0.5,
        ..Default::default()
    };
    let render = render_title(&at(&params, state), SIZE, SIZE);
    let mask = render.text();
    let (mut left, mut right) = (0u64, 0u64);
    for y in 0..mask.height {
        for x in 0..mask.width {
            let v = u64::from(mask.data[(y * mask.width + x) as usize]);
            if x < mask.width / 2 {
                left += v;
            } else {
                right += v;
            }
        }
    }
    assert!(left > right, "{left} vs {right}");
}

#[test]
fn decor_grows_with_progress() {
    let params = title("underline");
    let dur = params.anim.enter.as_ref().unwrap().duration;
    let decor_ink = |t: f32| {
        let state = AnimState {
            t_in: t,
            ..Default::default()
        };
        let render = render_title(&at(&params, state), SIZE, SIZE);
        ink(&render.layers[0].0)
    };
    let (a, b) = (decor_ink(dur * 0.4), decor_ink(dur));
    assert!(0 < a && a < b, "{a} {b}");
}

#[test]
fn karaoke_tints_only_the_active_word() {
    let params = TitleParams {
        anim: TextAnimations {
            karaoke: Some(vv_core::Karaoke::default()),
            ..Default::default()
        },
        ..title("fade")
    };
    let state = AnimState {
        kara_word: 1,
        clip_len: 3.0,
        ..Default::default()
    };
    let render = render_title(&at(&params, state), SIZE, SIZE);
    let (tint, color) = render.layers.last().unwrap();
    assert_eq!(*color, vv_core::Karaoke::default().color);
    assert!(ink(tint) > 0);
    assert!(ink(tint) < ink(&render.layers[render.layers.len() - 2].0));
}

#[test]
fn every_loop_and_out_preset_renders() {
    for id in presets::LOOP_IDS {
        let params = TitleParams {
            anim: TextAnimations {
                looped: presets::loop_preset(id),
                ..Default::default()
            },
            ..title("fade")
        };
        let state = AnimState {
            t_loop: 0.7,
            ..Default::default()
        };
        assert!(!render_title(&at(&params, state), SIZE, SIZE).layers.is_empty(), "{id}");
    }
    for id in presets::out_ids() {
        let params = TitleParams {
            anim: TextAnimations {
                exit: presets::out_preset(id),
                ..Default::default()
            },
            ..title("fade")
        };
        let state = AnimState {
            t_out: 0.3,
            ..Default::default()
        };
        assert!(!render_title(&at(&params, state), SIZE, SIZE).layers.is_empty(), "{id}");
    }
}
