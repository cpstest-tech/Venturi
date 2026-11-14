use super::*;

fn suggestion(start_ms: u32, end_ms: u32, score: u8) -> Suggestion {
    Suggestion {
        start_ms,
        end_ms,
        title: "Moment".into(),
        description: String::new(),
        score,
        reason: String::new(),
        selected: false,
    }
}

#[test]
fn refine_rejects_invalid_times_and_overlaps() {
    let words = (0..80)
        .map(|i| CaptionWord {
            text: if i % 10 == 9 { "end." } else { "word" }.into(),
            start_ms: i * 1_000,
            end_ms: i * 1_000 + 700,
        })
        .collect::<Vec<_>>();
    let proposed = vec![
        suggestion(1_100, 21_600, 92),
        suggestion(2_000, 22_000, 80),
        suggestion(35_100, 55_600, 86),
        suggestion(78_000, 99_000, 99),
        suggestion(1_000, 1_500, 100),
    ];
    let result = refine(proposed, &words, 80_000, 0);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].score, 92);
    assert_eq!(result[1].score, 86);
    assert_eq!(result[0].start_ms, 1_000);
    assert_eq!(result[0].end_ms, 21_700);
}

#[test]
fn cache_selection_key_keeps_prompt_and_duration_distinct() {
    let selections = [
        Selection {
            prompt: "funny".into(),
            duration: 0,
            model: MODEL.into(),
            clips: vec![],
            verified: true,
        },
        Selection {
            prompt: "technology".into(),
            duration: 0,
            model: MODEL.into(),
            clips: vec![],
            verified: true,
        },
    ];
    let find = |prompt: &str, duration| {
        selections
            .iter()
            .position(|s| s.prompt == prompt && s.duration == duration && s.model == MODEL)
    };
    assert_eq!(find("funny", 0), Some(0));
    assert_eq!(find("technology", 0), Some(1));
    assert_eq!(find("funny", 1), None);
}

#[test]
fn gemini_json_requires_every_clip_field() {
    assert!(parse_suggestions(r#"{"clips":[{"start_ms":1000}]}"#).is_err());
    assert!(parse_suggestions(r#"{"clips":[{"start_ms":1000,"end_ms":22000,"title":"A","description":"B","score":300,"reason":"C"}]}"#).is_err());
    let parsed = parse_suggestions(r#"{"clips":[{"start_ms":1000,"end_ms":22000,"title":"A","description":"B","score":90,"reason":"C"}]}"#).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].score, 90);
}

#[test]
fn cached_transcript_and_selection_survive_serialization() {
    let cache = Cache {
        fingerprint: 1234,
        duration_ms: 90_000,
        language: "it".into(),
        words: vec![CaptionWord {
            text: "Ciao".into(),
            start_ms: 1_000,
            end_ms: 1_400,
        }],
        segments: vec![],
        selections: vec![Selection {
            prompt: "technology".into(),
            duration: 1,
            model: MODEL.into(),
            clips: vec![suggestion(1_000, 31_000, 87)],
            verified: true,
        }],
        frames: vec![CachedFrame {
            at_ms: 16_000,
            png_base64: "abc".into(),
        }],
    };
    let restored: Cache = serde_json::from_slice(&serde_json::to_vec(&cache).unwrap()).unwrap();
    assert_eq!(restored.words, cache.words);
    assert_eq!(restored.selections[0].clips[0].score, 87);
    assert!(restored.selections[0].verified);
    assert_eq!(restored.frames[0].at_ms, 16_000);
}

#[test]
fn transcript_segments_break_at_sentence_and_pause() {
    let words = vec![
        CaptionWord {
            text: "Hello".into(),
            start_ms: 0,
            end_ms: 300,
        },
        CaptionWord {
            text: "world.".into(),
            start_ms: 310,
            end_ms: 600,
        },
        CaptionWord {
            text: "Next".into(),
            start_ms: 700,
            end_ms: 950,
        },
        CaptionWord {
            text: "thought".into(),
            start_ms: 2_000,
            end_ms: 2_400,
        },
    ];
    let segments = segment_words(&words);
    assert_eq!(segments.len(), 3);
    assert_eq!((segments[0].start_ms, segments[0].end_ms), (0, 600));
    assert_eq!(segments[2].text, "thought");
}

#[test]
fn source_fingerprint_changes_with_same_size_content() {
    let path = std::env::temp_dir().join(format!("vv-ai-clips-hash-{}", std::process::id()));
    std::fs::write(&path, b"abcd").unwrap();
    let first = source_fingerprint(&path).unwrap();
    std::fs::write(&path, b"abce").unwrap();
    let second = source_fingerprint(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_ne!(first, second);
}

#[test]
fn local_gemini_key_round_trips_without_entering_project_cache() {
    let dir = std::env::temp_dir().join(format!("vv-ai-clips-key-{}", std::process::id()));
    let path = dir.join("gemini.key");
    save_key_at(&path, "  test-secret  ").unwrap();
    assert_eq!(load_key_at(&path).unwrap(), "test-secret");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn ai_clips_tab_selects_project_video_without_media_pool_selection() {
    let path = std::env::temp_dir().join(format!("vv-ai-clips-selector-{}", std::process::id()));
    std::fs::write(&path, b"video marker").unwrap();
    let mut app = VenturiApp::default();
    app.ai_clips.key_present = Some(true);
    let media = app.session.project.media_pool.insert(vv_core::MediaItem {
        path: path.clone(),
        meta: vv_core::MediaMeta {
            duration_frames: 2_500,
            fps: Rational::new(25, 1),
            width: 1920,
            height: 1080,
            has_video: true,
            has_audio: true,
            sample_rate: 48_000,
            channels: 2,
            audio_streams: 1,
            file: Default::default(),
        },
        content_hash: 0,
        compound: None,
        folder: None,
    });
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(700.0, 500.0),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.show_ai_clips_tab(ui));
    assert_eq!(app.ai_clips.source_choice, Some(media));
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text().contains("vv-ai-clips-selector"))));
    output.textures_delta.clear();
    let second_item = app.session.project.media_pool.get(media).unwrap().clone();
    let second = app.session.project.media_pool.insert(second_item);
    app.ai_clips.source = Some(media);
    app.ai_clips.source_choice = Some(second);
    app.ai_clips.results = vec![suggestion(10_000, 30_000, 90)];
    app.sync_ai_clips_source(&ctx);
    assert!(app.ai_clips.results.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn subtitle_words_follow_inserted_clip_position() {
    let words = [
        CaptionWord {
            text: "Before".into(),
            start_ms: 9_000,
            end_ms: 9_400,
        },
        CaptionWord {
            text: "Selected".into(),
            start_ms: 12_000,
            end_ms: 12_500,
        },
        CaptionWord {
            text: "After".into(),
            start_ms: 31_000,
            end_ms: 31_500,
        },
    ];
    let shifted = words_at_timeline_position(&words, 10_000, 30_000, 1_000, Rational::new(25, 1));
    assert_eq!(
        shifted,
        vec![CaptionWord {
            text: "Selected".into(),
            start_ms: 42_000,
            end_ms: 42_500
        }]
    );
}

#[test]
fn insertion_and_replacement_preserve_source_ranges_and_undo() {
    let path = std::env::temp_dir().join(format!("vv-ai-clips-insert-{}", std::process::id()));
    std::fs::write(&path, b"source marker").unwrap();
    let fingerprint = source_fingerprint(&path).unwrap();
    let mut app = VenturiApp::default();
    let timeline_id = app.session.project.timelines.insert(vv_core::Timeline {
        name: "AI Clips test".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks: vec![
            vv_core::Track::new(TrackKind::Video),
            vv_core::Track::new(TrackKind::Audio),
        ],
        markers: Vec::new(),
        master: Default::default(),
    });
    let media = app.session.project.media_pool.insert(vv_core::MediaItem {
        path: path.clone(),
        meta: vv_core::MediaMeta {
            duration_frames: 2_500,
            fps: Rational::new(25, 1),
            width: 1920,
            height: 1080,
            has_video: true,
            has_audio: true,
            sample_rate: 48_000,
            channels: 2,
            audio_streams: 1,
            file: Default::default(),
        },
        content_hash: fingerprint,
        compound: None,
        folder: None,
    });
    app.timeline_id = Some(timeline_id);
    app.ai_clips.source = Some(media);
    app.ai_clips.source_fingerprint = Some(fingerprint);
    app.ai_clips.results = vec![
        suggestion(10_000, 30_000, 90),
        suggestion(40_000, 60_000, 85),
    ];
    for clip in &mut app.ai_clips.results {
        clip.selected = true;
    }
    app.insert_ai_clips(false);
    let timeline = &app.session.project.timelines[timeline_id];
    assert_eq!(timeline.tracks[0].clips.len(), 2);
    assert_eq!(timeline.tracks[1].clips.len(), 2);
    assert_eq!(
        (
            timeline.tracks[0].clips[0].source_offset,
            timeline.tracks[0].clips[0].timeline_len
        ),
        (250, 500)
    );
    assert_eq!(timeline.tracks[0].clips[1].timeline_start, 500);
    assert_eq!(
        timeline.tracks[0].clips[0].linked_group,
        timeline.tracks[1].clips[0].linked_group
    );
    app.ai_clips.results = vec![suggestion(70_000, 90_000, 95)];
    app.ai_clips.results[0].selected = true;
    app.insert_ai_clips(true);
    let timeline = &app.session.project.timelines[timeline_id];
    assert_eq!(timeline.tracks[0].clips.len(), 1);
    assert_eq!(timeline.tracks[0].clips[0].source_offset, 1_750);
    app.session.history.undo(&mut app.session.project);
    assert_eq!(
        app.session.project.timelines[timeline_id].tracks[0]
            .clips
            .len(),
        2
    );
    app.session.history.undo(&mut app.session.project);
    assert!(
        app.session.project.timelines[timeline_id].tracks[0]
            .clips
            .is_empty()
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn gemini_errors_show_the_api_message_and_retry_only_transient_statuses() {
    let body = br#"{"error":{"code":503,"message":"The model is overloaded. Please try again later.","status":"UNAVAILABLE"}}"#;
    assert_eq!(
        api_error(503, body),
        "Gemini error 503: The model is overloaded. Please try again later."
    );
    assert_eq!(api_error(500, b"not json"), "Gemini error 500");
    assert!(is_transient(503) && is_transient(429));
    assert!(!is_transient(400) && !is_transient(403));
}
