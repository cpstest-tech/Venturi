use super::*;
use crate::captions::{CaptionWord, Captions};
use crate::{ClipSource, Keyframed, Rational, TitleParams, Track, TrackKind};

fn word(text: &str, start: f64, end: f64) -> Word {
    Word {
        text: text.into(),
        start,
        end,
        confidence: None,
    }
}

fn flat(duration: f64) -> Energy {
    Energy {
        step: 0.01,
        db: vec![-20.0; (duration * 100.0) as usize],
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn runs_group_consecutive_indices() {
    assert_eq!(runs([5, 1, 2, 3, 9, 2]), vec![1..4, 5..6, 9..10]);
    assert!(runs([]).is_empty());
}

#[test]
fn a_deleted_word_takes_its_pauses_minus_the_margin() {
    let words = [
        word("uno", 0.5, 1.0),
        word("due", 2.0, 2.5),
        word("tre", 4.0, 4.5),
    ];
    let cuts = word_cuts(&words, &runs([1]), 6.0, 0.12, &flat(6.0));
    assert_eq!(cuts.len(), 1);
    assert!(
        close(cuts[0].start, 1.12) && close(cuts[0].end, 3.88),
        "{cuts:?}"
    );
}

#[test]
fn adjacent_words_are_cut_at_their_shared_edge_without_eating_the_neighbours() {
    let words = [
        word("a", 0.0, 1.0),
        word("b", 1.0, 1.4),
        word("c", 1.4, 2.0),
    ];
    let cuts = word_cuts(&words, &runs([1]), 2.0, 0.12, &flat(2.0));
    assert!(
        close(cuts[0].start, 1.0) && close(cuts[0].end, 1.4),
        "{cuts:?}"
    );
}

#[test]
fn overlapping_words_never_lose_the_kept_one() {
    let words = [
        word("a", 0.0, 1.05),
        word("b", 1.0, 1.5),
        word("c", 1.45, 2.0),
    ];
    let cuts = word_cuts(&words, &runs([1]), 2.0, 0.12, &flat(2.0));
    assert!(
        cuts[0].start >= 1.05 - 1e-9 && cuts[0].end <= 1.45 + 1e-9,
        "{cuts:?}"
    );
}

#[test]
fn the_edges_move_to_the_quietest_point_inside_the_pause() {
    let words = [
        word("a", 0.0, 1.0),
        word("b", 2.0, 2.5),
        word("c", 3.5, 4.0),
    ];
    let mut energy = flat(4.0);
    energy.db[108] = -80.0;
    let cuts = word_cuts(&words, &runs([1]), 4.0, 0.12, &energy);
    assert!(close(cuts[0].start, 1.08), "{cuts:?}");
}

#[test]
fn consecutive_selected_words_make_one_cut() {
    let words = [
        word("a", 0.0, 0.5),
        word("b", 0.6, 1.0),
        word("c", 1.05, 1.5),
        word("d", 1.6, 2.0),
    ];
    let cuts = word_cuts(&words, &runs([2, 1]), 2.0, 0.12, &flat(2.0));
    assert_eq!(cuts.len(), 1, "{cuts:?}");
    assert!(cuts[0].start <= 0.6 && cuts[0].start >= 0.5 && close(cuts[0].end, 1.5));
}

#[test]
fn the_first_and_last_words_keep_only_the_margin_at_the_clip_edges() {
    let words = [word("a", 1.0, 1.5), word("b", 3.0, 3.5)];
    let cuts = word_cuts(&words, &runs([0]), 5.0, 0.12, &flat(5.0));
    assert!(
        close(cuts[0].start, 0.12) && close(cuts[0].end, 2.88),
        "{cuts:?}"
    );
    let cuts = word_cuts(&words, &runs([1]), 5.0, 0.12, &flat(5.0));
    assert!(
        close(cuts[0].start, 1.62) && close(cuts[0].end, 4.88),
        "{cuts:?}"
    );
}

fn project_with_subtitles() -> (Project, TimelineId) {
    let mut project = Project::default();
    let id = project.alloc_clip_id();
    let mut clip = Clip::from_source_range(id, ClipSource::Text, 0, 25, 0, Rational::one());
    let mut title = TitleParams::default();
    title.content = Keyframed::constant("ciao mondo crudele".into());
    clip.effects.title = Some(title);
    let mut track = Track::new(TrackKind::Video);
    track.subtitles = true;
    track.clips.push(clip);
    let timeline = project.timelines.insert(crate::Timeline {
        name: "T".into(),
        fps: Rational::new(25, 1),
        resolution: (1080, 1920),
        tracks: vec![Track::new_main(), track],
        markers: Vec::new(),
        master: Default::default(),
    });
    let w = |text: &str, start_ms, end_ms| CaptionWord {
        text: text.into(),
        start_ms,
        end_ms,
    };
    project.captions.insert(
        timeline,
        Captions {
            words: vec![
                w("ciao", 0, 300),
                w("mondo", 300, 600),
                w("crudele", 600, 900),
            ],
            ..Captions::default()
        },
    );
    (project, timeline)
}

fn edit(text: &str) -> TranscriptEdit {
    TranscriptEdit {
        path: "/a.mp4".into(),
        stream: 0,
        start_ms: 1300,
        original: "mondo".into(),
        text: text.into(),
    }
}

#[test]
fn a_correction_reaches_the_subtitles_and_undoes_whole() {
    let (mut project, timeline) = project_with_subtitles();
    let mut command = CorrectWord::new(timeline, edit("Mondo!"), "mondo".into(), (310, 590));
    command.apply(&mut project);
    assert_eq!(project.transcript_edits, vec![edit("Mondo!")]);
    assert_eq!(project.captions[&timeline].words[1].text, "Mondo!");
    let title = |p: &Project| {
        p.timelines[timeline].tracks[1].clips[0]
            .effects
            .title
            .as_ref()
            .unwrap()
            .content
            .default
            .clone()
    };
    assert_eq!(title(&project), "ciao Mondo! crudele");
    assert_eq!(
        project.timelines[timeline].tracks[1].clips[0].timeline_len,
        25
    );
    command.undo(&mut project);
    assert!(project.transcript_edits.is_empty());
    assert_eq!(project.captions[&timeline].words[1].text, "mondo");
    assert_eq!(title(&project), "ciao mondo crudele");
}

#[test]
fn correcting_back_to_the_original_drops_the_edit() {
    let (mut project, timeline) = project_with_subtitles();
    CorrectWord::new(timeline, edit("monte"), "mondo".into(), (310, 590)).apply(&mut project);
    CorrectWord::new(timeline, edit("mondo"), "monte".into(), (310, 590)).apply(&mut project);
    assert!(project.transcript_edits.is_empty());
    assert_eq!(project.captions[&timeline].words[1].text, "mondo");
    assert_eq!(
        corrected(&[edit("monte")], Path::new("/a.mp4"), 0, 1300, "mondo"),
        Some("monte")
    );
}
