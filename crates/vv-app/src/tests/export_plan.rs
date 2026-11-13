use super::*;
use vv_core::{Clip, MediaItem, MediaMeta, Timeline, Track};

fn meta(width: u32, height: u32, fps: Rational, frames: i64, audio: bool) -> MediaMeta {
    MediaMeta {
        duration_frames: frames,
        fps,
        width,
        height,
        has_video: true,
        has_audio: audio,
        sample_rate: 48_000,
        channels: 2,
        audio_streams: u16::from(audio),
        file: Default::default(),
    }
}

fn item(meta: MediaMeta) -> MediaItem {
    MediaItem {
        path: "x.mp4".into(),
        meta,
        content_hash: 1,
        compound: None,
        folder: None,
    }
}

struct Fixture {
    project: Project,
    timeline: TimelineId,
}

/// A video track with one clip per entry `(width, height, fps, frames)`.
fn fixture(clips: &[(u32, u32, u32, i64)]) -> Fixture {
    let mut project = Project::default();
    let mut track = Track::new_main();
    let mut start = 0;
    for &(w, h, fps, frames) in clips {
        let media = project
            .media_pool
            .insert(item(meta(w, h, Rational::new(fps as i32, 1), frames, true)));
        let id = project.alloc_clip_id();
        track.clips.push(Clip::from_source_range(
            id,
            ClipSource::Media(media),
            0,
            frames,
            start,
            Rational::one(),
        ));
        start += frames;
    }
    let timeline = project.timelines.insert(Timeline {
        name: "t".into(),
        fps: Rational::new(30, 1),
        resolution: (1920, 1080),
        tracks: vec![track],
        markers: Vec::new(),
        master: Default::default(),
    });
    Fixture { project, timeline }
}

#[test]
fn the_short_side_names_the_resolution_of_any_orientation() {
    assert_eq!(scaled_size((1920, 1080), 720), (1280, 720));
    assert_eq!(scaled_size((1080, 1920), 1080), (1080, 1920));
    assert_eq!(scaled_size((1080, 1920), 720), (720, 1280));
    assert_eq!(scaled_size((1920, 1080), 2160), (3840, 2160));
    assert_eq!(scaled_size((1080, 1080), 480), (480, 480));
}

#[test]
fn the_default_resolution_follows_the_project_or_is_1080p() {
    assert_eq!(default_resolution(720, true), ResolutionChoice::P720);
    assert_eq!(default_resolution(2160, true), ResolutionChoice::P4K);
    assert_eq!(default_resolution(720, false), ResolutionChoice::P1080);
    assert_eq!(default_resolution(1000, true), ResolutionChoice::P1080);
}

#[test]
fn match_best_takes_the_largest_video() {
    let f = fixture(&[(1280, 720, 30, 30), (3840, 2160, 60, 30), (1080, 1920, 25, 30)]);
    let media = active_video_media(&f.project, f.timeline);
    assert_eq!(media.len(), 3);
    assert_eq!(best_short_side(&f.project, &media), Some(2160));
    assert_eq!(best_fps(&f.project, &media), Some(Rational::new(60, 1)));
    let short = target_short_side(ResolutionChoice::MatchBest, 1080, Some(2160));
    assert_eq!(short, 2160);
    assert_eq!(target_short_side(ResolutionChoice::MatchBest, 1080, None), 1080);
}

#[test]
fn hidden_disabled_and_photo_clips_are_ignored() {
    let mut f = fixture(&[(1920, 1080, 30, 30), (3840, 2160, 60, 30), (3840, 2160, 60, 30)]);
    let photo = f.project.media_pool.insert(item(meta(
        8000,
        6000,
        Rational::new(30, 1),
        vv_core::IMAGE_DURATION_FRAMES,
        false,
    )));
    let timeline = &mut f.project.timelines[f.timeline];
    timeline.tracks[0].clips[1].disabled = true;
    let mut hidden = Track::new(TrackKind::Video);
    hidden.muted = true;
    hidden.clips.push(timeline.tracks[0].clips[2].clone());
    timeline.tracks[0].clips.truncate(2);
    timeline.tracks.push(hidden);
    let id = f.project.alloc_clip_id();
    f.project.timelines[f.timeline].tracks[0].clips.push(Clip::from_source_range(
        id,
        ClipSource::Media(photo),
        0,
        150,
        60,
        Rational::one(),
    ));
    let media = active_video_media(&f.project, f.timeline);
    assert_eq!(media.len(), 1);
    assert_eq!(best_short_side(&f.project, &media), Some(1080));
}

#[test]
fn compound_clips_count_through_their_content() {
    let mut f = fixture(&[(3840, 2160, 60, 30)]);
    let inner = f.timeline;
    let outer = f.project.timelines.insert(Timeline {
        name: "outer".into(),
        fps: Rational::new(30, 1),
        resolution: (1920, 1080),
        tracks: vec![Track::new_main()],
        markers: Vec::new(),
        master: Default::default(),
    });
    let mut compound = item(meta(1920, 1080, Rational::new(30, 1), 30, false));
    compound.compound = Some(inner);
    let media = f.project.media_pool.insert(compound);
    let id = f.project.alloc_clip_id();
    f.project.timelines[outer].tracks[0].clips.push(Clip::from_source_range(
        id,
        ClipSource::Media(media),
        0,
        30,
        0,
        Rational::one(),
    ));
    let found = active_video_media(&f.project, outer);
    assert_eq!(best_short_side(&f.project, &found), Some(2160));
}

#[test]
fn ffprobe_output_gives_the_stream_bitrate_or_the_container_one() {
    assert_eq!(parse_bitrate_kbps("60000000\n61000000\n"), Some(60_000));
    assert_eq!(parse_bitrate_kbps("N/A\n25000000\n"), Some(25_000));
    assert_eq!(parse_bitrate_kbps("N/A\nN/A\n"), None);
    assert_eq!(parse_bitrate_kbps(""), None);
}

#[test]
fn match_best_uses_the_highest_bitrate_and_falls_back_to_recommended() {
    let detected = [12_000u32, 25_000, 60_000].into_iter().max().map(|k| f64::from(k) / 1000.0);
    assert_eq!(choose_mbps(BitrateChoice::MatchBest, 8.0, detected, 5.0), 60.0);
    assert_eq!(choose_mbps(BitrateChoice::MatchBest, 8.0, None, 5.0), 8.0);
}

#[test]
fn recommended_follows_size_fps_and_codec() {
    let hd = recommended_mbps((1920, 1080), 30.0, 1.0);
    assert!((5.0..=8.0).contains(&hd), "{hd}");
    assert!(recommended_mbps((3840, 2160), 30.0, 1.0) > hd * 3.5);
    assert!(recommended_mbps((1920, 1080), 60.0, 1.0) > hd * 1.9);
    assert!(recommended_mbps((1920, 1080), 30.0, 0.65) < hd);
}

#[test]
fn lower_and_higher_scale_the_recommendation() {
    let lower = choose_mbps(BitrateChoice::Lower, 10.0, None, 0.0);
    let higher = choose_mbps(BitrateChoice::Higher, 10.0, None, 0.0);
    assert_eq!((lower, higher), (7.0, 15.0));
    assert_eq!(choose_mbps(BitrateChoice::Custom, 10.0, None, 42.0), 42.0);
}

#[test]
fn the_estimate_is_bitrate_times_seconds_over_eight() {
    let estimate = Estimate {
        video_mbps: 5.8,
        has_video: true,
        video_has_audio: true,
        audio_file: None,
        seconds: 46.0,
    };
    let expected = (5.8 + 0.192) * 46.0 / 8.0;
    assert!((estimate.megabytes() - expected).abs() < 1e-9);
    let silent = Estimate {
        video_has_audio: false,
        ..estimate
    };
    assert!((silent.megabytes() - 5.8 * 46.0 / 8.0).abs() < 1e-9);
}

#[test]
fn an_audio_only_estimate_uses_the_audio_format() {
    let estimate = Estimate {
        video_mbps: 50.0,
        has_video: false,
        video_has_audio: true,
        audio_file: Some(AudioFormat::Wav),
        seconds: 10.0,
    };
    assert!((estimate.megabytes() - 1.536 * 10.0 / 8.0).abs() < 1e-9);
}

#[test]
fn durations_and_sizes_are_shown_compactly() {
    assert_eq!(format_duration(46.0), "46s");
    assert_eq!(format_duration(125.0), "2m 05s");
    assert_eq!(format_duration(3725.0), "1h 02m 05s");
    assert_eq!(format_megabytes(34.2), "34 MB");
    assert_eq!(format_megabytes(3.46), "3.5 MB");
    assert_eq!(format_megabytes(1500.0), "1.5 GB");
}

#[test]
fn a_taken_name_gets_a_numeric_suffix() {
    let taken = |name: &str| ["clip", "clip_1"].contains(&name);
    assert_eq!(unique_name("other", taken), "other");
    assert_eq!(unique_name("clip", taken), "clip_2");
    assert_eq!(sanitize_name(" a/b "), "a_b");
}

#[test]
fn av1_does_not_go_in_mov() {
    assert!(!valid_combination(CodecChoice::Av1, Container::Mov));
    assert!(valid_combination(CodecChoice::Av1, Container::Mp4));
    assert!(valid_combination(CodecChoice::H264, Container::Mov));
}

#[test]
fn only_what_the_engine_encodes_is_available() {
    assert!(CodecChoice::H264.available());
    assert!(!CodecChoice::Hevc.available());
    assert!(!CodecChoice::Av1.available());
}

#[test]
fn the_frame_rate_follows_the_choice() {
    let timeline = Rational::new(30, 1);
    let best = Some(Rational::new(60, 1));
    assert_eq!(target_fps(FpsChoice::F24, timeline, best), Rational::new(24, 1));
    assert_eq!(target_fps(FpsChoice::MatchBest, timeline, best), Rational::new(60, 1));
    assert_eq!(target_fps(FpsChoice::MatchBest, timeline, None), timeline);
}
