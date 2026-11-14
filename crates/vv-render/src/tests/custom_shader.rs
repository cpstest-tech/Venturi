use crate::assets::{self, AssetKind};
use crate::{Compositor, CustomPass, Layer, LayerContent, OutputFrame};
use vv_core::{Rgba, Transform};

const RED: Rgba = Rgba {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const BLUE: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

fn solid(color: Rgba) -> Layer<'static> {
    Layer::new(LayerContent::Solid(color), Transform::default())
}

fn pass<'a>(id: &'a str, source: &'a str, params: [f32; 8], progress: f32) -> CustomPass<'a> {
    CustomPass {
        key: id,
        source,
        params,
        curves: [0.0; crate::assets::MAX_CURVES],
        progress,
        time: 0.4,
        amount: 1.0,
    }
}

fn pixel(compositor: &Compositor, texture: &wgpu::Texture, at: (u32, u32)) -> [u8; 4] {
    let layers = [Layer::new(
        LayerContent::Texture {
            texture,
            source_size: (16, 16),
        },
        Transform::default(),
    )];
    let out = compositor.render_layers(&layers, OutputFrame::exact(16, 16));
    let i = ((at.1 * 16 + at.0) * 4) as usize;
    [out[i], out[i + 1], out[i + 2], out[i + 3]]
}

#[test]
fn every_builtin_shader_compiles_and_runs() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(16, 16);
    let a = compositor.render_layers_to_owned_texture_transparent(&[solid(RED)], output);
    let b = compositor.render_layers_to_owned_texture_transparent(&[solid(BLUE)], output);
    let lib = assets::library();
    for kind in AssetKind::ALL {
        for asset in lib.of_kind(kind) {
            let params = asset.shader_params(&asset.default_params());
            let custom = CustomPass {
                curves: asset.curve_values(0.5),
                ..pass(&asset.id, &asset.shader, params, 0.5)
            };
            let result = compositor.render_custom_pass(&custom, &a, &b);
            assert!(result.is_some(), "{} does not compile", asset.id);
        }
    }
}

#[test]
fn animation_rest_endpoints_preserve_the_input() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(16, 16);
    let a = compositor.render_layers_to_owned_texture_transparent(&[solid(RED)], output);
    let lib = assets::library();
    for asset in lib
        .of_kind(AssetKind::Animation)
        .filter(|a| a.category != "combo")
    {
        let progress = if asset.category == "in" { 1.0 } else { 0.0 };
        let custom = CustomPass {
            curves: asset.curve_values(progress),
            ..pass(&asset.id, &asset.shader, asset.shader_params(&[]), progress)
        };
        let texture = compositor.render_custom_pass(&custom, &a, &a).unwrap();
        let [r, g, b, _] = pixel(&compositor, &texture, (8, 8));
        assert!(r >= 250 && g <= 1 && b <= 1, "{}: {r}, {g}, {b}", asset.id);
    }
}

#[test]
fn a_dissolve_at_half_mixes_both_sides() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(16, 16);
    let a = compositor.render_layers_to_owned_texture_transparent(&[solid(RED)], output);
    let b = compositor.render_layers_to_owned_texture_transparent(&[solid(BLUE)], output);
    let lib = assets::library();
    let asset = lib.get("transitions/dissolve").unwrap();
    let mixed = compositor
        .render_custom_pass(&pass(&asset.id, &asset.shader, [0.0; 8], 0.5), &a, &b)
        .unwrap();
    let [r, _, bl, alpha] = pixel(&compositor, &mixed, (8, 8));
    assert!(
        (120..=135).contains(&r) && (120..=135).contains(&bl),
        "{r} {bl}"
    );
    assert_eq!(alpha, 255);
}

#[test]
fn a_transition_is_the_first_side_at_zero_and_the_second_at_one() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(16, 16);
    let a = compositor.render_layers_to_owned_texture_transparent(&[solid(RED)], output);
    let b = compositor.render_layers_to_owned_texture_transparent(&[solid(BLUE)], output);
    let lib = assets::library();
    for asset in lib.of_kind(AssetKind::Transition) {
        let params = asset.shader_params(&asset.default_params());
        for (progress, expect_red) in [(0.0, true), (1.0, false)] {
            let custom = CustomPass {
                curves: asset.curve_values(progress),
                ..pass(&asset.id, &asset.shader, params, progress)
            };
            let out = compositor.render_custom_pass(&custom, &a, &b).unwrap();
            let [r, _, bl, _] = pixel(&compositor, &out, (4, 8));
            let ok = if expect_red {
                r > 240 && bl < 15
            } else {
                bl > 240 && r < 15
            };
            assert!(ok, "{} at {progress}: r={r} b={bl}", asset.id);
        }
    }
}

#[test]
fn a_broken_shader_is_skipped_not_fatal() {
    let compositor = Compositor::new_headless();
    let a = compositor
        .render_layers_to_owned_texture_transparent(&[solid(RED)], OutputFrame::exact(16, 16));
    let bad = pass(
        "broken",
        "fn effect(uv: vec2<f32>) -> vec4<f32> { return nope; }",
        [0.0; 8],
        0.0,
    );
    assert!(compositor.render_custom_pass(&bad, &a, &a).is_none());
}
