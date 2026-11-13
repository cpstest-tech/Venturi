use super::*;

fn edit(session: &mut vv_session::Session) -> vv_session::ChangeMark {
    session.create_timeline(vv_core::Timeline {
        name: "T".into(),
        fps: vv_core::Rational::new(25, 1),
        resolution: (64, 48),
        tracks: Vec::new(),
        markers: Vec::new(),
        master: Default::default(),
    });
    session.change_mark()
}

#[test]
fn a_save_is_due_two_seconds_after_the_last_edit() {
    let mut session = vv_session::Session::default();
    let mut autosave = Autosave::default();
    let t0 = Instant::now();
    autosave.observe(session.change_mark(), false, t0);
    assert!(!autosave.due(t0 + Duration::from_secs(60)));

    let mark = edit(&mut session);
    autosave.observe(mark, true, t0);
    assert!(!autosave.due(t0 + Duration::from_millis(1900)));
    assert!(autosave.due(t0 + DEBOUNCE));
    assert_eq!(autosave.next_check(t0), Some(DEBOUNCE));
}

#[test]
fn continuous_edits_still_save_every_thirty_seconds() {
    let mut session = vv_session::Session::default();
    let mut autosave = Autosave::default();
    let t0 = Instant::now();
    for second in 0..30 {
        let now = t0 + Duration::from_secs(second);
        autosave.observe(edit(&mut session), true, now);
        assert!(!autosave.due(now), "second {second}");
    }
    assert!(autosave.due(t0 + MAX_INTERVAL));
}

#[test]
fn a_failed_save_waits_before_retrying() {
    let mut session = vv_session::Session::default();
    let mut autosave = Autosave::default();
    let t0 = Instant::now();
    autosave.observe(edit(&mut session), true, t0);
    let failed_at = t0 + DEBOUNCE;
    autosave.failed("disk full".into(), failed_at);
    assert!(!autosave.due(failed_at + Duration::from_secs(1)));
    assert!(autosave.due(failed_at + RETRY_AFTER));
    autosave.succeeded();
    assert!(autosave.error.is_none());
}

#[test]
fn nothing_is_due_once_the_edits_are_on_disk() {
    let mut session = vv_session::Session::default();
    let mut autosave = Autosave::default();
    let t0 = Instant::now();
    let mark = edit(&mut session);
    autosave.observe(mark, true, t0);
    autosave.observe(mark, false, t0 + Duration::from_secs(5));
    assert!(!autosave.due(t0 + Duration::from_secs(60)));
    assert_eq!(autosave.next_check(t0), None);
}

#[test]
fn flushing_writes_the_pending_edits_to_the_project_file() {
    let dir = vv_media::test_support::TempDir::new("vv-app-autosave-flush-test");
    let path = dir.join("p.vvproj");
    let mut app = VenturiApp::default();
    app.save_project_to(&path);
    app.create_timeline("Edited".into(), vv_core::Rational::new(25, 1), (64, 48));
    assert_eq!(app.save_status(), SaveStatus::Saving);

    assert!(app.flush_autosave());

    assert!(!app.has_unsaved_changes());
    assert_eq!(app.save_status(), SaveStatus::Saved);
    let saved = vv_core::load_project(&path).unwrap();
    assert!(saved.timelines.values().any(|t| t.name == "Edited"));
}

#[test]
fn a_background_save_is_waited_for_and_marks_the_project_saved() {
    let dir = vv_media::test_support::TempDir::new("vv-app-autosave-background-test");
    let path = dir.join("p.vvproj");
    let mut app = VenturiApp::default();
    app.save_project_to(&path);
    app.create_timeline("Background".into(), vv_core::Rational::new(25, 1), (64, 48));
    app.start_background_save(path.clone());
    assert_eq!(app.save_status(), SaveStatus::Saving);

    assert!(app.flush_autosave());

    assert!(!app.has_unsaved_changes());
    let saved = vv_core::load_project(&path).unwrap();
    assert!(saved.timelines.values().any(|t| t.name == "Background"));
}
