use super::*;
use vv_core::{Clip, MediaItem, MediaMeta, Rational, Track};

fn media(app: &mut VenturiApp, path: &str, video: bool, audio: bool) -> vv_core::MediaId {
    app.session.project.media_pool.insert(MediaItem {
        path: path.into(),
        meta: MediaMeta {
            duration_frames: 1000,
            fps: Rational::new(25, 1),
            width: if video { 1080 } else { 0 },
            height: if video { 1920 } else { 0 },
            has_video: video,
            has_audio: audio,
            sample_rate: if audio { 48_000 } else { 0 },
            channels: u16::from(audio),
            audio_streams: u16::from(audio),
            file: Default::default(),
        },
        content_hash: path.len() as u64,
        compound: None,
        folder: None,
    })
}

fn clip(app: &mut VenturiApp, media: vv_core::MediaId, start: FrameIdx, len: FrameIdx) -> Clip {
    let id = app.session.project.alloc_clip_id();
    Clip::from_source_range(id, ClipSource::Media(media), 0, len, start, Rational::one())
}

/// Three shots back to back (the second already animated by the user), a
/// title above, music with a flat volume and music with a hand-made fade.
fn edited_app() -> (VenturiApp, TimelineId) {
    let mut app = VenturiApp::default();
    let video = media(&mut app, "/tmp/vv-pack-shot.mp4", true, false);
    let song = media(&mut app, "/tmp/vv-pack-song.mp3", false, true);
    let mut main = Track::new_main();
    for i in 0..3 {
        let mut shot = clip(&mut app, video, i * 50, 50);
        if i == 1 {
            shot.effects.animations.in_asset = Some("animations/zoom_2".into());
        }
        main.clips.push(shot);
    }
    let mut titles = Track::new(TrackKind::Video);
    let id = app.session.project.alloc_clip_id();
    let mut title = Clip::from_source_range(id, ClipSource::Text, 0, 40, 10, Rational::one());
    title.effects.title = Some(vv_core::TitleParams::default());
    titles.clips.push(title);
    let mut music = Track::new(TrackKind::Audio);
    music.clips.push(clip(&mut app, song, 0, 150));
    let mut faded = Track::new(TrackKind::Audio);
    let mut hand = clip(&mut app, song, 0, 150);
    hand.effects
        .gain_db
        .upsert(100, -20.0, vv_core::Interpolation::Linear);
    faded.clips.push(hand);
    let timeline = app.session.project.timelines.insert(vv_core::Timeline {
        name: "T".into(),
        fps: Rational::new(25, 1),
        resolution: (1080, 1920),
        tracks: vec![main, titles, music, faded],
        markers: Vec::new(),
        master: Default::default(),
    });
    app.timeline_id = Some(timeline);
    (app, timeline)
}

fn pack() -> StylePack {
    StylePack {
        animations: Some(Animations {
            clip_in: Some("animations/flash_in".into()),
            text_in: Some("pop".into()),
            ..Animations::default()
        }),
        music: Some(Music::default()),
        transition: Some(TransitionStyle {
            asset: "transitions/dissolve".into(),
            seconds: 0.4,
        }),
        intro: Some(Bumper {
            template: "neon_pulse".into(),
            text: "Benvenuti".into(),
        }),
        safe_zone: Some(SafeZone {
            platform: "reels".into(),
        }),
        ..StylePack::new("test", "Test")
    }
}

fn snapshot(app: &VenturiApp, timeline: TimelineId) -> String {
    format!("{:?}", app.session.project.timelines[timeline])
}

#[test]
fn every_built_in_pack_refers_to_parts_that_exist() {
    let app = VenturiApp::default();
    let packs = builtin_packs();
    assert!(packs.len() >= 3);
    for pack in &packs {
        assert_eq!(app.missing_pack_parts(pack), Vec::new(), "{}", pack.id);
        assert!(pack.components().len() >= 4, "{}", pack.id);
        let reread = vv_core::style_pack::parse(pack.to_json().as_bytes()).unwrap();
        assert_eq!(&reread, pack);
    }
}

#[test]
fn missing_parts_are_named_and_skipped() {
    let (mut app, _) = edited_app();
    let mut pack = pack();
    pack.transition.as_mut().unwrap().asset = "no_such_transition".into();
    let missing = app.missing_pack_parts(&pack);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].0, Component::Transition);
    let report = app.apply_style_pack(&pack, &BTreeSet::from([Component::Transition]));
    assert!(matches!(
        report[0],
        (Component::Transition, Outcome::Skipped(_))
    ));
}

#[test]
fn applying_a_pack_is_one_undo_step_that_keeps_the_user_work() {
    let (mut app, timeline) = edited_app();
    let before = snapshot(&app, timeline);
    let steps = app.session.history.position();
    let choice: BTreeSet<Component> = pack().components().into_iter().collect();
    let report = app.apply_style_pack(&pack(), &choice);
    assert_eq!(app.session.history.position(), steps + 1);
    for (component, outcome) in &report {
        assert!(matches!(outcome, Outcome::Applied(_)), "{component:?}");
    }
    let tl = &app.session.project.timelines[timeline];
    assert_eq!(tl.tracks[0].crossings.len(), 2);
    let anim = |i: usize| tl.tracks[0].clips[i].effects.animations.in_asset.clone();
    assert_eq!(anim(0).as_deref(), Some("animations/flash_in"));
    assert_eq!(anim(1).as_deref(), Some("animations/zoom_2"));
    let title = tl.tracks[1].clips[0].effects.title.as_ref().unwrap();
    assert_eq!(
        title.anim.enter.as_ref().map(|a| a.id.as_str()),
        Some("pop")
    );
    let flat = &tl.tracks[2].clips[0].effects.gain_db;
    assert!(flat.is_constant());
    assert_eq!(flat.default, Music::default().volume_db);
    assert_eq!(tl.tracks[3].clips[0].effects.gain_db.keyframes().len(), 1);
    let intro = tl.tracks[4..]
        .iter()
        .flat_map(|t| &t.clips)
        .find_map(|c| c.effects.title.as_ref().filter(|_| c.timeline_start == 0))
        .expect("intro title at the start");
    assert_eq!(intro.content.default, "Benvenuti");
    assert!(app.settings.shorts.safe_zones);
    app.session.history.undo(&mut app.session.project);
    assert_eq!(snapshot(&app, timeline), before);
}

#[test]
fn a_preview_cancels_back_to_the_edit_or_is_confirmed() {
    let (mut app, timeline) = edited_app();
    app.style_packs.entries = vec![Entry {
        pack: pack(),
        path: None,
    }];
    app.select_style_pack("test");
    let before = snapshot(&app, timeline);
    app.preview_style_pack();
    assert_ne!(snapshot(&app, timeline), before);
    assert!(app.settings.shorts.safe_zones);
    // Unticking a part applies the rest again in place of the first preview.
    app.style_packs.choice.remove(&Component::Intro);
    app.preview_style_pack();
    let tracks = app.session.project.timelines[timeline].tracks.len();
    assert_eq!(tracks, 4);
    app.cancel_style_pack();
    assert_eq!(snapshot(&app, timeline), before);
    assert!(!app.settings.shorts.safe_zones);
    app.preview_style_pack();
    app.confirm_style_pack();
    assert!(app.style_packs.preview.is_none());
    assert_ne!(snapshot(&app, timeline), before);
    assert!(app.settings.shorts.safe_zones);
}

#[test]
fn the_library_saves_duplicates_renames_imports_exports_and_deletes() {
    let dir = std::env::temp_dir().join(format!("vv-style-packs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("style-packs")).unwrap();
    std::fs::write(dir.join("style-packs/broken.json"), b"{ nope").unwrap();
    let newer = pack()
        .to_json()
        .replace("\"version\": 1", "\"version\": 99");
    std::fs::write(dir.join("style-packs/newer.json"), newer).unwrap();
    let (mut app, _) = edited_app();
    app.settings_path = Some(dir.join("settings.json"));
    app.reload_style_packs();
    let builtins = builtin_packs().len();
    assert_eq!(app.style_packs.entries.len(), builtins);
    assert_eq!(app.style_packs.load_errors.len(), 2);

    let captured = app.capture_style_pack();
    let id = captured.id.clone();
    app.save_style_pack(captured).unwrap();
    app.duplicate_style_pack(&id);
    app.rename_style_pack(&id, "Mio stile");
    app.reload_style_packs();
    assert_eq!(app.style_packs.entries.len(), builtins + 2);
    assert_eq!(app.style_pack(&id).unwrap().pack.name, "Mio stile");

    let file = dir.join("exported.json");
    app.export_style_pack(&id, &file);
    app.import_style_pack(&file);
    assert_eq!(app.style_packs.entries.len(), builtins + 3);
    let imported = app.style_packs.selected.clone().unwrap();
    assert_ne!(imported, id);
    assert_eq!(app.style_pack(&imported).unwrap().pack.name, "Mio stile");

    app.import_style_pack(&dir.join("style-packs/newer.json"));
    assert!(matches!(app.style_packs.message, Some((true, _))));
    app.delete_style_pack(&id);
    app.delete_style_pack("builtin.clean");
    app.reload_style_packs();
    assert_eq!(app.style_packs.entries.len(), builtins + 2);
    assert!(app.style_pack("builtin.clean").is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn capturing_reads_the_look_of_the_edit() {
    let (mut app, timeline) = edited_app();
    app.apply_style_pack(
        &pack(),
        &BTreeSet::from([Component::Transition, Component::Animations]),
    );
    app.settings.shorts.safe_zones = true;
    let captured = app.capture_style_pack();
    assert_eq!(
        captured.transition.as_ref().map(|t| t.asset.as_str()),
        Some("transitions/dissolve")
    );
    let anim = captured.animations.unwrap();
    assert_eq!(anim.clip_in.as_deref(), Some("animations/flash_in"));
    assert_eq!(anim.text_in.as_deref(), Some("pop"));
    assert!(captured.music.is_some());
    assert!(captured.safe_zone.is_some());
    assert!(app.session.project.timelines.contains_key(timeline));
}
