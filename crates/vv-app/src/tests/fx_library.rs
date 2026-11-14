use super::*;

#[test]
fn a_transition_loop_goes_there_holds_and_comes_back() {
    let (swapped, progress, _) = loop_frame(AssetKind::Transition, 0);
    assert!(!swapped && progress == 0.0);
    let (_, progress, _) = loop_frame(AssetKind::Transition, TRANSITION_FRAMES - 1);
    assert_eq!(progress, 1.0);
    let (swapped, progress, _) = loop_frame(AssetKind::Transition, TRANSITION_FRAMES + HOLD_FRAMES);
    assert!(swapped && progress == 0.0);
    let (swapped, progress, _) = loop_frame(AssetKind::Transition, LOOP_FRAMES - 1);
    assert!(swapped && progress == 1.0);
}

#[test]
fn an_effect_loop_runs_forward_in_time() {
    let (_, _, first) = loop_frame(AssetKind::Effect, 0);
    let (_, _, later) = loop_frame(AssetKind::Effect, 10);
    assert!(later > first);
}

#[test]
fn sample_pictures_are_opaque_and_different() {
    let (a, b) = (sample_pixels(0), sample_pixels(1));
    assert_eq!(a.len(), (PREVIEW.0 * PREVIEW.1 * 4) as usize);
    assert!(a.chunks(4).all(|p| p[3] == 255));
    assert_ne!(a, b);
}

#[test]
fn category_labels_are_translated_or_capitalised() {
    assert_eq!(category_label(FAVORITES), t!("ribbon.favorites"));
    assert_eq!(category_label("holiday"), "Holiday");
}

#[test]
fn favorite_keys_are_namespaced() {
    assert_eq!(fx_key("effects/glitch"), "fx:effects/glitch");
}

#[test]
fn every_preview_renders_on_the_headless_compositor() {
    let compositor = vv_render::Compositor::new_headless();
    let samples = Samples::new(&compositor);
    let library = assets::library();
    for kind in AssetKind::ALL {
        for asset in library.of_kind(kind) {
            for index in [0, LOOP_FRAMES / 2, LOOP_FRAMES - 1] {
                let image = render_frame(&compositor, &samples, asset, loop_frame(kind, index));
                assert!(image.is_some(), "{} frame {index}", asset.id);
            }
        }
    }
}

mod flow {
    use super::*;
    use vv_core::{Clip, ClipId, ClipSource, FrameIdx, Rational, Track, TrackKind};

    fn app() -> VenturiApp {
        let mut app = VenturiApp::default();
        let id = app.session.project.timelines.insert(vv_core::Timeline {
            name: "T".into(),
            fps: Rational::new(25, 1),
            resolution: (1920, 1080),
            tracks: vec![Track::new(TrackKind::Video), Track::new(TrackKind::Audio)],
            markers: Vec::new(),
            master: Default::default(),
        });
        app.timeline_id = Some(id);
        app
    }

    fn add(app: &mut VenturiApp, track: usize, start: FrameIdx, len: FrameIdx) -> ClipId {
        let id = app.session.project.alloc_clip_id();
        let clip =
            Clip::from_source_range(id, ClipSource::SolidColor, 0, len, start, Rational::one());
        let timeline = app.timeline_id.unwrap();
        app.session.project.timelines[timeline].tracks[track].insert_sorted(clip);
        id
    }

    fn timeline(app: &VenturiApp) -> &vv_core::Timeline {
        &app.session.project.timelines[app.timeline_id.unwrap()]
    }

    #[test]
    fn plus_on_an_effect_applies_to_the_selected_clip_in_one_undo_step() {
        let mut app = app();
        let clip = add(&mut app, 0, 0, 100);
        app.timeline_state.selected.insert((0, clip));
        app.add_fx_at_playhead("effects/glitch");
        let assets = &timeline(&app).clip(0, clip).unwrap().effects.assets;
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].id, "effects/glitch");
        app.undo();
        assert!(
            timeline(&app)
                .clip(0, clip)
                .unwrap()
                .effects
                .assets
                .is_empty()
        );
    }

    #[test]
    fn a_filter_behaves_like_an_effect() {
        let mut app = app();
        let clip = add(&mut app, 0, 0, 100);
        app.timeline_state.selected.insert((0, clip));
        app.add_fx_at_playhead("filters/noir");
        let assets = &timeline(&app).clip(0, clip).unwrap().effects.assets;
        assert_eq!(assets[0].id, "filters/noir");
        assert_eq!(assets[0].params, vec![1.25]);
    }

    #[test]
    fn plus_with_no_selection_makes_a_three_second_element_on_a_new_track() {
        let mut app = app();
        let base = add(&mut app, 0, 0, 200);
        app.timeline_state.playhead = 50;
        app.add_fx_at_playhead("effects/pulse");
        let tl = timeline(&app);
        let element_track = tl
            .tracks
            .iter()
            .position(|t| t.clips.iter().any(|c| c.is_adjustment()))
            .expect("an effect element");
        assert_ne!(element_track, 0);
        let element = &tl.tracks[element_track].clips[0];
        assert_eq!((element.timeline_start, element.timeline_len), (50, 75));
        assert_eq!(element.effects.assets[0].id, "effects/pulse");
        assert!(tl.clip(0, base).unwrap().effects.assets.is_empty());
        app.undo();
        assert!(
            timeline(&app)
                .tracks
                .iter()
                .all(|t| t.clips.iter().all(|c| !c.is_adjustment()))
        );
    }

    #[test]
    fn a_drop_on_free_space_makes_a_selected_element_on_an_effects_track() {
        let mut app = app();
        add(&mut app, 0, 0, 200);
        app.add_fx_element_at(
            "filters/noir",
            3000,
            40,
            crate::timeline_ui::MediaDropTarget::Default,
        );
        let tl = timeline(&app);
        let track = tl
            .tracks
            .iter()
            .position(|t| t.effects)
            .expect("effects track");
        let element = &tl.tracks[track].clips[0];
        assert!(element.is_effect_element());
        assert_eq!((element.timeline_start, element.timeline_len), (40, 75));
        assert!(app.timeline_state.selected.contains(&(track, element.id)));
        let target = crate::panel_target(app.timeline_id.unwrap(), track, element, 50);
        assert!(app.is_effect_element(&target));

        // A second one where there is room shares the track.
        app.add_fx_element_at(
            "effects/pulse",
            3000,
            150,
            crate::timeline_ui::MediaDropTarget::Track(track),
        );
        let tl = timeline(&app);
        assert_eq!(tl.tracks.iter().filter(|t| t.effects).count(), 1);
        assert_eq!(tl.tracks[track].clips.len(), 2);
    }

    #[test]
    fn a_transition_is_refused_between_clips_that_are_not_adjacent() {
        let mut app = app();
        let a = add(&mut app, 0, 0, 100);
        let b = add(&mut app, 0, 150, 100);
        app.timeline_state.selected.insert((0, a));
        app.timeline_state.selected.insert((0, b));
        app.add_fx_at_playhead("transitions/dissolve");
        assert!(timeline(&app).tracks[0].crossings.is_empty());
        assert_eq!(
            app.toast.as_ref().map(|(text, _)| text.as_str()),
            Some(t!("fx.no_adjacent_pair").as_ref())
        );
    }

    #[test]
    fn a_transition_goes_on_the_selected_adjacent_pair_with_the_default_length() {
        let mut app = app();
        let a = add(&mut app, 0, 0, 100);
        let b = add(&mut app, 0, 100, 100);
        app.timeline_state.selected.insert((0, a));
        app.add_fx_at_playhead("transitions/wipe");
        let crossing = &timeline(&app).tracks[0].crossings[0];
        assert_eq!((crossing.left_clip, crossing.right_clip), (a, b));
        assert_eq!(crossing.transition.duration, 13);
        assert_eq!(
            crossing.transition.asset.as_ref().map(|fx| fx.id.as_str()),
            Some("transitions/wipe")
        );
    }

    #[test]
    fn compound_and_ungroup_round_trip_loses_nothing() {
        let mut app = app();
        let a = add(&mut app, 0, 10, 60);
        app.session.project.timelines[app.timeline_id.unwrap()]
            .clip_mut(0, a)
            .unwrap()
            .effects
            .assets = vec![vv_core::AssetFx::new("filters/vivid", vec![0.4])];
        app.timeline_state.selected.insert((0, a));
        app.make_compound_from_selection();
        let tl = timeline(&app);
        assert_eq!(tl.tracks[0].clips.len(), 1);
        assert!(tl.tracks[0].clips[0].effects.assets.is_empty());

        app.timeline_state
            .selected
            .insert((0, tl.tracks[0].clips[0].id));
        app.ungroup_selected_compound();
        let tl = timeline(&app);
        assert_eq!(tl.tracks[0].clips.len(), 1);
        let back = &tl.tracks[0].clips[0];
        assert_eq!((back.timeline_start, back.timeline_len), (10, 60));
        assert_eq!(back.effects.assets[0].params, vec![0.4]);
        assert!(app.session.project.media_pool.is_empty());
    }

    #[test]
    fn ungrouping_a_plain_clip_only_shows_a_message() {
        let mut app = app();
        let a = add(&mut app, 0, 0, 50);
        app.timeline_state.selected.insert((0, a));
        app.ungroup_selected_compound();
        assert_eq!(timeline(&app).tracks[0].clips.len(), 1);
        assert!(app.toast.is_some());
    }
}
