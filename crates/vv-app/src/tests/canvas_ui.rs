use super::*;

#[test]
fn a_picked_image_becomes_the_canvas_in_one_undo_step() {
    let mut app = VenturiApp::default();
    let timeline = app.ensure_timeline();
    let mut clips = Vec::new();
    for start in [0, 10] {
        let id = app.session.project.alloc_clip_id();
        app.session.project.timelines[timeline].tracks[0]
            .clips
            .push(vv_core::Clip::from_source_range(
                id,
                vv_core::ClipSource::SolidColor,
                0,
                10,
                start,
                vv_core::Rational::one(),
            ));
        clips.push((timeline, 0, id));
    }
    let path = PathBuf::from("/nonexistent/back.png");
    app.set_canvas_image(clips, path.clone());
    let canvases = |app: &VenturiApp| -> Vec<_> {
        app.session.project.timelines[timeline].tracks[0]
            .clips
            .iter()
            .map(|c| c.effects.canvas.clone())
            .collect()
    };
    let expected = Some(Canvas::Image(path));
    assert_eq!(canvases(&app), [expected.clone(), expected]);
    app.undo();
    assert_eq!(canvases(&app), [None, None]);
}
