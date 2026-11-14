use super::*;

#[test]
fn flame_atlas_has_distinct_visible_frames() {
    let frames = fire_frames();
    assert_eq!(frames.len(), 38);
    assert!(frames.iter().all(|frame| frame.iter().any(|(mask, _)| {
        mask.width == 280 && mask.height == 117 && mask.data.iter().any(|&v| v > 0)
    })));
    assert_ne!(frames[0][0].0.data, frames[10][0].0.data);
}

#[test]
fn sticker_waits_for_its_start_and_tracks_the_title() {
    let mut params = TitleParams::default();
    params.style.sticker = Some(vv_core::TextSticker {
        asset: "capcut_onfire".into(),
        size: [4.0, 1.0],
        offset: [0.0, 1.0],
        rotation: 0.0,
        start: 0.38,
    });
    params.anim_state = Some(Default::default());
    let mut render = TitleRender {
        size: (640, 360),
        layers: Vec::new(),
    };
    append(&mut render, &params, (640, 360), (640, 360));
    assert!(render.layers.is_empty());
    params.anim_state.as_mut().unwrap().t_loop = 0.8;
    append(&mut render, &params, (640, 360), (640, 360));
    assert!(!render.layers.is_empty());
    let TitleLayer::Parts(parts, _) = &render.layers[0] else {
        panic!("sprite parts")
    };
    let before = parts[0].xf.shift;
    params.position = [100.0, 40.0];
    let mut moved = TitleRender {
        size: (640, 360),
        layers: Vec::new(),
    };
    append(&mut moved, &params, (640, 360), (640, 360));
    let TitleLayer::Parts(parts, _) = &moved.layers[0] else {
        panic!("sprite parts")
    };
    assert_eq!(parts[0].xf.shift, (before.0 + 100.0, before.1 - 40.0));
}
