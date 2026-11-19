use crate::{Compositor, Layer, LayerContent, OutputFrame};

fn solid(r: f32, g: f32, b: f32) -> vv_core::Rgba {
    vv_core::Rgba { r, g, b, a: 1.0 }
}

fn render_solid(
    compositor: &Compositor,
    color: vv_core::Rgba,
    output: OutputFrame,
) -> crate::PooledTexture {
    compositor.render_layers_to_owned_texture_transparent(
        &[Layer::new(
            LayerContent::Solid(color),
            vv_core::Transform::default(),
        )],
        output,
    )
}

fn read(compositor: &Compositor, texture: &wgpu::Texture, output: OutputFrame) -> Vec<u8> {
    compositor.render_layers(
        &[Layer::new(
            LayerContent::Texture {
                texture,
                source_size: (output.width, output.height),
            },
            vv_core::Transform::default(),
        )],
        output,
    )
}

#[test]
fn weighted_frames_add_up_to_their_weighted_average() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(8, 8);
    let black = render_solid(&compositor, solid(0.0, 0.0, 0.0), output);
    let white = render_solid(&compositor, solid(1.0, 1.0, 1.0), output);
    let mut sum = compositor.blend_sum((8, 8), 1.0);
    compositor.blend_add(&mut sum, &black, 0.75);
    compositor.blend_add(&mut sum, &white, 0.25);
    let blended = compositor.blend_resolve(&sum);
    let rgba = read(&compositor, &blended, output);
    for px in rgba.chunks(4) {
        assert!((px[0] as i32 - 64).abs() <= 2, "{px:?}");
        assert_eq!(px[3], 255);
    }
}

#[test]
fn a_reset_sum_forgets_the_previous_frames() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(8, 8);
    let white = render_solid(&compositor, solid(1.0, 1.0, 1.0), output);
    let red = render_solid(&compositor, solid(1.0, 0.0, 0.0), output);
    let mut sum = compositor.blend_sum((8, 8), 1.0);
    compositor.blend_add(&mut sum, &white, 1.0);
    sum.reset(1.0);
    compositor.blend_add(&mut sum, &red, 1.0);
    let rgba = read(&compositor, &compositor.blend_resolve(&sum), output);
    assert!(rgba.chunks(4).all(|px| px[0] > 250 && px[1] < 5));
}

/// Gamma above 1 blends in a brighter space: half white over black comes
/// out brighter than the plain average.
#[test]
fn a_higher_gamma_blends_brighter() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(8, 8);
    let black = render_solid(&compositor, solid(0.0, 0.0, 0.0), output);
    let white = render_solid(&compositor, solid(1.0, 1.0, 1.0), output);
    let blend = |gamma: f32| {
        let mut sum = compositor.blend_sum((8, 8), gamma);
        compositor.blend_add(&mut sum, &black, 0.5);
        compositor.blend_add(&mut sum, &white, 0.5);
        read(&compositor, &compositor.blend_resolve(&sum), output)[0]
    };
    let plain = blend(1.0);
    let bright = blend(2.2);
    assert!((plain as i32 - 128).abs() <= 2, "{plain}");
    assert!(bright > plain + 40, "{bright} vs {plain}");
}

#[test]
fn the_difference_is_zero_for_equal_frames_and_one_for_black_and_white() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(64, 36);
    let black = render_solid(&compositor, solid(0.0, 0.0, 0.0), output);
    let black_again = render_solid(&compositor, solid(0.0, 0.0, 0.0), output);
    let white = render_solid(&compositor, solid(1.0, 1.0, 1.0), output);
    assert!(compositor.frame_difference(&black, &black_again) < 1e-3);
    let full = compositor.frame_difference(&black, &white);
    assert!((full - 1.0).abs() < 0.01, "{full}");
}

#[test]
fn an_interpolated_frame_between_two_equal_frames_is_the_same_frame() {
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(32, 32);
    let grey = render_solid(&compositor, solid(0.5, 0.5, 0.5), output);
    let flow = compositor.frame_flow(&grey, &grey);
    let middle = compositor.interpolate_frame(&grey, &grey, &flow, 0.5);
    let a = read(&compositor, &grey, output);
    let b = read(&compositor, &middle, output);
    for (x, y) in a.iter().zip(&b) {
        assert!((*x as i32 - *y as i32).abs() <= 2);
    }
}
