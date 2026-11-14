use super::*;

#[test]
fn every_curve_starts_at_0_and_ends_at_1() {
    for easing in Easing::ALL {
        assert!(easing.ease(0.0).abs() < 1e-4, "{easing:?} start");
        assert!((easing.ease(1.0) - 1.0).abs() < 1e-4, "{easing:?} end");
    }
}

#[test]
fn input_is_clamped() {
    for easing in Easing::ALL {
        assert_eq!(easing.ease(-3.0), easing.ease(0.0));
        assert_eq!(easing.ease(7.0), easing.ease(1.0));
    }
}

#[test]
fn overshooting_curves_leave_the_unit_range() {
    for easing in [Easing::BackOut, Easing::ElasticOut, Easing::Spring] {
        let peak = (0..=100)
            .map(|i| easing.ease(i as f32 / 100.0))
            .fold(0.0, f32::max);
        assert!(peak > 1.02, "{easing:?} peak {peak}");
    }
}

#[test]
fn bounce_and_quad_stay_in_range() {
    for easing in [Easing::BounceOut, Easing::QuadIn, Easing::CubicInOut] {
        for i in 0..=100 {
            let v = easing.ease(i as f32 / 100.0);
            assert!((-1e-4..=1.0001).contains(&v), "{easing:?} {v}");
        }
    }
}

#[test]
fn keyframes_use_the_library() {
    let mut k = crate::Keyframed::constant(0.0f32);
    k.upsert(0, 0.0, crate::Interpolation::Curve(Easing::BackOut));
    k.upsert(10, 10.0, crate::Interpolation::Linear);
    let mid = k.value_at(5);
    assert!((mid - 10.0 * Easing::BackOut.ease(0.5)).abs() < 1e-4);
    assert!(k.value_at(8) > 10.0);
}
