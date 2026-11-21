use super::translate::{Item, Provider, Translator};
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};
use vv_core::{Id, Timeline, Track};
use vv_media::audio_file::{AudioFileFormat, write_audio_file};

fn temp_dir() -> PathBuf {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    std::env::temp_dir().join(format!("tandem-dub-{}-{now:?}", std::process::id()))
}

fn wav(path: &Path, samples: usize) {
    write_audio_file(path, AudioFileFormat::Wav, &vec![0.1; samples], 22_050, 1).unwrap();
}

fn row(path: &Path, text: &str, start: f64) -> Row {
    Row {
        start,
        end: start + 1.0,
        window: start..start + 1.2,
        source: "Ciao, iPad.".into(),
        text: text.into(),
        translated: true,
        audio: Some(Audio {
            path: path.to_owned(),
            secs: audio_secs(path).unwrap(),
            key: audio_key(&Prefs::default().voice, text),
        }),
        error: None,
        busy: false,
    }
}

#[test]
fn dubbing_requires_cloud_opt_in_and_manual_translation_works_offline() {
    let mut app = VenturiApp::default();
    app.settings.shorts.dub.translator = Provider::Gemini;
    assert!(app.translator_blocked().is_some());
    app.settings.shorts.dub.translator = Provider::Manual;
    assert!(app.translator_blocked().is_none());

    let items = [Item {
        id: 2,
        text: "iPad".into(),
        max_chars: 10,
    }];
    let manual = translate::Manual;
    assert_eq!(
        manual
            .translate(&items, "en", &[], false, &AtomicBool::new(false))
            .unwrap(),
        [(2, "iPad".into())]
    );
    assert_eq!(
        translate::parse_reply(r#"{"translations":[{"id":2,"text":"  "}]}"#, &items),
        Err(translate::Error::Empty(vec![2]))
    );
}

#[test]
fn audio_is_current_only_for_the_reviewed_text_and_voice() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("audio.wav");
    wav(&file, 22_050);
    let mut row = row(&file, "Hello", 0.0);
    let voice = Prefs::default().voice;
    assert!(row.audio_fresh(&voice));
    assert!(!row.audio_fresh("some other voice"));
    row.text.push('!');
    assert!(!row.audio_fresh(&voice));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn preview_adds_editable_audio_and_text_and_is_one_undo_step() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("voice.wav");
    wav(&file, 22_050);
    let mut app = VenturiApp::default();
    app.session.set_path(Some(dir.join("project.tandem")));
    let fps = Rational::new(30, 1);
    let timeline = app.session.project.timelines.insert(Timeline {
        name: "Dub test".into(),
        fps,
        resolution: (1080, 1920),
        tracks: vec![Track::new_main()],
        markers: vec![],
        master: Default::default(),
    });
    app.timeline_id = Some(timeline);
    let first = row(&file, "Hello, iPad.", 0.0);
    let second = row(&file, "Goodbye.", 2.0);
    let rows = vec![
        (first.clone(), first.fit(0.2).unwrap()),
        (second.clone(), second.fit(0.2).unwrap()),
    ];
    let (wav, offsets) = concatenate(
        &rows,
        "en",
        app.session.path(),
        &AtomicBool::new(false),
        || {},
    )
    .unwrap();
    assert_eq!(offsets, [(0, 30), (30, 60)]);
    let meta = vv_media::probe_media(&wav).unwrap();
    let prepared = Prepared {
        timeline,
        generation: app.session.history.generation(),
        prefs: Prefs::default(),
        rows,
        wav,
        offsets,
        meta,
    };
    app.build_dub(prepared).unwrap();
    assert!(app.dub_preview_pending());
    let tl = &app.session.project.timelines[timeline];
    assert_eq!(dub::languages(tl), ["en"]);
    assert_eq!(tl.tracks[1].clips.len(), 2);
    assert_eq!(tl.tracks[1].clips[1].timeline_start, 60);
    assert_eq!(tl.tracks[2].kind, TrackKind::Video);
    assert!(vv_core::captions::track_to_txt(&tl.tracks[2]).contains("iPad"));
    let mut snapshot = app.session.project.clone();
    dub::apply_language(&mut snapshot, timeline, &Language::Original);
    assert!(snapshot.timelines[timeline].tracks[1].muted);
    assert!(snapshot.timelines[timeline].tracks[2].muted);
    assert!(!tl.tracks[1].muted);
    app.discard_dub();
    assert_eq!(app.session.project.timelines[timeline].tracks.len(), 1);
    app.session.history.redo(&mut app.session.project);
    assert_eq!(app.session.project.timelines[timeline].tracks.len(), 3);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn translation_cache_distinguishes_model_language_and_glossary() {
    let key = |language, glossary: &[String]| {
        translate::cache_key(Provider::Gemini, "model-a", language, glossary, "iPad")
    };
    assert_ne!(key("en", &[]), key("fr", &[]));
    assert_ne!(key("en", &[]), key("en", &["iPad".into()]));
}

#[test]
fn preview_rejects_results_from_another_timeline() {
    let mut app = VenturiApp::default();
    let result = app.build_dub(Prepared {
        timeline: TimelineId::from_raw(1),
        generation: 0,
        prefs: Prefs::default(),
        rows: Vec::new(),
        wav: PathBuf::new(),
        offsets: Vec::new(),
        meta: vv_core::MediaMeta {
            duration_frames: 0,
            fps: Rational::one(),
            width: 0,
            height: 0,
            has_video: false,
            has_audio: false,
            sample_rate: 0,
            channels: 0,
            audio_streams: 0,
            file: Default::default(),
        },
    });
    assert!(result.is_err());
}

#[test]
#[ignore = "requires a local Piper installation with Joe and Paola"]
fn piper_synthesizes_english_and_italian() {
    let models = PathBuf::from(std::env::var_os("TANDEM_TEST_PIPER_MODELS").unwrap());
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    for (id, text) in [
        ("en_US-joe-medium", "A local dubbing test."),
        ("it_IT-paola-medium", "Una prova di doppiaggio locale."),
    ] {
        let voice = tts::voice_for(&models, id).unwrap();
        let path = dir.join(format!("{id}.wav"));
        voice
            .synthesize(text, &path, &AtomicBool::new(false))
            .unwrap();
        assert!(audio_secs(&path).is_some_and(|seconds| seconds > 0.5));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_partial_word_does_not_count_as_a_preserved_glossary_term() {
    let glossary = translate::glossary("Mac, iPad");
    assert_eq!(
        translate::missing_term("Lo uso su Mac.", "I use it with a macro.", &glossary),
        Some("Mac"),
    );
    assert_eq!(
        translate::missing_term("Lo uso su Mac.", "I use it on my Mac.", &glossary),
        None,
    );
}
