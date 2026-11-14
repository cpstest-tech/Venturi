use super::*;

#[test]
fn asset_slots_use_their_own_intervals() {
    let a = ClipAnimations {
        in_asset: Some("animations/zoom_2".into()),
        out_asset: Some("animations/fade_out".into()),
        combo_asset: Some("animations/bounce_1".into()),
        in_seconds: 0.5,
        out_seconds: 1.0,
        ..Default::default()
    };
    assert_eq!(
        a.asset_times(0.25, 4.0),
        vec![
            ("animations/bounce_1", 0.0625, 0.25),
            ("animations/zoom_2", 0.5, 0.25)
        ]
    );
    assert_eq!(
        a.asset_times(2.0, 4.0),
        vec![("animations/bounce_1", 0.5, 2.0)]
    );
    assert_eq!(
        a.asset_times(3.5, 4.0),
        vec![
            ("animations/bounce_1", 0.875, 3.5),
            ("animations/fade_out", 0.5, 0.5)
        ]
    );
    assert!(a.asset_times(0.0, 0.0).is_empty());
    let saved = ron::to_string(&a).unwrap();
    assert_eq!(ron::from_str::<ClipAnimations>(&saved).unwrap(), a);
    let old: ClipAnimations = serde_json::from_str("{}").unwrap();
    assert!(old.asset_times(0.0, 2.0).is_empty());
}

#[test]
fn asset_edges_fit_short_clips_and_sanitize_duration() {
    let mut a = ClipAnimations {
        in_asset: Some("in".into()),
        out_asset: Some("out".into()),
        in_seconds: 1.0,
        out_seconds: 1.0,
        ..Default::default()
    };
    let passes = a.asset_times(0.5, 1.0);
    assert_eq!(passes, vec![("in", 1.0, 0.5), ("out", 0.0, 0.0)]);
    a.in_seconds = f64::NAN;
    a.out_seconds = f64::INFINITY;
    assert!(
        a.asset_times(0.1, 1.0)
            .iter()
            .all(|(_, p, t)| p.is_finite() && t.is_finite())
    );
}

#[test]
fn disabled_preserves_transform() {
    let mut t = Transform {
        zoom: [2.0, 3.0],
        position: [20.0, 30.0],
        ..Default::default()
    };
    let before = t;
    ClipAnimations::default().apply(&mut t, 0.2, 2.0, 1920.0);
    assert_eq!(t, before);
}

#[test]
fn entrances_and_exits_follow_clip_time() {
    let mut a = ClipAnimations {
        entrance: Some(Entrance::FromLeft),
        exit: Some(Exit::ToRight),
        ..Default::default()
    };
    for (time, x) in [(0.0, -1920.0), (1.0, 0.0), (2.0, 1920.0)] {
        let mut t = Transform::default();
        a.apply(&mut t, time, 2.0, 1920.0);
        assert_eq!(t.position[0], x);
    }
    a.entrance = Some(Entrance::Zoom);
    a.exit = Some(Exit::Zoom);
    for (time, zoom) in [(0.0, 0.0001), (1.0, 1.0), (2.0, 0.0001)] {
        let mut t = Transform::default();
        a.apply(&mut t, time, 2.0, 1920.0);
        assert!((t.zoom[0] - zoom).abs() < 0.00001);
    }
}

#[test]
fn pulse_repeats_and_composes_with_keyframes() {
    let a = ClipAnimations {
        pulse: true,
        ..Default::default()
    };
    for (time, zoom) in [(0.0, 2.0), (0.5, 2.2), (1.0, 2.0)] {
        let mut t = Transform {
            zoom: [2.0; 2],
            ..Default::default()
        };
        a.apply(&mut t, time, 3.0, 1920.0);
        assert!((t.zoom[0] - zoom).abs() < 0.00001);
    }
}

#[test]
fn short_clips_fit_both_edges_and_roundtrip() {
    let a = ClipAnimations {
        entrance: Some(Entrance::Zoom),
        exit: Some(Exit::Zoom),
        ..Default::default()
    };
    let mut t = Transform::default();
    a.apply(&mut t, 0.05, 0.1, 1920.0);
    assert_eq!(t.zoom, [1.0; 2]);
    let saved = ron::to_string(&a).unwrap();
    assert_eq!(ron::from_str::<ClipAnimations>(&saved).unwrap(), a);
    a.apply(&mut t, 0.0, 0.0, 1920.0);
    assert!(t.zoom[0].is_finite());
}
