use super::*;
use crate::{History, Timeline};

fn words(spec: &[(&str, u32, u32)]) -> Vec<CaptionWord> {
    spec.iter()
        .map(|&(text, start_ms, end_ms)| CaptionWord {
            text: text.into(),
            start_ms,
            end_ms,
        })
        .collect()
}

fn project() -> (Project, TimelineId) {
    let mut project = Project::default();
    let timeline = project.timelines.insert(Timeline {
        name: "t".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks: vec![Track::new_main()],
        markers: Vec::new(),
        master: Default::default(),
    });
    (project, timeline)
}

fn sample() -> Vec<CaptionWord> {
    words(&[
        ("Ciao", 0, 400),
        ("a", 450, 600),
        ("tutti.", 650, 1200),
        ("Oggi", 2500, 2900),
        ("parliamo", 2950, 3500),
    ])
}

fn texts(project: &Project, timeline: TimelineId) -> Vec<String> {
    let timeline = &project.timelines[timeline];
    let track = &timeline.tracks[subtitle_track(timeline).unwrap()];
    track
        .clips
        .iter()
        .map(|c| c.effects.title.as_ref().unwrap().content.default.clone())
        .collect()
}

fn generate(project: &mut Project, timeline: TimelineId, id: &str) -> SetCaptions {
    let template = SubtitleTemplate::find(id).unwrap();
    let words = sample();
    let clips = build_clips(project, timeline, &words, template);
    let captions = Captions {
        language: "it".into(),
        words,
        template: id.into(),
        linked: true,
    };
    SetCaptions::new(timeline, Some(captions), clips)
}

#[test]
fn lines_break_on_sentence_end_and_pause() {
    let lines = caption_lines(&sample());
    let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["Ciao a tutti.", "Oggi parliamo"]);
}

#[test]
fn long_text_wraps_at_the_character_limit() {
    let many: Vec<_> = (0..20)
        .map(|i| CaptionWord {
            text: "parola".into(),
            start_ms: i * 300,
            end_ms: i * 300 + 250,
        })
        .collect();
    assert!(
        caption_lines(&many)
            .iter()
            .all(|l| l.text.chars().count() <= 32)
    );
}

#[test]
fn a_line_template_makes_one_clip_per_line() {
    let (mut project, timeline) = project();
    let mut history = History::default();
    let command = generate(&mut project, timeline, "line_classic");
    history.do_command(&mut project, Box::new(command));
    assert_eq!(
        texts(&project, timeline),
        ["Ciao a tutti.", "Oggi parliamo"]
    );
}

#[test]
fn word_templates_make_one_clip_per_word() {
    let (mut project, timeline) = project();
    let mut history = History::default();
    let command = generate(&mut project, timeline, "word_pop");
    history.do_command(&mut project, Box::new(command));
    assert_eq!(texts(&project, timeline).len(), 5);
    let command = generate(&mut project, timeline, "word_build");
    history.do_command(&mut project, Box::new(command));
    assert_eq!(
        texts(&project, timeline)[..3],
        ["Ciao", "Ciao a", "Ciao a tutti."]
    );
}

#[test]
fn a_new_template_replaces_the_old_clips_and_keeps_the_words() {
    let (mut project, timeline) = project();
    let mut history = History::default();
    let command = generate(&mut project, timeline, "word_pop");
    history.do_command(&mut project, Box::new(command));
    let command = generate(&mut project, timeline, "line_outline");
    history.do_command(&mut project, Box::new(command));
    assert_eq!(texts(&project, timeline).len(), 2);
    let subtitle_tracks = project.timelines[timeline]
        .tracks
        .iter()
        .filter(|t| t.subtitles)
        .count();
    assert_eq!(subtitle_tracks, 1);
    assert_eq!(project.captions[&timeline].words, sample());
    assert_eq!(project.captions[&timeline].template, "line_outline");
}

#[test]
fn undo_restores_the_previous_clips_and_the_track() {
    let (mut project, timeline) = project();
    let mut history = History::default();
    let command = generate(&mut project, timeline, "word_pop");
    history.do_command(&mut project, Box::new(command));
    let command = generate(&mut project, timeline, "line_classic");
    history.do_command(&mut project, Box::new(command));
    history.undo(&mut project);
    assert_eq!(texts(&project, timeline).len(), 5);
    assert_eq!(project.captions[&timeline].template, "word_pop");
    history.undo(&mut project);
    assert_eq!(project.timelines[timeline].tracks.len(), 1);
    assert!(project.captions.is_empty());
}

#[test]
fn clips_never_overlap_and_have_a_length() {
    let (mut project, timeline) = project();
    let tight = words(&[("a", 0, 10), ("b", 10, 20), ("c", 20, 30)]);
    let template = SubtitleTemplate::find("word_pop").unwrap();
    let clips = build_clips(&mut project, timeline, &tight, template);
    for pair in clips.windows(2) {
        assert!(pair[0].timeline_end() <= pair[1].timeline_start);
    }
    assert!(clips.iter().all(|c| c.timeline_len >= 1));
}

#[test]
fn srt_and_txt_follow_the_lines() {
    let srt = to_srt(&sample());
    assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:01,200\nCiao a tutti.\n\n2\n"));
    assert_eq!(to_txt(&sample()), "Ciao a tutti. Oggi parliamo\n");
}

#[test]
fn captions_survive_a_save_and_old_files_load_without_them() {
    let (mut project, timeline) = project();
    let mut history = History::default();
    let command = generate(&mut project, timeline, "line_classic");
    history.do_command(&mut project, Box::new(command));
    let text = crate::persistence::project_to_string(&project).unwrap();
    let loaded = crate::persistence::project_from_str(&text).unwrap();
    assert_eq!(loaded.captions, project.captions);
    assert!(
        loaded.timelines[timeline]
            .tracks
            .iter()
            .any(|t| t.subtitles)
    );
    let empty = crate::persistence::project_to_string(&Project::default()).unwrap();
    assert!(
        crate::persistence::project_from_str(&empty)
            .unwrap()
            .captions
            .is_empty()
    );
}

#[test]
fn portrait_lines_are_shorter_than_landscape_ones() {
    let spoken: Vec<_> = (0..12)
        .map(|i| {
            (
                ["Ciao", "a", "tutti", "benvenuti"][i % 4],
                i as u32 * 300,
                i as u32 * 300 + 250,
            )
        })
        .collect();
    let spoken = words(&spoken);
    let template = SubtitleTemplate::find("line_classic").unwrap();
    let longest = |resolution: (u32, u32)| {
        let (mut project, timeline) = project();
        project.timelines.get_mut(timeline).unwrap().resolution = resolution;
        build_clips(&mut project, timeline, &spoken, template)
            .iter()
            .filter_map(|c| c.effects.title.as_ref())
            .map(|t| t.content.default.chars().count())
            .max()
            .unwrap()
    };
    assert!(longest((1080, 1920)) <= 14);
    assert!(longest((1080, 1920)) < longest((1920, 1080)));
}

#[test]
fn a_long_word_shrinks_to_fit_the_frame() {
    let spoken = words(&[("piccolissimamente", 0, 600)]);
    let template = SubtitleTemplate::find("word_pop").unwrap();
    let (mut project, timeline) = project();
    project.timelines.get_mut(timeline).unwrap().resolution = (1080, 1920);
    let clips = build_clips(&mut project, timeline, &spoken, template);
    let size = clips[0].effects.title.as_ref().unwrap().size;
    assert!(17.0 * size * 0.6 <= 1080.0 * 0.88 + 1.0, "size {size}");
}

fn enter_of(
    project: &Project,
    timeline: TimelineId,
    n: usize,
) -> Option<crate::text_anim::AnimDescriptor> {
    let track = subtitle_track(&project.timelines[timeline]).unwrap();
    project.timelines[timeline].tracks[track].clips[n]
        .effects
        .title
        .as_ref()
        .unwrap()
        .anim
        .enter
        .clone()
}

#[test]
fn templates_animate_each_clip_with_their_preset() {
    for (id, preset) in [
        ("word_pop", "sub_pop"),
        ("word_slam", "sub_slam"),
        ("word_drop", "sub_drop"),
        ("line_typewriter", "sub_typewriter"),
        ("word_yellow", "sub_rise"),
        ("line_classic", "sub_fade"),
    ] {
        let (mut project, timeline) = project();
        let command = generate(&mut project, timeline, id);
        History::default().do_command(&mut project, Box::new(command));
        assert_eq!(enter_of(&project, timeline, 0).unwrap().id, preset, "{id}");
    }
}

#[test]
fn pop_overshoots_before_settling() {
    let pop = crate::text_anim_presets::subtitle_in("sub_pop").unwrap();
    let scale = pop
        .tracks
        .iter()
        .find(|t| t.prop == crate::text_anim::AnimProp::Scale)
        .unwrap();
    let at = |p: f32| scale.from + (scale.to - scale.from) * pop.easing.ease(p);
    assert!(at(0.0) < 0.3);
    assert!(at(0.7) > 1.02, "no overshoot: {}", at(0.7));
    assert!((at(1.0) - 1.0).abs() < 1e-4);
}

#[test]
fn the_in_of_a_short_clip_fits_inside_it() {
    let (mut project, timeline) = project();
    let words = words(&[("Hi", 0, 100), ("there", 120, 300)]);
    let template = SubtitleTemplate::find("word_drop").unwrap();
    let clips = build_clips(&mut project, timeline, &words, template);
    for clip in &clips {
        let secs = (clip.timeline_end() - clip.timeline_start) as f32 / 25.0;
        let enter = clip
            .effects
            .title
            .as_ref()
            .unwrap()
            .anim
            .enter
            .as_ref()
            .unwrap();
        assert!(
            enter.duration <= secs * 0.6 + 1e-4,
            "{} > {secs}",
            enter.duration
        );
    }
}

#[test]
fn linking_the_subtitles_is_undoable_and_defaults_on() {
    let (mut project, timeline) = project();
    let mut history = History::default();
    let generate = generate(&mut project, timeline, "word_pop");
    history.do_command(&mut project, Box::new(generate));
    assert!(subtitles_linked(&project, timeline));
    history.do_command(
        &mut project,
        Box::new(SetSubtitlesLinked::new(timeline, false)),
    );
    assert!(!subtitles_linked(&project, timeline));
    history.undo(&mut project);
    assert!(subtitles_linked(&project, timeline));
}

#[test]
fn captions_saved_before_linking_load_as_linked() {
    let old = r#"{"language":"it","words":[],"template":"line_classic"}"#;
    let captions: Captions = serde_json::from_str(old).unwrap();
    assert!(captions.linked);
}

#[test]
fn every_template_animates_its_subtitles() {
    for template in TEMPLATES.iter() {
        let title = template.title("Hi", (1920, 1080));
        assert!(title.anim.enter.is_some(), "{} is static", template.id);
    }
}

#[test]
fn a_preview_frame_animates_like_the_clips_of_its_template() {
    let pop = SubtitleTemplate::find("word_pop").unwrap();
    let start = pop.preview_frame("Ciao a tutti", (1080, 1920), PREVIEW_WORD_SECS + 0.01);
    assert_eq!(start.content.default, "a");
    let state = start.anim_state.expect("the word is popping in");
    assert!(state.t_in < 0.05);
    let line = SubtitleTemplate::find("line_classic").unwrap();
    let frame = line.preview_frame("Ciao a tutti", (1920, 1080), 0.05);
    assert_eq!(frame.content.default, "Ciao a tutti");
    assert!(frame.anim_state.is_some());
    let build = SubtitleTemplate::find("word_build").unwrap();
    let frame = build.preview_frame("Ciao a tutti", (1920, 1080), 2.0 * PREVIEW_WORD_SECS + 0.1);
    assert_eq!(frame.content.default, "Ciao a tutti");
    assert!(
        pop.preview_frame("", (1920, 1080), 0.3)
            .content
            .default
            .is_empty()
    );
}
