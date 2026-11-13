use super::*;
use vv_core::MediaFileInfo;

fn video(width: u32, height: u32, fps: Rational) -> MediaMeta {
    MediaMeta {
        duration_frames: 500,
        fps,
        width,
        height,
        has_video: true,
        has_audio: true,
        sample_rate: 48_000,
        channels: 2,
        audio_streams: 1,
        file: MediaFileInfo::default(),
    }
}

fn photo(width: u32, height: u32) -> MediaMeta {
    MediaMeta {
        duration_frames: vv_core::IMAGE_DURATION_FRAMES,
        has_audio: false,
        sample_rate: 0,
        channels: 0,
        audio_streams: 0,
        ..video(width, height, Rational::new(25, 1))
    }
}

#[test]
fn fps_rounds_to_the_nearest_standard_rate() {
    for (fps, expected) in [
        (23.976, 24),
        (24.0, 24),
        (25.0, 25),
        (29.97, 30),
        (47.95, 50),
        (50.0, 50),
        (59.94, 60),
        (120.0, 60),
        (12.0, 24),
        (27.5, 25),
    ] {
        assert_eq!(round_fps(fps), Rational::new(expected, 1), "{fps}");
    }
}

#[test]
fn a_photo_sets_30_fps_whatever_its_pool_rate() {
    let format = project_format(&photo(4000, 3000), 0);
    assert_eq!(format.fps, Rational::new(30, 1));
}

#[test]
fn native_size_is_kept_up_to_4k() {
    assert_eq!(fit_to_max((1920, 1080)), (1920, 1080));
    assert_eq!(fit_to_max((3840, 2160)), (3840, 2160));
    assert_eq!(fit_to_max((7680, 4320)), (3840, 2160));
    assert_eq!(fit_to_max((2160, 3840)), (2160, 3840));
    assert_eq!(fit_to_max((4320, 7680)), (2160, 3840));
    assert_eq!(fit_to_max((6000, 4000)), (3240, 2160));
}

#[test]
fn sides_are_even() {
    assert_eq!(fit_to_max((1081, 1921)), (1080, 1920));
}

#[test]
fn rotation_swaps_the_orientation() {
    assert_eq!(effective_size((1920, 1080), 0), (1920, 1080));
    assert_eq!(effective_size((1920, 1080), 90), (1080, 1920));
    assert_eq!(effective_size((1920, 1080), 180), (1920, 1080));
    assert_eq!(effective_size((1920, 1080), 270), (1080, 1920));
    let format = project_format(&video(1920, 1080, Rational::new(30, 1)), 90);
    assert_eq!(format.resolution, (1080, 1920));
    assert_eq!(format.ratio, Ratio::R9x16);
}

#[test]
fn a_matching_ratio_picks_the_preset_otherwise_original() {
    assert_eq!(Ratio::of((1920, 1080)), Ratio::R16x9);
    assert_eq!(Ratio::of((1080, 1920)), Ratio::R9x16);
    assert_eq!(Ratio::of((1080, 1080)), Ratio::R1x1);
    assert_eq!(Ratio::of((1440, 1080)), Ratio::R4x3);
    assert_eq!(Ratio::of((2560, 1080)), Ratio::Original);
    assert_eq!(Ratio::of((1000, 1000)), Ratio::R1x1);
}

#[test]
fn a_video_defines_resolution_fps_and_ratio() {
    let format = project_format(&video(3840, 2160, Rational::new(2997, 100)), 0);
    assert_eq!(format.resolution, (3840, 2160));
    assert_eq!(format.fps, Rational::new(30, 1));
    assert_eq!(format.ratio, Ratio::R16x9);
}

#[test]
fn rotation_is_read_from_the_display_matrix_counter_clockwise() {
    let json =
        r#"{"streams":[{"side_data_list":[{"side_data_type":"Display Matrix","rotation":-90}]}]}"#;
    assert_eq!(parse_rotation(json), 90);
    let json = r#"{"streams":[{"side_data_list":[{"rotation":90}]}]}"#;
    assert_eq!(parse_rotation(json), 270);
    let json = r#"{"streams":[{"side_data_list":[{"rotation":180}]}]}"#;
    assert_eq!(parse_rotation(json), 180);
}

#[test]
fn rotation_falls_back_to_the_rotate_tag() {
    let json = r#"{"streams":[{"tags":{"rotate":"270"}}]}"#;
    assert_eq!(parse_rotation(json), 270);
}

#[test]
fn no_rotation_or_bad_output_is_zero() {
    assert_eq!(parse_rotation(r#"{"streams":[{}]}"#), 0);
    assert_eq!(parse_rotation(r#"{"streams":[]}"#), 0);
    assert_eq!(parse_rotation("not json"), 0);
}

#[test]
fn a_quarter_turn_zooms_the_frame_to_fill_the_canvas() {
    let (turn, zoom) = rotation_fit((1920, 1080), (1080, 1920), 90);
    assert_eq!(turn, 90.0);
    assert!((zoom - 1920.0 / 1080.0).abs() < 1e-4, "{zoom}");
    let (turn, zoom) = rotation_fit((1920, 1080), (1920, 1080), 180);
    assert_eq!((turn, zoom), (180.0, 1.0));
    assert_eq!(rotation_fit((1920, 1080), (1920, 1080), 0), (0.0, 1.0));
}

#[test]
fn ratio_resolution_keeps_the_longer_side() {
    assert_eq!(Ratio::R9x16.resolution((1920, 1080), None), (1080, 1920));
    assert_eq!(Ratio::R16x9.resolution((1080, 1920), None), (1920, 1080));
    assert_eq!(Ratio::R1x1.resolution((1920, 1080), None), (1920, 1920));
    assert_eq!(Ratio::R4x3.resolution((1920, 1080), None), (1920, 1440));
}

#[test]
fn ratio_original_uses_the_first_media_or_the_current_size() {
    assert_eq!(
        Ratio::Original.resolution((1920, 1080), Some((1000, 1000))),
        (1000, 1000)
    );
    assert_eq!(Ratio::Original.resolution((1920, 1080), None), (1920, 1080));
}

#[test]
fn ratio_round_trips_through_the_resolution() {
    for ratio in [Ratio::R16x9, Ratio::R9x16, Ratio::R1x1, Ratio::R4x3] {
        assert_eq!(Ratio::of(ratio.resolution((1920, 1080), None)), ratio);
    }
}
