use super::*;

#[test]
fn sampled_animation_interpolates_overshoot_and_survives_project_serialization() {
    let mut descriptor = AnimDescriptor::default();
    descriptor.sampled_tracks.push(SampledTrack {
        prop: AnimProp::Scale,
        values: vec![0.0, 1.12, 0.98, 1.0],
    });
    let text = ron::to_string(&descriptor).unwrap();
    let decoded: AnimDescriptor = ron::from_str(&text).unwrap();
    assert_eq!(decoded, descriptor);
    let track = &decoded.sampled_tracks[0];
    assert_eq!(track.value_at(-1.0), Some(0.0));
    assert_eq!(track.value_at(2.0), Some(1.0));
    assert!((track.value_at(0.5).unwrap() - 1.05).abs() < 1e-6);
    let animations = TextAnimations {
        enter: Some(decoded),
        ..Default::default()
    };
    let state = AnimState {
        t_in: 0.2,
        ..Default::default()
    };
    let plan = plan(&animations, "PAUSE", &state);
    assert!((plan.fx[0].scale[0] - 1.12).abs() < 1e-6);
}
use crate::text_anim_presets as presets;

fn anims_in(id: &str) -> TextAnimations {
    TextAnimations {
        enter: presets::in_preset(id),
        ..Default::default()
    }
}

#[test]
fn units_by_granularity() {
    let text = "ab cd\nef";
    assert_eq!(split_units(text, Granularity::Line).len(), 2);
    assert_eq!(split_units(text, Granularity::Word).len(), 3);
    assert_eq!(split_units(text, Granularity::Char).len(), 6);
    assert_eq!(split_units(text, Granularity::Block).len(), 1);
    let words = split_units(text, Granularity::Word);
    assert_eq!((words[1].line, words[1].start, words[1].end), (0, 3, 5));
    assert_eq!(words[2].line, 1);
}

#[test]
fn non_ascii_chars_split_on_boundaries() {
    let units = split_units("àè", Granularity::Char);
    assert_eq!((units[0].start, units[0].end), (0, 2));
    assert_eq!((units[1].start, units[1].end), (2, 4));
}

#[test]
fn orders() {
    assert_eq!(ranks(UnitOrder::Forward, 4), [0, 1, 2, 3]);
    assert_eq!(ranks(UnitOrder::Backward, 4), [3, 2, 1, 0]);
    let center = ranks(UnitOrder::FromCenter, 5);
    assert_eq!(center[2], 0);
    assert!(center[0] > center[1] && center[4] > center[3]);
    let random = ranks(UnitOrder::Random, 8);
    let mut sorted = random.clone();
    sorted.sort();
    assert_eq!(sorted, (0..8).collect::<Vec<_>>());
    assert_eq!(random, ranks(UnitOrder::Random, 8));
}

#[test]
fn in_starts_hidden_and_ends_at_rest() {
    let anims = anims_in("fade");
    let start = plan(&anims, "Hello", &AnimState::default());
    assert_eq!(start.fx[0].opacity, 0.0);
    let dur = anims.enter.as_ref().unwrap().duration;
    let end = plan(
        &anims,
        "Hello",
        &AnimState {
            t_in: dur,
            ..Default::default()
        },
    );
    assert!(end.fx[0].is_rest());
}

#[test]
fn stagger_delays_later_units() {
    let anims = anims_in("typewriter");
    let dur = anims.enter.as_ref().unwrap().duration;
    let mid = plan(
        &anims,
        "abcdef",
        &AnimState {
            t_in: dur * 0.5,
            ..Default::default()
        },
    );
    assert_eq!(mid.fx[0].opacity, 1.0);
    assert_eq!(mid.fx[5].opacity, 0.0);
}

#[test]
fn out_mirrors_the_in() {
    let out = TextAnimations {
        exit: presets::out_preset("pop"),
        ..Default::default()
    };
    let dur = out.exit.as_ref().unwrap().duration;
    assert!(plan(&out, "Hi", &AnimState::default()).fx[0].is_rest());
    let gone = plan(
        &out,
        "Hi",
        &AnimState {
            t_out: dur,
            ..Default::default()
        },
    );
    assert_eq!(gone.fx[0].opacity, 0.0);
}

#[test]
fn loop_is_periodic() {
    let anims = TextAnimations {
        looped: presets::loop_preset("pulse"),
        ..Default::default()
    };
    let period = anims.looped.as_ref().unwrap().duration;
    let at = |t: f32| {
        plan(
            &anims,
            "Hi",
            &AnimState {
                t_loop: t,
                ..Default::default()
            },
        )
    };
    let (a, b) = (at(0.3), at(0.3 + period));
    assert!((a.fx[0].scale[0] - b.fx[0].scale[0]).abs() < 1e-4);
    assert!(at(period / 2.0).fx[0].scale[0] > 1.07);
}

#[test]
fn same_state_same_plan_for_every_preset() {
    let text = "Some title\nwith two lines";
    let state = AnimState {
        t_in: 0.21,
        t_out: 0.13,
        t_loop: 1.37,
        kara_word: 2,
        clip_len: 4.0,
    };
    for id in presets::IN_IDS {
        let a = TextAnimations {
            enter: presets::in_preset(id),
            exit: presets::out_preset(id),
            looped: presets::loop_preset("shake"),
            karaoke: Some(Karaoke::default()),
        };
        assert_eq!(
            plan(&a, text, &state),
            plan(&a.clone(), text, &state),
            "{id}"
        );
    }
}

#[test]
fn every_preset_exists() {
    for id in presets::IN_IDS {
        assert!(presets::in_preset(id).is_some(), "{id}");
    }
    for id in presets::out_ids() {
        assert!(presets::out_preset(id).is_some(), "{id}");
    }
    for id in presets::LOOP_IDS {
        assert!(presets::loop_preset(id).is_some(), "{id}");
    }
}

#[test]
fn jitter_is_deterministic_and_settles() {
    let anims = anims_in("glitch");
    let dur = anims.enter.as_ref().unwrap().duration;
    let at = |t: f32| {
        plan(
            &anims,
            "glitch",
            &AnimState {
                t_in: t,
                ..Default::default()
            },
        )
    };
    assert_eq!(at(0.2), at(0.2));
    assert!(at(dur).fx.iter().all(|f| f.offset[0].abs() < 1e-6));
}

#[test]
fn karaoke_uses_word_times_or_spreads_evenly() {
    let mut anims = TextAnimations {
        karaoke: Some(Karaoke {
            word_ms: vec![[0, 400], [500, 900]],
            ..Karaoke::default()
        }),
        ..Default::default()
    };
    assert_eq!(anims.state_at("one two", 0.2, 2.0).unwrap().kara_word, 0);
    assert_eq!(anims.state_at("one two", 0.7, 2.0).unwrap().kara_word, 1);
    anims.karaoke.as_mut().unwrap().word_ms.clear();
    assert_eq!(anims.state_at("one two", 1.5, 2.0).unwrap().kara_word, 1);
    let p = plan(
        &anims,
        "one two",
        &AnimState {
            kara_word: 1,
            clip_len: 2.0,
            ..Default::default()
        },
    );
    assert_eq!(p.fx[0].tint.1, 0.0);
    assert_eq!(p.fx[1].tint.1, 1.0);
}

#[test]
fn state_is_none_when_nothing_moves() {
    let anims = anims_in("fade");
    assert!(anims.state_at("x", 0.1, 5.0).is_some());
    assert!(anims.state_at("x", 2.0, 5.0).is_none());
    assert!(TextAnimations::default().state_at("x", 0.0, 1.0).is_none());
}

#[test]
fn descriptors_round_trip_through_ron() {
    let anims = TextAnimations {
        enter: presets::in_preset("bar_expand"),
        exit: presets::out_preset("fall"),
        looped: presets::loop_preset("shimmer"),
        karaoke: Some(Karaoke::default()),
    };
    let text = ron::to_string(&anims).unwrap();
    let back: TextAnimations = ron::from_str(&text).unwrap();
    assert_eq!(anims, back);
}

#[test]
fn decor_progress_follows_the_in() {
    let anims = anims_in("underline");
    let dur = anims.enter.as_ref().unwrap().duration;
    let start = plan(&anims, "x", &AnimState::default());
    let end = plan(
        &anims,
        "x",
        &AnimState {
            t_in: dur,
            ..Default::default()
        },
    );
    assert_eq!(start.decor[0].progress, 0.0);
    assert_eq!(end.decor[0].progress, 1.0);
}
