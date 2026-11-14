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
