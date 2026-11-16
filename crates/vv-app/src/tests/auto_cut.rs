use super::*;
use vv_core::auto_cut::SegmentType;

fn segment(kind: SegmentType, start: f64, end: f64, checked: bool) -> Segment {
    Segment {
        id: 0,
        start,
        end,
        kind,
        text: String::new(),
        checked,
    }
}

#[test]
fn changing_threshold_keeps_manual_choices_for_overlapping_suggestions() {
    let old = [
        segment(SegmentType::Silence, 1.0, 2.0, false),
        segment(SegmentType::Filler, 3.0, 3.4, true),
    ];
    let mut new = [
        segment(SegmentType::Silence, 1.1, 1.9, true),
        segment(SegmentType::Filler, 3.0, 3.4, false),
        segment(SegmentType::Silence, 6.0, 7.0, true),
    ];
    carry_selection(&old, &mut new);
    assert!(!new[0].checked);
    assert!(new[1].checked);
    assert!(new[2].checked);
}

#[test]
fn source_hash_uses_content_and_honors_cancellation() {
    let path = std::env::temp_dir().join(format!("vv-auto-cut-hash-{}", std::process::id()));
    std::fs::write(&path, b"abc").unwrap();
    let progress = Progress::default();
    let cancel = AtomicBool::new(false);
    let hash = hash_file(&path, &cancel, &progress).unwrap();
    assert_eq!(
        hash,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(progress.done.load(Ordering::Relaxed), 1000);
    std::fs::write(&path, b"abd").unwrap();
    assert_ne!(hash_file(&path, &cancel, &progress).unwrap(), hash);
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        hash_file(&path, &cancel, &progress).unwrap_err(),
        "cancelled"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn dropping_dialog_cancels_pending_work() {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut project = vv_core::Project::default();
    let timeline = project.timelines.insert(vv_core::Timeline {
        name: "Test".into(),
        fps: vv_core::Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks: vec![vv_core::Track::new_main()],
        markers: Vec::new(),
        master: Default::default(),
    });
    let id = project.alloc_clip_id();
    let clip = Clip::from_source_range(
        id,
        ClipSource::SolidColor,
        0,
        25,
        0,
        vv_core::Rational::one(),
    );
    let modal = Modal {
        timeline,
        target: (0, id),
        clip,
        fps: 25.0,
        media_start: 0.0,
        media_speed: 1.0,
        generation: 0,
        key: None,
        source: None,
        job: Some(SourceJob {
            cancel: cancel.clone(),
            progress: Arc::new(Progress::default()),
            rx,
        }),
        analysis_job: None,
        requested: None,
        analysis: None,
        config: Config::default(),
        ripple_all: true,
        error: None,
        original_playhead: 0,
        original_selection: Default::default(),
        active_segment: None,
        preview: true,
    };
    drop(modal);
    assert!(cancel.load(Ordering::Relaxed));
    assert!(tx.send(Err("cancelled".into())).is_err());
}

#[test]
fn adjacent_checked_rows_are_merged_before_rounding_to_frames() {
    let clip = Clip::from_source_range(
        ClipId(1),
        ClipSource::SolidColor,
        0,
        100,
        20,
        vv_core::Rational::one(),
    );
    let segments = [
        segment(SegmentType::Speech, 0.0, 1.01, true),
        segment(SegmentType::Silence, 1.01, 2.0, true),
    ];
    assert_eq!(removed_frames(&segments, &clip, 25.0), vec![(20, 70)]);
    let segments = [
        segment(SegmentType::Speech, 0.0, 1.01, true),
        segment(SegmentType::Silence, 1.01, 2.0, false),
    ];
    assert_eq!(removed_frames(&segments, &clip, 25.0), vec![(20, 45)]);
}

#[test]
fn long_waveforms_are_reduced_to_bounded_peak_buckets() {
    let mut audio = Energy {
        step: 0.01,
        db: vec![-80.0; 100_000],
    };
    audio.db[42_000] = -2.0;
    let peaks = waveform_energy(&audio, 1000.0);
    assert_eq!(peaks.db.len(), 4096);
    assert!(peaks.db.contains(&-2.0));
    assert!((peaks.step * peaks.db.len() as f64 - 1000.0).abs() < 1e-6);
}
