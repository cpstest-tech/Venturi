use super::*;
use vv_media::test_support::{TempDir, ffmpeg};

fn video(dir: &TempDir, name: &str, size: &str, rate: u32) -> PathBuf {
    let path = dir.join(name);
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=size={size}:rate={rate}:duration=2"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &path,
    );
    path
}

fn picture(dir: &TempDir, name: &str, size: &str) -> PathBuf {
    let path = dir.join(name);
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=size={size}:rate=1:duration=1"),
            "-frames:v",
            "1",
        ],
        &path,
    );
    path
}

fn timeline_of(app: &VenturiApp) -> &vv_core::Timeline {
    &app.session.project.timelines[app.timeline_id.expect("a timeline")]
}

fn pooled(app: &mut VenturiApp, path: &Path) -> MediaId {
    app.pool_media_for_path(path).expect("probed")
}

#[test]
fn every_tab_lists_favorites_first() {
    for tab in &RibbonTab::ALL[..4] {
        let categories = tab.categories();
        assert_eq!(categories[0], Category::Favorites, "{tab:?}");
        assert!(categories.len() > 1, "{tab:?}");
    }
}

#[test]
fn only_audio_and_sticker_categories_have_a_library_folder() {
    let mut ids = Vec::new();
    for tab in RibbonTab::ALL {
        for category in tab.categories() {
            let expects_folder = matches!(tab, RibbonTab::Audio | RibbonTab::Sticker)
                && category != Category::Favorites;
            assert_eq!(
                category.folder_id().is_some(),
                expects_folder,
                "{category:?}"
            );
            ids.extend(category.folder_id());
        }
    }
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len());
}

#[test]
fn toggling_a_favorite_adds_then_removes_it() {
    let mut favorites = Vec::new();
    let key = media_key("/videos/a.mp4");
    toggle_favorite(&mut favorites, &key);
    assert!(is_favorite(&favorites, &key));
    toggle_favorite(&mut favorites, &asset_key("x"));
    toggle_favorite(&mut favorites, &key);
    assert!(!is_favorite(&favorites, &key));
    assert!(is_favorite(&favorites, &asset_key("x")));
}

#[test]
fn favorite_keys_do_not_collide_across_kinds() {
    assert_ne!(media_key("a"), asset_key("a"));
    assert_ne!(asset_key("a"), text_key("a"));
}

#[test]
fn the_import_category_follows_the_tab() {
    let mut app = VenturiApp::default();
    assert_eq!(app.ribbon_import_category(), None);
    app.library_ribbon.tab = RibbonTab::Audio;
    assert_eq!(app.ribbon_import_category(), Some(Category::Music));
    app.library_ribbon
        .selected
        .insert(RibbonTab::Audio, Category::Sfx);
    assert_eq!(app.ribbon_import_category(), Some(Category::Sfx));
    app.library_ribbon
        .selected
        .insert(RibbonTab::Audio, Category::Favorites);
    assert_eq!(app.ribbon_import_category(), Some(Category::Music));
    app.library_ribbon.tab = RibbonTab::Sticker;
    assert_eq!(
        app.ribbon_import_category(),
        Some(Category::Sticker(StickerCategory::Shapes))
    );
    app.library_ribbon.tab = RibbonTab::Text;
    assert_eq!(app.ribbon_import_category(), None);
}

#[test]
fn svg_and_gif_stickers_are_flagged_not_imported() {
    let mut app = VenturiApp::default();
    app.import_into_category(
        vec![PathBuf::from("/x/a.svg"), PathBuf::from("/x/b.gif")],
        Category::Sticker(StickerCategory::Other),
    );
    assert_eq!(app.import_warnings.len(), 2);
    assert!(app.library.assets.is_empty());
}

#[test]
fn audio_import_refuses_other_kinds_of_file() {
    let mut app = VenturiApp::default();
    app.import_into_category(vec![PathBuf::from("/x/a.png")], Category::Music);
    assert_eq!(app.import_warnings.len(), 1);
}

#[test]
fn importing_only_fills_the_pool() {
    let dir = TempDir::new("vv-ribbon-import");
    let path = video(&dir, "a.mp4", "64x48", 25);
    let mut app = VenturiApp::default();
    app.import_media(path);
    assert!(app.timeline_id.is_none());
    assert!(app.session.project.timelines.is_empty());
    assert_eq!(app.session.project.media_pool.len(), 1);
}

#[test]
fn the_first_media_inserted_defines_resolution_and_fps() {
    let dir = TempDir::new("vv-ribbon-first");
    let first = video(&dir, "first.mp4", "96x54", 24);
    let second = video(&dir, "second.mp4", "64x48", 50);
    let mut app = VenturiApp::default();
    let first = pooled(&mut app, &first);
    let second = pooled(&mut app, &second);
    app.insert_pool_media_at_playhead(first);
    assert_eq!(timeline_of(&app).resolution, (96, 54));
    assert_eq!(timeline_of(&app).fps, vv_core::Rational::new(24, 1));
    app.insert_pool_media_at_playhead(second);
    assert_eq!(timeline_of(&app).resolution, (96, 54));
    assert_eq!(timeline_of(&app).fps, vv_core::Rational::new(24, 1));
}

#[test]
fn a_photo_as_first_media_sets_30_fps_and_lasts_five_seconds() {
    let dir = TempDir::new("vv-ribbon-photo");
    let path = picture(&dir, "photo.png", "80x60");
    let mut app = VenturiApp::default();
    let media = pooled(&mut app, &path);
    app.insert_pool_media_at_playhead(media);
    let timeline = timeline_of(&app);
    assert_eq!(timeline.fps, vv_core::Rational::new(30, 1));
    assert_eq!(timeline.resolution, (80, 60));
    assert_eq!(timeline.total_frames(), 150);
}

#[test]
fn a_rotated_file_sets_the_oriented_size_and_turns_its_clip() {
    let dir = TempDir::new("vv-ribbon-rotation");
    let path = video(&dir, "phone.mp4", "96x54", 25);
    let mut app = VenturiApp::default();
    app.media_rotation.insert(path.clone(), 90);
    let media = pooled(&mut app, &path);
    app.insert_pool_media_at_playhead(media);
    let timeline = timeline_of(&app);
    assert_eq!(timeline.resolution, (54, 96));
    let clip = &timeline.tracks[timeline.main_track_index().unwrap()].clips[0];
    let param = |p| clip.effects.transform.track(p).default;
    assert_eq!(param(vv_core::TransformParam::Rotation), 90.0);
    let zoom = param(vv_core::TransformParam::ZoomX);
    assert!((zoom - 96.0 / 54.0).abs() < 1e-3, "{zoom}");
    assert_eq!(zoom, param(vv_core::TransformParam::ZoomY));
}

#[test]
fn a_file_without_rotation_keeps_an_identity_transform() {
    let dir = TempDir::new("vv-ribbon-upright");
    let path = video(&dir, "plain.mp4", "96x54", 25);
    let mut app = VenturiApp::default();
    app.media_rotation.insert(path.clone(), 0);
    let media = pooled(&mut app, &path);
    app.insert_pool_media_at_playhead(media);
    let timeline = timeline_of(&app);
    let clip = &timeline.tracks[timeline.main_track_index().unwrap()].clips[0];
    assert_eq!(
        clip.effects
            .transform
            .track(vv_core::TransformParam::Rotation)
            .default,
        0.0
    );
}

#[test]
fn the_plus_inserts_at_the_playhead_in_one_undo_step() {
    let dir = TempDir::new("vv-ribbon-plus");
    let path = video(&dir, "a.mp4", "64x48", 25);
    let mut app = VenturiApp::default();
    let media = pooled(&mut app, &path);
    app.timeline_state.magnet = false;
    app.timeline_state.playhead = 10;
    app.insert_pool_media_at_playhead(media);
    let timeline = timeline_of(&app);
    let starts: Vec<_> = timeline
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .map(|c| c.timeline_start)
        .collect();
    assert!(starts.contains(&10), "{starts:?}");
    let before = app.session.history.position();
    app.undo();
    assert_eq!(app.session.history.position(), before - 1);
    assert!(app.session.project.timelines.is_empty());
}

#[test]
fn a_sticker_lasts_three_seconds_on_an_overlay_track() {
    let dir = TempDir::new("vv-ribbon-sticker");
    let base = video(&dir, "base.mp4", "64x48", 25);
    let sticker = picture(&dir, "star.png", "32x32");
    let mut app = VenturiApp::default();
    let base = pooled(&mut app, &base);
    let sticker = pooled(&mut app, &sticker);
    app.insert_pool_media_at_playhead(base);
    app.insert_at_playhead(&[sticker], true);
    let timeline = timeline_of(&app);
    let main = timeline.main_track_index().unwrap();
    let overlay = timeline
        .tracks
        .iter()
        .enumerate()
        .find(|(i, t)| *i != main && t.kind == TrackKind::Video && !t.clips.is_empty())
        .map(|(_, t)| t)
        .expect("an overlay track");
    assert_eq!(overlay.clips[0].timeline_len, 75);
}

#[test]
fn a_sticker_alone_defines_the_project_like_a_photo() {
    let dir = TempDir::new("vv-ribbon-first-sticker");
    let sticker = picture(&dir, "star.png", "32x32");
    let mut app = VenturiApp::default();
    let sticker = pooled(&mut app, &sticker);
    app.insert_at_playhead(&[sticker], true);
    let timeline = timeline_of(&app);
    assert_eq!(timeline.fps, vv_core::Rational::new(30, 1));
    assert_eq!(timeline.total_frames(), 90);
}

#[test]
fn text_lasts_three_seconds() {
    let mut app = VenturiApp::default();
    app.insert_text_at_playhead(None);
    let timeline = timeline_of(&app);
    let clip = timeline
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .find(|c| matches!(c.source, vv_core::ClipSource::Text))
        .expect("a text clip");
    assert_eq!(
        clip.timeline_len,
        (timeline.fps.as_f64() * 3.0).round() as vv_core::FrameIdx
    );
    assert_eq!(clip.effects.title, Some(vv_core::TitleParams::default()));
}

#[test]
fn a_saved_template_styles_the_inserted_text() {
    let mut app = VenturiApp::default();
    let title = vv_core::TitleParams {
        size: 123.0,
        ..Default::default()
    };
    app.save_text_template(title.clone());
    assert_eq!(app.settings.text_templates.len(), 1);
    let id = app.settings.text_templates[0].id.clone();
    app.insert_text_at_playhead(Some(&id));
    let timeline = timeline_of(&app);
    let clip = timeline
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .find(|c| matches!(c.source, vv_core::ClipSource::Text))
        .expect("a text clip");
    assert_eq!(clip.effects.title, Some(title));
}

#[test]
fn dropping_a_template_card_uses_its_style_once() {
    let mut app = VenturiApp::default();
    let title = vv_core::TitleParams {
        size: 77.0,
        ..Default::default()
    };
    app.save_text_template(title.clone());
    app.library_ribbon.dragged_template = Some(app.settings.text_templates[0].id.clone());
    app.add_generator_to_timeline_at(
        timeline_ui::Generator::Text,
        0,
        timeline_ui::MediaDropTarget::Default,
    );
    assert!(app.library_ribbon.dragged_template.is_none());
    let timeline = timeline_of(&app);
    let clip = &timeline.tracks[0].clips[0];
    assert_eq!(clip.effects.title, Some(title));
}

#[test]
fn favorites_and_templates_survive_a_restart() {
    let dir = TempDir::new("vv-ribbon-settings");
    let path = dir.join("settings.json");
    let mut settings = Settings::default();
    toggle_favorite(&mut settings.favorites, &media_key("/videos/a.mp4"));
    toggle_favorite(&mut settings.favorites, &asset_key("abc"));
    settings.text_templates.push(TextTemplate {
        id: "t1".into(),
        name: "Style 1".into(),
        title: vv_core::TitleParams::default(),
    });
    settings.save(&path).unwrap();
    let loaded = Settings::load(&path);
    assert_eq!(loaded.favorites, settings.favorites);
    assert_eq!(loaded.text_templates, settings.text_templates);
}

#[test]
fn every_ribbon_string_is_translated() {
    let keys = [
        "ribbon.tab_media",
        "ribbon.tab_audio",
        "ribbon.tab_text",
        "ribbon.tab_sticker",
        "ribbon.favorites",
        "ribbon.import",
        "ribbon.media_empty",
        "ribbon.save_template",
    ];
    for locale in rust_i18n::available_locales!() {
        for key in keys {
            assert!(
                crate::_rust_i18n_try_translate(&locale, key).is_some(),
                "{locale}: {key}"
            );
        }
    }
    for tab in RibbonTab::ALL {
        for category in tab.categories() {
            assert!(!category.label().starts_with("ribbon."), "{category:?}");
        }
    }
}

#[test]
fn the_text_preview_draws_the_title_in_its_color() {
    let mut title = vv_core::TitleParams::default();
    title.color = vv_core::Rgba {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    let image = title_preview_image(&title, (1920, 1080));
    let reddish = image
        .pixels
        .iter()
        .filter(|p| p.r() > 200 && p.g() < 60)
        .count();
    assert!(reddish > 20, "{reddish}");
    assert_eq!(image.pixels[0], egui::Color32::from_rgb(34, 34, 38));
}

#[test]
fn the_new_ribbon_tabs_have_no_categories_to_show() {
    for tab in [
        RibbonTab::Effects,
        RibbonTab::Curves,
        RibbonTab::Mixer,
        RibbonTab::Color,
    ] {
        let state = RibbonState {
            tab,
            ..Default::default()
        };
        assert_eq!(state.category(), Category::Favorites);
    }
}
