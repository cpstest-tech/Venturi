use super::*;
use vv_media::test_support::TempDir;

/// Runs the store and the library until the sticker is a library asset.
fn materialize(app: &mut VenturiApp, id: &str) -> String {
    for _ in 0..600 {
        if let Some(store) = &mut app.stickers.store {
            store.poll();
        }
        let _ = app.library.poll();
        if let Some(asset) = app.sticker_asset_id(id) {
            return asset;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("{id} never reached the library");
}

fn app_with_store(dir: &TempDir) -> VenturiApp {
    let mut app = VenturiApp::default();
    app.stickers.store = Some(Store::new(dir.join("stickers"), &egui::Context::default()));
    app
}

fn overlay_clip(app: &VenturiApp) -> &vv_core::Clip {
    let timeline = &app.session.project.timelines[app.timeline_id.expect("a timeline")];
    timeline
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .next()
        .expect("a clip")
}

#[test]
fn a_static_catalog_sticker_becomes_a_three_second_clip() {
    let dir = TempDir::new("vv-sticker-ui-static");
    let mut app = app_with_store(&dir);
    let asset = materialize(&mut app, "shape:star");
    app.insert_assets_at_playhead(&[asset], true);
    assert_eq!(overlay_clip(&app).timeline_len, 90);
}

#[test]
fn an_animated_sticker_plays_as_a_video_with_alpha_cut_to_three_seconds() {
    let dir = TempDir::new("vv-sticker-ui-animated");
    let mut app = app_with_store(&dir);
    let asset = materialize(&mut app, "anim:ripple");
    let path = app
        .library
        .assets
        .iter()
        .find(|a| a.id == asset)
        .and_then(|a| a.item().map(|i| i.path.clone()))
        .expect("the file");
    assert!(vv_media::has_alpha(&path));
    app.insert_assets_at_playhead(&[asset], true);
    let clip = overlay_clip(&app);
    assert_eq!(clip.timeline_len, 90);
    let vv_core::ClipSource::Media(media) = clip.source else {
        panic!("a media clip")
    };
    let meta = &app.session.project.media_pool[media].meta;
    assert!(!meta.is_image(), "a video, not a still");
    assert!(meta.duration_frames as f64 / meta.fps.as_f64() >= 5.9);
}

#[test]
fn an_unknown_id_is_a_library_asset_and_passes_through() {
    let mut app = VenturiApp::default();
    assert_eq!(app.sticker_asset_id("17ab-3"), Some("17ab-3".to_owned()));
}

#[test]
fn the_sticker_color_adds_edits_and_removes_a_colorize_filter() {
    let dir = TempDir::new("vv-sticker-ui-tint");
    let mut app = app_with_store(&dir);
    let asset = materialize(&mut app, "shape:heart");
    app.insert_assets_at_playhead(&[asset], true);
    let timeline_id = app.timeline_id.expect("a timeline");
    let (track_index, clip) = {
        let timeline = &app.session.project.timelines[timeline_id];
        let (i, track) = timeline
            .tracks
            .iter()
            .enumerate()
            .find(|(_, t)| !t.clips.is_empty())
            .unwrap();
        (i, track.clips[0].clone())
    };
    let target = panel_target(timeline_id, track_index, &clip, 0);
    let info = app.clip_panel_info(target).unwrap();
    assert!(info.tintable);
    assert_eq!(info.tint, None);

    let red = to_rgba([239, 68, 68]);
    for color in [Some(red), Some(to_rgba([6, 182, 212])), None] {
        let commands = crate::properties_panel::set_tint_commands(
            Some(&app.session.project.timelines[timeline_id]),
            &[target],
            color,
        );
        app.apply_effect_changes(commands, false);
        let info = app.clip_panel_info(target).unwrap();
        assert_eq!(info.tint, color);
        let clip = app.session.project.timelines[timeline_id]
            .clip(track_index, clip.id)
            .unwrap();
        let filters = clip.effects.filters.len();
        assert_eq!(
            filters,
            usize::from(color.is_some()),
            "one filter, edited in place"
        );
    }
}

#[test]
fn swatches_are_distinct_colors() {
    assert_eq!(to_rgba([255, 0, 0]).r, 1.0);
    assert!(same_color(to_rgba([34, 197, 94]), to_rgba([34, 197, 94])));
    assert!(!same_color(
        to_rgba([255, 255, 255]),
        to_rgba([239, 68, 68])
    ));
    for (i, (_, a)) in SWATCHES.iter().enumerate() {
        for (_, b) in &SWATCHES[i + 1..] {
            assert!(!same_color(to_rgba(*a), to_rgba(*b)));
        }
    }
}
