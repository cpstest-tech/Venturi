use crate::{Compositor, Layer, LayerContent, OutputFrame};
use vv_core::{AnimState, TextAnimations, TitleParams, text_anim_presets as presets};

fn styled(size: f32) -> TitleParams {
    let mut params = TitleParams {
        content: vv_core::Keyframed::constant("Hello brave\nworld".into()),
        size,
        color: vv_core::Rgba::WHITE,
        ..Default::default()
    };
    params.style.strokes = vec![vv_core::TextStroke {
        width: size / 16.0,
        color: vv_core::Rgba {
            r: 0.9,
            g: 0.1,
            b: 0.5,
            a: 1.0,
        },
        ..Default::default()
    }];
    params.style.shadows = vec![vv_core::TextShadow {
        blur: size / 4.0,
        ..Default::default()
    }];
    params.style.glows = vec![vv_core::TextGlow {
        radius: size / 2.0,
        ..Default::default()
    }];
    params.style.fill = vv_core::TextFill::Gradient(vv_core::Gradient::two_stops(
        vv_core::GradientKind::Linear,
        vv_core::Rgba::WHITE,
        vv_core::Rgba {
            r: 0.1,
            g: 0.4,
            b: 1.0,
            a: 1.0,
        },
    ));
    params
}

/// Mean and largest difference, in 8-bit levels, between the compositor
/// drawing the sprites of `title` and it colouring the masks the CPU drew,
/// and whether the title shows at all.
fn difference(compositor: &Compositor, title: &TitleParams, size: (u32, u32)) -> (f64, u8, bool) {
    let render = |cpu: bool| {
        crate::text::CPU_PARTS.set(cpu);
        let out = compositor.render_layers(
            &[Layer::new(LayerContent::Text(title), Default::default())],
            OutputFrame::exact(size.0, size.1),
        );
        crate::text::CPU_PARTS.set(false);
        out
    };
    let (gpu, cpu) = (render(false), render(true));
    let shows = cpu.chunks(4).any(|px| px[0] > 100);
    let diffs = gpu.iter().zip(&cpu).map(|(a, b)| a.abs_diff(*b));
    let total: u64 = diffs.clone().map(u64::from).sum();
    (
        total as f64 / gpu.len() as f64,
        diffs.max().unwrap_or(0),
        shows,
    )
}

fn states(anim: &TextAnimations) -> Vec<AnimState> {
    let duration = |d: &Option<vv_core::AnimDescriptor>| d.as_ref().map_or(1.0, |d| d.duration);
    [0.15, 0.4, 0.7, 0.95]
        .into_iter()
        .map(|share| AnimState {
            t_in: if anim.enter.is_some() {
                duration(&anim.enter) * share
            } else {
                10.0
            },
            t_out: if anim.exit.is_some() {
                duration(&anim.exit) * share
            } else {
                10.0
            },
            t_loop: duration(&anim.looped) * share,
            clip_len: 20.0,
            ..Default::default()
        })
        .collect()
}

#[test]
fn sprites_drawn_on_the_gpu_match_the_masks_drawn_on_the_cpu() {
    let compositor = Compositor::new_headless();
    let size = (640, 360);
    let anims = (presets::IN_IDS.iter().map(|id| TextAnimations {
        enter: presets::in_preset(id),
        ..Default::default()
    }))
    .chain(presets::out_ids().into_iter().map(|id| TextAnimations {
        exit: presets::out_preset(id),
        ..Default::default()
    }))
    .chain(presets::LOOP_IDS.iter().map(|id| TextAnimations {
        looped: presets::loop_preset(id),
        ..Default::default()
    }));
    for anim in anims {
        let name = [&anim.enter, &anim.exit, &anim.looped]
            .into_iter()
            .flatten()
            .map(|d| d.id.clone())
            .collect::<String>();
        let blurs = [&anim.enter, &anim.exit, &anim.looped]
            .into_iter()
            .flatten()
            .any(|d| d.tracks.iter().any(|t| t.prop == vv_core::AnimProp::Blur));
        let mut shown = false;
        for state in states(&anim) {
            let title = TitleParams {
                anim: anim.clone(),
                anim_state: Some(state),
                ..styled(44.0)
            };
            let (mean, max, shows) = difference(&compositor, &title, size);
            shown |= shows;
            // The GPU blurs with a true Gaussian and the layer as a whole,
            // the CPU with three box passes sprite by sprite.
            let (mean_cap, max_cap) = if blurs { (1.0, 60) } else { (0.1, 12) };
            assert!(
                mean < mean_cap && max <= max_cap,
                "{name} at {state:?}: mean {mean:.3}, max {max}"
            );
        }
        assert!(shown, "{name} never shows");
    }
}

#[test]
fn layers_kept_smaller_land_where_the_full_size_ones_would() {
    let compositor = Compositor::new_headless();
    // Large enough for the shadow and the glow to be kept at a lower resolution.
    let title = TitleParams {
        anim: TextAnimations {
            looped: presets::loop_preset("pulse"),
            ..Default::default()
        },
        anim_state: Some(AnimState {
            t_loop: 0.3,
            clip_len: 20.0,
            ..Default::default()
        }),
        ..styled(200.0)
    };
    let (mean, max, shows) = difference(&compositor, &title, (1920, 1080));
    assert!(shows && mean < 0.1 && max <= 12, "mean {mean:.3}, max {max}");
}

#[test]
fn a_title_with_nothing_showing_still_clears_the_frame() {
    let compositor = Compositor::new_headless();
    let title = TitleParams {
        anim: TextAnimations {
            enter: presets::in_preset("fade"),
            ..Default::default()
        },
        anim_state: Some(AnimState::default()),
        ..styled(44.0)
    };
    let out = compositor.render_layers(
        &[Layer::new(LayerContent::Text(&title), Default::default())],
        OutputFrame::exact(64, 36),
    );
    assert!(out.chunks(4).all(|px| px == [0, 0, 0, 255]));
}
