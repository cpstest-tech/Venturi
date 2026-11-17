use super::*;

fn app() -> VenturiApp {
    let mut app = VenturiApp::default();
    let clip = vv_core::Clip::from_source_range(
        vv_core::ClipId(1),
        vv_core::ClipSource::SolidColor,
        0,
        300,
        0,
        vv_core::Rational::one(),
    );
    let mut track = vv_core::Track::new(vv_core::TrackKind::Video);
    track.clips.push(clip);
    let timeline = app.session.project.timelines.insert(vv_core::Timeline {
        name: "Context".into(),
        fps: vv_core::Rational::new(30, 1),
        resolution: (1920, 1080),
        tracks: vec![track],
        markers: Vec::new(),
        master: Default::default(),
    });
    app.timeline_id = Some(timeline);
    app.session.project.captions.insert(
        timeline,
        vv_core::Captions {
            words: vec![
                vv_core::CaptionWord {
                    start_ms: 500,
                    end_ms: 1000,
                    text: "Before".into(),
                },
                vv_core::CaptionWord {
                    start_ms: 2500,
                    end_ms: 4500,
                    text: "CPU".into(),
                },
            ],
            ..Default::default()
        },
    );
    app
}

#[test]
fn timeline_in_out_range_works_without_selected_clips_and_crops_caption_times() {
    let mut app = app();
    app.ai_sequences.use_marks = true;
    app.ai_sequences.length = Length::Clip;
    app.ai_sequences.style = "Minimal purple diagrams".into();
    app.timeline_state.export_marks.mark_in = Some(60);
    app.timeline_state.export_marks.mark_out = Some(120);
    let request = app.ai_sequence_request().unwrap();
    assert_eq!(request.start, 60);
    assert_eq!(request.composition.frames, 60);
    assert_eq!(request.style, "Minimal purple diagrams");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&request.context).unwrap(),
        serde_json::json!([{"start_ms":500,"end_ms":2000,"text":"CPU"}])
    );
    assert_eq!(request.anchor_clip, Some(1));
}

#[test]
fn prompt_only_does_not_send_project_captions_or_media() {
    let mut app = app();
    app.timeline_state.playhead = 75;
    let request = app.ai_sequence_request().unwrap();
    assert_eq!(request.context, "[]");
    assert!(request.media.is_empty());
}

#[test]
fn ai_interface_strings_are_available_in_every_locale() {
    for key in [
        "ai_sequence.title",
        "ai_sequence.intro",
        "ai_sequence.prompt_hint",
        "ai_sequence.marks",
        "ai_sequence.transparent",
        "ai_sequence.style",
        "ai_sequence.runtime_directory",
        "ai_sequence.node",
        "ai_sequence.browser",
        "ai_sequence.new_composition",
        "ai_sequence.generate",
        "ai_sequence.save_first",
        "ai_sequence.jobs",
        "ai_sequence.cancelling",
        "ai_sequence.cancel",
        "ai_sequence.error_details",
        "ai_sequence.provider_events",
        "ai_sequence.usage_requests",
        "ai_sequence.usage",
        "ai_sequence.import",
        "ai_sequence.revise",
        "ai_sequence.retry",
        "ai_sequence.revision_hint",
        "ai_sequence.generate_revision",
        "ai_sequence.fresh_session",
        "ai_sequence.revision_provider",
        "ai_sequence.preview",
        "ai_sequence.apply_version",
        "ai_sequence.suggestions",
        "ai_sequence.suggestions_hint",
        "ai_sequence.analysis_model",
        "ai_sequence.analyzing",
        "ai_sequence.cancel_analysis",
        "ai_sequence.refresh_analysis",
        "ai_sequence.analyze",
        "ai_sequence.analysis_usage",
        "ai_sequence.generate_selected",
        "ai_sequence.discard",
        "ai_sequence.other_projects",
        "ai_sequence.open_project",
        "ai_sequence.detect",
        "ai_sequence.default_provider",
        "ai_sequence.settings_model",
        "ai_sequence.list_models",
        "ai_sequence.listing_hint",
        "ai_sequence.hydrogen_hint",
        "ai_sequence.pricing_tiers",
        "ai_sequence.quotas",
        "ai_sequence.attempts",
        "ai_sequence.provider_timeout",
        "ai_sequence.render_timeout",
        "ai_sequence.runtime",
        "ai_sequence.free_only",
        "ai_sequence.confirm_unbilled",
        "ai_sequence.billing_limit",
        "ai_sequence.allow_billing",
        "ai_sequence.access_disabled",
        "ai_sequence.free_refused",
        "ai_sequence.key_changed",
        "ai_sequence.active_tasks",
        "ai_sequence.tab",
        "ai_sequence.context_clip",
        "ai_sequence.context_no_timeline",
        "ai_sequence.context_title",
        "ai_sequence.length_auto",
        "ai_sequence.length_clip",
        "ai_sequence.length_custom",
        "ai_sequence.placeholder",
        "ai_sequence.placeholder_failed",
        "ai_sequence.dismiss",
        "ai_sequence.current_revision",
        "ai_sequence.revising",
        "ai_sequence.stage_queued",
        "ai_sequence.stage_preparing",
        "ai_sequence.stage_analyzing",
        "ai_sequence.stage_generating",
        "ai_sequence.stage_validating",
        "ai_sequence.stage_rendering",
        "ai_sequence.stage_completed",
        "ai_sequence.stage_failed",
        "ai_sequence.stage_cancelled",
        "ai_sequence.prompt_example",
        "ai_sequence.section_what",
        "ai_sequence.use_transcript",
        "ai_sequence.section_style",
        "ai_sequence.look_simple",
        "ai_sequence.look_simple_hint",
        "ai_sequence.look_modern",
        "ai_sequence.look_modern_hint",
        "ai_sequence.look_vivid",
        "ai_sequence.look_vivid_hint",
        "ai_sequence.section_where",
        "ai_sequence.placement_pill_hint",
        "ai_sequence.placement_above_head_hint",
        "ai_sequence.tab_generate",
        "ai_sequence.tab_suggestions",
        "ai_sequence.history",
        "ai_sequence.setup_title",
        "ai_sequence.setup_node",
        "ai_sequence.setup_packages",
        "ai_sequence.setup_browser",
        "ai_sequence.setup_isolation",
        "ai_sequence.setup_waiting",
        "ai_sequence.setup_failed",
        "ai_sequence.setup_retry",
        "ai_sequence.setup_done",
        "ai_sequence.setup_queued",
        "ai_sequence.suggestions_intro",
        "ai_sequence.scope_title",
        "ai_sequence.scope_selection",
        "ai_sequence.scope_timeline",
        "ai_sequence.scope_select_clip",
        "ai_sequence.focus_title",
        "ai_sequence.focus_hint",
        "ai_sequence.analysis_with",
        "ai_sequence.ideas_found",
        "ai_sequence.select_all",
        "ai_sequence.select_none",
        "ai_sequence.jump_to",
        "ai_sequence.uses_settings",
        "ai_sequence.change",
        "ai_sequence.generate_count",
        "ai_sequence.look_custom",
        "ai_sequence.use_idea",
        "ai_sequence.where_side",
        "ai_sequence.where_side_hint",
        "ai_sequence.where_full",
        "ai_sequence.where_full_hint",
        "ai_sequence.placement_side",
        "ai_sequence.placement_side_hint",
        "ai_sequence.generate_infographic",
        "ai_sequence.advanced",
        "ai_sequence.placeholder_done",
        "ai_sequence.placeholder_adding",
        "ai_sequence.placeholder_retry",
        "ai_sequence.retry_short",
        "ai_sequence.edit_title",
        "ai_sequence.edit_subtitle",
        "ai_sequence.apply_change",
        "ai_sequence.or_try",
        "ai_sequence.suggestion_hover",
        "ai_sequence.editing",
        "ai_sequence.edit_failed",
        "ai_sequence.edit_details",
    ] {
        for locale in rust_i18n::available_locales!() {
            assert!(
                crate::_rust_i18n_try_translate(&locale, key).is_some(),
                "{locale}: {key}"
            );
        }
    }
}

#[test]
fn failed_ai_imports_clear_pending_references_and_keep_a_retryable_result() {
    let mut app = app();
    app.ai_sequences.prompt = "Synthetic import failure test".into();
    let root = vv_media::test_support::TempDir::new("vv-ai-import-failure");
    let project = root.join("project.tandem");
    app.session.set_path(Some(project.clone()));
    let store = crate::ai_sequence::Store::new(&root.join("sources")).unwrap();
    let job = store.create(app.ai_sequence_request().unwrap()).unwrap();
    app.ai_sequences
        .pending_imports
        .insert(job.id.clone(), (project, job.clone()));
    app.ai_sequences
        .import_jobs
        .insert((app.session.epoch(), 42), job.id.clone());
    app.ai_import_finished(42, &["Disk full during import".into()]);
    assert!(app.ai_sequences.pending_imports.is_empty());
    assert!(app.ai_sequences.import_jobs.is_empty());
    assert!(
        app.ai_sequences
            .error
            .as_deref()
            .unwrap()
            .contains("Disk full")
    );
    assert!(store.load(&job.id).is_ok());
}

#[test]
fn background_ai_import_completion_preserves_the_viewer_and_user_selection() {
    let mut app = app();
    app.timeline_state.selected.insert((0, vv_core::ClipId(1)));
    app.ai_sequences
        .import_jobs
        .insert((app.session.epoch(), 7), "sequence".into());
    app.import_warnings = vec!["Existing unrelated warning".into()];
    app.handle_session_event(vv_session::SessionEvent::ImportStarted { job: 7 });
    assert_eq!(app.import_warnings, vec!["Existing unrelated warning"]);
    app.handle_session_event(vv_session::SessionEvent::ImportFinished {
        job: 7,
        imported: Vec::new(),
        errors: Vec::new(),
    });
    assert!(app.browsing.is_none());
    assert!(
        app.timeline_state
            .selected
            .contains(&(0, vv_core::ClipId(1)))
    );
    assert!(app.ai_sequences.import_jobs.is_empty());
}

#[test]
fn length_modes_set_the_composition_and_let_the_ai_decide_by_default() {
    let mut app = app();
    app.timeline_state.playhead = 30;
    let request = app.ai_sequence_request().unwrap();
    assert!(request.auto_duration);
    assert_eq!(request.composition.frames, 900);

    app.ai_sequences.length = Length::Clip;
    assert_eq!(app.ai_sequence_request().unwrap().composition.frames, 150);

    app.ai_sequences.length = Length::Custom;
    app.ai_sequences.custom_seconds = 7.0;
    let request = app.ai_sequence_request().unwrap();
    assert!(!request.auto_duration);
    assert_eq!(request.composition.frames, 210);
}

#[test]
fn selected_clip_is_the_context_and_no_selection_is_a_free_animation() {
    let mut app = app();
    app.ai_sequences.length = Length::Clip;
    assert!(app.ai_range().unwrap().is_none());
    app.timeline_state.selected.insert((0, vv_core::ClipId(1)));
    let range = app.ai_range().unwrap().unwrap();
    assert_eq!((range.start, range.end, range.clips), (0, 300, 1));
    let request = app.ai_sequence_request().unwrap();
    assert_eq!(request.composition.frames, 300);
    assert_eq!(request.anchor_clip, Some(1));
}

#[test]
fn running_generations_reserve_their_place_and_failures_are_dismissable() {
    let mut app = app();
    app.ai_sequences.prompt = "Placeholder test".into();
    app.ai_sequences.length = Length::Custom;
    app.ai_sequences.custom_seconds = 2.0;
    app.timeline_state.playhead = 45;
    let root = vv_media::test_support::TempDir::new("vv-ai-placeholder");
    let project = root.join("project.tandem");
    app.session.set_path(Some(project.clone()));
    let store = crate::ai_sequence::Store::new(&root.join("sources")).unwrap();
    let mut job = store.create(app.ai_sequence_request().unwrap()).unwrap();
    job.stage = Stage::Generating;
    app.ai_sequences
        .queues
        .insert(project.clone(), Queue::open(&project).unwrap());
    assert!(app.ai_placeholders().is_empty());
    app.ai_sequences
        .queues
        .get_mut(&project)
        .unwrap()
        .jobs
        .push(job.clone());

    let placeholders = app.ai_placeholders();
    assert_eq!(placeholders.len(), 1);
    assert_eq!((placeholders[0].start, placeholders[0].len), (45, 60));
    assert!(!placeholders[0].failed);

    let queue = app.ai_sequences.queues.get_mut(&project).unwrap();
    queue.jobs[0].stage = Stage::Failed;
    assert!(app.ai_placeholders()[0].failed);

    app.timeline_state.ai_placeholder_action =
        Some(timeline_ui::AiPlaceholderAction::Dismiss(job.id.clone()));
    app.apply_ai_placeholder_action();
    assert!(app.ai_placeholders().is_empty());
}

#[test]
fn failures_from_earlier_sessions_get_no_placeholder() {
    let mut app = app();
    app.ai_sequences.prompt = "Old failure".into();
    let root = vv_media::test_support::TempDir::new("vv-ai-old-failure");
    let project = root.join("project.tandem");
    app.session.set_path(Some(project.clone()));
    let store = crate::ai_sequence::Store::new(&root.join("sources")).unwrap();
    let mut job = store.create(app.ai_sequence_request().unwrap()).unwrap();
    job.stage = Stage::Failed;
    let mut queue = Queue::open(&project).unwrap();
    queue.jobs.push(job);
    app.ai_sequences.queues.insert(project, queue);
    assert!(app.ai_placeholders().is_empty());
}

#[test]
fn the_tab_renders_within_a_narrow_library_panel() {
    let mut app = app();
    app.timeline_state.selected.insert((0, vv_core::ClipId(1)));
    app.ai_sequences.prompt = "Animated bar chart".into();
    let ctx = egui::Context::default();
    for width in [240.0_f32, 360.0, 800.0] {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 900.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| app.show_ai_sequence_tab(ui));
        output.textures_delta.clear();
        assert!(
            ctx.globally_used_rect().width() <= width + 1.0,
            "tab overflows at {width}: {}",
            ctx.globally_used_rect().width()
        );
    }
}

#[test]
fn auto_length_placeholder_stays_short_until_the_model_decides() {
    let mut app = app();
    app.ai_sequences.prompt = "Auto length".into();
    let root = vv_media::test_support::TempDir::new("vv-ai-auto-placeholder");
    let project = root.join("project.tandem");
    app.session.set_path(Some(project.clone()));
    let store = crate::ai_sequence::Store::new(&root.join("sources")).unwrap();
    let mut job = store.create(app.ai_sequence_request().unwrap()).unwrap();
    assert_eq!(job.request.composition.frames, 900);
    job.stage = Stage::Generating;
    app.ai_sequences
        .queues
        .insert(project.clone(), Queue::open(&project).unwrap());
    app.ai_placeholders();
    app.ai_sequences
        .queues
        .get_mut(&project)
        .unwrap()
        .jobs
        .push(job);
    assert_eq!(app.ai_placeholders()[0].len, 240);
}

#[test]
fn the_transcript_can_be_left_out_of_the_request() {
    let mut app = app();
    app.timeline_state.selected.insert((0, vv_core::ClipId(1)));
    assert_ne!(app.ai_sequence_request().unwrap().context, "[]");
    app.ai_sequences.no_transcript = true;
    assert_eq!(app.ai_sequence_request().unwrap().context, "[]");
}

#[test]
fn the_modern_look_is_the_default_style() {
    let mut app = app();
    assert!(
        app.ai_sequence_request()
            .unwrap()
            .style
            .starts_with("Modern")
    );
    app.ai_sequences.style = LOOKS[2].prompt.into();
    assert!(
        app.ai_sequence_request()
            .unwrap()
            .style
            .starts_with("Vivid")
    );
}

#[test]
fn suggestion_cards_fit_narrow_panels() {
    let idea = crate::ai_sequence::Improvement {
        id: "bigger-title".into(),
        title: "Make the title bigger".into(),
        description: "Enlarge the title and raise its contrast so it reads from far away.".into(),
        instruction: "Enlarge the title".into(),
    };
    let ctx = egui::Context::default();
    for width in [200.0_f32, 320.0, 800.0] {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 600.0),
            )),
            ..Default::default()
        };
        let mut clicked = false;
        let mut output = ctx.run_ui(raw, |ui| {
            clicked = crate::ai_sequence_cards::improvement_card(ui, &idea, true).clicked();
        });
        output.textures_delta.clear();
        assert!(!clicked);
        assert!(
            ctx.globally_used_rect().width() <= width + 1.0,
            "card overflows at {width}"
        );
    }
}

#[test]
fn every_style_and_placement_card_is_translated_and_distinct() {
    let prompts: std::collections::HashSet<_> = LOOKS.iter().map(|look| look.prompt).collect();
    assert_eq!(prompts.len(), 10);
    let keys = LOOKS.iter().flat_map(|look| [look.label, look.hint]).chain(
        PLACEMENTS
            .iter()
            .flat_map(|(placement, hint, _)| [placement_key(*placement), *hint]),
    );
    for key in keys {
        for locale in rust_i18n::available_locales!() {
            assert!(
                crate::_rust_i18n_try_translate(&locale, key).is_some(),
                "{locale}: {key}"
            );
        }
    }
}

#[test]
fn whole_timeline_analysis_covers_every_frame_without_an_anchor() {
    let mut app = app();
    app.ai_sequences.scope = Scope::Timeline;
    let range = app.ai_analysis_range().unwrap().unwrap();
    assert_eq!((range.start, range.end, range.anchor), (0, 300, None));
    let request = app
        .request_for(Some(range), Length::Clip, "numbers")
        .unwrap();
    assert_eq!(request.composition.frames, 300);
    assert_eq!(request.prompt, "numbers");
    assert_eq!(request.anchor_clip, None);
}

#[test]
fn selection_analysis_needs_a_selected_clip() {
    let app = app();
    assert!(app.ai_analysis_range().unwrap().is_none());
}
