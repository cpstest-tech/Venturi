use super::*;

fn generate_button_bottom(height: f32, selected: bool) -> Option<f32> {
    let mut app = VenturiApp::default();
    let id = app.session.project.timelines.insert(vv_core::Timeline {
        name: "T".into(),
        fps: vv_core::Rational::new(25, 1),
        resolution: (1080, 1920),
        tracks: vec![vv_core::Track::new(vv_core::TrackKind::Audio)],
        markers: Vec::new(),
        master: Default::default(),
    });
    app.timeline_id = Some(id);
    app.subtitles.section = Section::Templates;
    app.subtitles.selected = selected.then(|| "word_pop".to_owned());
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(700.0, height),
        )),
        ..Default::default()
    };
    let label = t!("subtitles.generate").into_owned();
    let mut bottom = None;
    for _ in 0..2 {
        let mut output = ctx.run_ui(raw.clone(), |ui| app.show_subtitles_tab(ui));
        output.textures_delta.clear();
        // The sidebar entry has the same label: the button is the lowest.
        bottom = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.visual_bounding_rect().bottom())
                }
                _ => None,
            })
            .max_by(f32::total_cmp)
            .filter(|bottom| *bottom > 40.0);
    }
    bottom
}

#[test]
fn generate_button_stays_visible_without_scrolling() {
    for height in [260.0, 320.0, 480.0] {
        for selected in [false, true] {
            let bottom = generate_button_bottom(height, selected)
                .unwrap_or_else(|| panic!("no button at {height}"));
            assert!(bottom <= height, "button at {bottom} past {height}");
        }
    }
}

fn pixels(title: &vv_core::TitleParams) -> Vec<egui::Color32> {
    square_preview_image(title, text_chars(title)).pixels
}

#[test]
fn hovering_an_animated_card_plays_its_animation_and_loops() {
    let sample = "Ciao";
    for template in vv_core::captions::TEMPLATES.iter() {
        let id = template.id;
        let title = vv_core::captions::SubtitleTemplate::find(id)
            .unwrap()
            .title(sample, PREVIEW_RES);
        let rest = pixels(&title);
        let early = pixels(&hover_frame(&title, 0.08));
        let settled = pixels(&hover_frame(&title, 1.0));
        let looped = pixels(&hover_frame(
            &title,
            PREVIEW_CLIP_SECS + PREVIEW_PAUSE_SECS + 0.08,
        ));
        assert_ne!(early, rest, "{id} does not animate");
        assert_eq!(settled, rest, "{id} does not settle");
        assert_ne!(looped, rest, "{id} does not loop");
    }
}

#[test]
fn hovering_a_word_template_plays_the_words_one_after_another() {
    let template = SubtitleTemplate::find("word_pop").unwrap();
    let sample = "Ciao a tutti";
    let word = vv_core::captions::PREVIEW_WORD_SECS;
    let text_at = |secs: f32| template_hover_frame(template, sample, secs).content.default;
    assert_eq!(text_at(0.1), "Ciao");
    assert_eq!(text_at(word + 0.1), "a");
    assert_eq!(text_at(2.0 * word + 0.1), "tutti");
    let chars = widest_hover_text(template, sample);
    assert_eq!(chars, 5);
    let image = |secs: f32| {
        square_preview_image(&template_hover_frame(template, sample, secs), chars).pixels
    };
    for n in 0..3 {
        let start = n as f32 * word;
        assert_ne!(
            image(start + 0.03),
            image(start + word - 0.01),
            "word {n} does not pop"
        );
    }
    let period = SubtitleTemplate::preview_secs(sample) + PREVIEW_PAUSE_SECS;
    assert_eq!(text_at(period + 0.1), "Ciao", "does not loop");
}

#[test]
fn subtitle_templates_use_a_bundled_font() {
    crate::fonts::register_bundled();
    let families = vv_render::text::font_families();
    for template in TEMPLATES.iter() {
        let family = template.title("Ciao", PREVIEW_RES).font_family;
        assert!(
            families.contains(&family),
            "{}: {family} not bundled",
            template.id
        );
    }
}
