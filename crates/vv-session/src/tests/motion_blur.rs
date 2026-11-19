use super::*;

fn assert_weights(got: &[f32], expected: &[f64]) {
    assert_eq!(got.len(), expected.len(), "{got:?}");
    let total: f64 = expected.iter().sum();
    for (g, e) in got.iter().zip(expected) {
        assert!(
            (f64::from(*g) - e / total).abs() < 1e-5,
            "{got:?} vs {expected:?}"
        );
    }
}

fn w(frames: usize, weighting: Weighting) -> Vec<f32> {
    weights(frames, &weighting, &GaussianParams::default())
}

#[test]
fn the_weightings_match_blur() {
    assert_weights(&w(3, Weighting::Equal), &[1.0, 1.0, 1.0]);
    assert_weights(&w(3, Weighting::Ascending), &[1.0, 2.0, 3.0]);
    assert_weights(&w(3, Weighting::Descending), &[3.0, 2.0, 1.0]);
    assert_weights(&w(5, Weighting::Pyramid), &[1.0, 2.0, 3.0, 2.0, 1.0]);
    assert_weights(&w(4, Weighting::Vegas), &[1.0, 2.0, 2.0, 1.0]);
    assert_weights(&w(5, Weighting::Vegas), &[1.0; 5]);
    let e2 = (-2.0f64).exp();
    // Bound (0, 2) becomes (-2, 2) around 0.
    assert_weights(&w(3, Weighting::GaussianSym), &[e2, 1.0, e2]);
    // Mean 2 over x = 0, 1, 2.
    let e05 = (-0.5f64).exp();
    assert_weights(&w(3, Weighting::Gaussian), &[e2, e05, 1.0]);
    assert_weights(&w(3, Weighting::GaussianReverse), &[1.0, e05, e2]);
}

#[test]
fn custom_weights_are_stretched_over_the_frames() {
    let custom = Weighting::parse_custom("1, 2").unwrap();
    assert_weights(
        &w(10, custom),
        &[1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0],
    );
}

#[test]
fn negative_custom_weights_are_shifted_like_blur() {
    let custom = Weighting::parse_custom("-1, 1").unwrap();
    // -1 - (-1) + 1 = 1, 1 - (-1) + 1 = 3.
    assert_weights(&w(2, custom), &[1.0, 3.0]);
}

#[test]
fn invalid_custom_weights_are_rejected() {
    assert_eq!(Weighting::parse_custom(""), None);
    assert_eq!(Weighting::parse_custom("1, a"), None);
    assert_eq!(Weighting::parse_custom("0, 0"), None);
}

#[test]
fn fps_targets_parse_like_blur() {
    assert_eq!(FpsTarget::parse("600"), Some(FpsTarget::Fixed(600)));
    assert_eq!(FpsTarget::parse(" 5x "), Some(FpsTarget::Multiplier(5.0)));
    assert_eq!(FpsTarget::parse("2.5X"), Some(FpsTarget::Multiplier(2.5)));
    assert_eq!(FpsTarget::parse("0"), None);
    assert_eq!(FpsTarget::parse("x"), None);
    assert_eq!(FpsTarget::parse("fast"), None);
    assert_eq!(FpsTarget::Multiplier(5.0).to_string(), "5x");
}

fn fps(n: i32) -> Rational {
    Rational::new(n, 1)
}

#[test]
fn sixty_fps_interpolated_to_six_hundred_blends_eleven_frames() {
    let plan = Plan::new(&MotionBlurSettings::default(), fps(60), fps(60), 100);
    assert_eq!(plan.multiplier, 10);
    assert_eq!(plan.weights.len(), 11);
    assert_eq!(plan.center(3), 30);
}

#[test]
fn an_even_frame_count_becomes_odd() {
    let settings = MotionBlurSettings {
        amount: 0.4,
        ..Default::default()
    };
    // Gap 10 × 0.4 = 4, made odd.
    let plan = Plan::new(&settings, fps(60), fps(60), 100);
    assert_eq!(plan.weights.len(), 5);
}

#[test]
fn without_interpolation_a_high_frame_rate_timeline_is_blended_down() {
    let settings = MotionBlurSettings {
        interpolation: None,
        ..Default::default()
    };
    let plan = Plan::new(&settings, fps(240), fps(60), 100);
    assert_eq!(plan.multiplier, 1);
    assert_eq!(plan.weights.len(), 5);
    assert_eq!(plan.center(2), 8);
}

#[test]
fn no_blur_without_frames_to_spare() {
    let settings = MotionBlurSettings {
        interpolation: None,
        ..Default::default()
    };
    assert_eq!(
        Plan::new(&settings, fps(60), fps(60), 10).weights,
        vec![1.0]
    );
    let none = MotionBlurSettings {
        amount: 0.0,
        ..Default::default()
    };
    assert_eq!(Plan::new(&none, fps(60), fps(60), 10).weights, vec![1.0]);
}

#[test]
fn the_taps_repeat_the_edge_frames() {
    let settings = MotionBlurSettings {
        interpolation: Some(FpsTarget::Multiplier(2.0)),
        ..Default::default()
    };
    // Gap 2 → 3 taps around the centre.
    let plan = Plan::new(&settings, fps(30), fps(30), 3);
    let taps = |n| plan.taps(n).map(|(h, _)| h).collect::<Vec<_>>();
    assert_eq!(taps(0), [0, 0, 1]);
    assert_eq!(taps(1), [1, 2, 3]);
    assert_eq!(taps(2), [3, 4, 4]);
}

fn anchors(changed: &[bool], range: usize) -> Anchors {
    let mut anchors = Anchors::new(range);
    for c in changed {
        anchors.push(*c);
    }
    anchors
}

#[test]
fn a_dropped_frame_is_interpolated_between_its_neighbours() {
    let a = anchors(&[true, false, true, true], 2);
    assert_eq!(a.sample(0, 1, true), Sample::Frame(0));
    assert_eq!(a.sample(1, 1, true), Sample::Between { a: 0, b: 2, t: 0.5 });
    assert_eq!(a.sample(2, 1, true), Sample::Frame(2));
}

#[test]
fn a_long_still_only_moves_in_its_last_range_frames() {
    let a = anchors(&[true, false, false, false, false, true], 2);
    assert_eq!(a.sample(1, 1, true), Sample::Frame(1));
    assert_eq!(a.sample(2, 1, true), Sample::Frame(2));
    assert_eq!(
        a.sample(3, 1, true),
        Sample::Between {
            a: 2,
            b: 5,
            t: 1.0 / 3.0
        }
    );
}

#[test]
fn interpolated_positions_fall_between_anchors() {
    let a = anchors(&[true, true, true], 0);
    assert_eq!(
        a.sample(5, 4, true),
        Sample::Between {
            a: 1,
            b: 2,
            t: 0.25
        }
    );
}

#[test]
fn a_duplicate_waits_until_its_range_is_known() {
    let a = anchors(&[true, false], 2);
    assert_eq!(a.is_anchor(1, false), None);
    assert_eq!(a.is_anchor(1, true), Some(true));
}
