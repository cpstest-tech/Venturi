use super::*;

fn word(text: &str, start: f64, end: f64, clip: usize, local_index: usize) -> TWord {
    TWord {
        text: text.into(),
        original: text.into(),
        source_ms: 0,
        start,
        end,
        confidence: None,
        clip,
        local_index,
        mark: Mark::Speech,
    }
}

fn pause(clip: usize, start: f64, end: f64) -> TPause {
    TPause {
        clip,
        start,
        end,
        local: start..end,
    }
}

fn model(words: Vec<TWord>, pauses: Vec<TPause>) -> Model {
    let items = interleave(&words, &pauses);
    Model {
        timeline: <TimelineId as vv_core::Id>::from_raw(1),
        stamp: (0, 0, 0),
        clips: Vec::new(),
        words,
        pauses,
        items,
        missing: Vec::new(),
        unreadable: 0,
        candidates: 1,
    }
}

#[test]
fn normalize_keeps_letters_and_digits_lowercase() {
    assert_eq!(normalize("Ciao,"), "ciao");
    assert_eq!(normalize("«Perché?»"), "perché");
    assert_eq!(normalize("3D!"), "3d");
}

#[test]
fn search_finds_words_and_phrases_ignoring_case_and_punctuation() {
    let words = vec![
        word("Ciao,", 0.0, 0.3, 0, 0),
        word("mondo.", 0.3, 0.6, 0, 1),
        word("Il", 1.0, 1.1, 0, 2),
        word("mondo", 1.1, 1.4, 0, 3),
    ];
    assert_eq!(search(&words, "MONDO"), vec![1..2, 3..4]);
    assert_eq!(search(&words, "ciao mon"), vec![0..2]);
    assert_eq!(search(&words, "il mondo"), vec![2..4]);
    assert!(search(&words, "   ").is_empty());
    assert!(search(&words, "mare").is_empty());
}

#[test]
fn pauses_sit_before_the_word_they_precede_within_their_clip() {
    let words = vec![
        word("a", 0.0, 0.5, 0, 0),
        word("b", 2.0, 2.5, 0, 1),
        word("c", 3.0, 3.5, 1, 0),
    ];
    let pauses = vec![pause(0, 0.6, 1.9), pause(0, 2.6, 2.9), pause(1, 3.6, 4.0)];
    assert_eq!(
        interleave(&words, &pauses),
        vec![
            Item::Word(0),
            Item::Pause(0),
            Item::Word(1),
            Item::Pause(1),
            Item::Word(2),
            Item::Pause(2),
        ]
    );
}

#[test]
fn the_current_word_follows_the_playhead_and_gaps_have_none() {
    let m = model(
        vec![word("a", 0.0, 0.5, 0, 0), word("b", 1.0, 1.5, 0, 1)],
        Vec::new(),
    );
    assert_eq!(m.word_at(0.2), Some(0));
    assert_eq!(m.word_at(0.7), None);
    assert_eq!(m.word_at(1.0), Some(1));
    assert_eq!(m.word_at(2.0), None);
}

#[test]
fn marked_items_select_false_starts_and_pauses() {
    let mut words = vec![word("io", 0.0, 0.3, 0, 0), word("io", 1.0, 1.3, 0, 1)];
    words[0].mark = Mark::FalseStart;
    let m = model(words, vec![pause(0, 0.4, 0.9)]);
    assert_eq!(
        m.marked_items(Some(Mark::FalseStart), false),
        BTreeSet::from([0])
    );
    assert_eq!(m.marked_items(None, true), BTreeSet::from([1]));
}

fn speech_app() -> (crate::VenturiApp, TimelineId, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("vv-transcript-{}.wav", std::process::id()));
    std::fs::write(&path, b"not really audio").unwrap();
    let mut app = crate::VenturiApp::default();
    let media = app.session.project.media_pool.insert(vv_core::MediaItem {
        path: path.clone(),
        meta: vv_core::MediaMeta {
            duration_frames: 250,
            fps: vv_core::Rational::new(25, 1),
            width: 0,
            height: 0,
            has_video: false,
            has_audio: true,
            sample_rate: 48_000,
            channels: 1,
            audio_streams: 1,
            file: Default::default(),
        },
        content_hash: 7,
        compound: None,
        folder: None,
    });
    let id = app.session.project.alloc_clip_id();
    let clip = Clip::from_source_range(
        id,
        ClipSource::Media(media),
        0,
        250,
        0,
        vv_core::Rational::one(),
    );
    let mut main = vv_core::Track::new_main();
    main.clips.push(clip.clone());
    let timeline = app.session.project.timelines.insert(vv_core::Timeline {
        name: "T".into(),
        fps: vv_core::Rational::new(25, 1),
        resolution: (1080, 1920),
        tracks: vec![main],
        markers: Vec::new(),
        master: Default::default(),
    });
    app.timeline_id = Some(timeline);
    let w = |text: &str, start: f64, end: f64| Word {
        text: text.into(),
        start,
        end,
        confidence: Some(0.9),
    };
    let words = vec![
        w("Allora", 0.5, 0.9),
        w("ehm", 1.5, 1.8),
        w("oggi", 2.5, 2.9),
        w("parliamo", 3.0, 3.6),
        w("di", 3.6, 3.7),
        w("montaggio.", 3.7, 4.4),
    ];
    let db = (0..1000)
        .map(|i| {
            let t = i as f64 / 100.0;
            if words.iter().any(|w| (w.start..w.end).contains(&t)) {
                -20.0
            } else {
                -60.0
            }
        })
        .collect();
    let source = std::sync::Arc::new(crate::auto_cut::Cached::new(
        10.0,
        words,
        Energy { step: 0.01, db },
    ));
    let key = app
        .speech_source(timeline, &clip)
        .and_then(|s| s.key)
        .expect("readable source");
    app.remember_speech(&key, &source);
    (app, timeline, path)
}

fn main_geometry(app: &crate::VenturiApp, timeline: TimelineId) -> Vec<(FrameIdx, FrameIdx)> {
    app.session.project.timelines[timeline].tracks[0]
        .clips
        .iter()
        .map(|c| (c.timeline_start, c.timeline_len))
        .collect()
}

#[test]
fn deleting_a_word_cuts_its_clip_in_one_undo_step() {
    let (mut app, timeline, path) = speech_app();
    app.refresh_transcript();
    let model = app.transcript.model.as_ref().unwrap();
    assert_eq!(model.words.len(), 6);
    assert!(model.missing.is_empty());
    let ehm = model.word_item(1).unwrap();
    app.transcript.selection = BTreeSet::from([ehm]);
    let steps = app.session.history.position();
    app.cut_transcript_selection();
    assert_eq!(app.transcript.error, None);
    assert_eq!(app.session.history.position(), steps + 1);
    // 1.02 s .. 2.38 s go: «ehm» and its pauses minus the 120 ms margin.
    assert_eq!(main_geometry(&app, timeline), vec![(0, 26), (26, 191)]);
    app.refresh_transcript();
    let texts: Vec<_> = app
        .transcript
        .model
        .as_ref()
        .unwrap()
        .words
        .iter()
        .map(|w| w.text.as_str())
        .collect();
    assert_eq!(texts, ["Allora", "oggi", "parliamo", "di", "montaggio."]);
    app.session.history.undo(&mut app.session.project);
    assert_eq!(main_geometry(&app, timeline), vec![(0, 250)]);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_correction_shows_in_the_transcript_and_undoes() {
    let (mut app, _, path) = speech_app();
    app.refresh_transcript();
    app.correct_transcript_word(2, "Oggi");
    app.refresh_transcript();
    let model = app.transcript.model.as_ref().unwrap();
    assert_eq!(model.words[2].text, "Oggi");
    assert_eq!(model.words[2].original, "oggi");
    app.session.history.undo(&mut app.session.project);
    app.refresh_transcript();
    assert_eq!(app.transcript.model.as_ref().unwrap().words[2].text, "oggi");
    std::fs::remove_file(path).unwrap();
}
