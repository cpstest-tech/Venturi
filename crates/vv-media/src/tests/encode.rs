use super::*;

// Checks the produced file by re-reading it with the decoding functions
// already existing/tested in this same crate (`probe`/`Decoder`/
// `decode_audio_track`), instead of introducing a dependency on
// ffprobe/JSON just for the tests: the same "dogfooding" principle already
// implicit in the rest of the test suite (the fixtures generated with the
// `ffmpeg` CLI are verified by decoding them with this crate).

#[test]
fn encodes_video_only_file_with_correct_dimensions_and_frame_count() {
    let dir = crate::test_support::TempDir::new("vv-media-encode-test");
    let path = dir.join("video_only.mp4");

    let fps = vv_core::Rational::new(25, 1);
    let mut encoder = Encoder::new(&path, 16, 16, fps, &VideoSettings::default(), None).unwrap();
    // Red in BT.709 limited range.
    let red = solid_i420(16, 16, [63, 102, 240]);
    for _ in 0..25 {
        encoder.write_video_frame(&red).unwrap();
    }
    encoder.finish().unwrap();

    let meta = crate::probe::probe(&path).unwrap();
    assert_eq!(meta.width, 16);
    assert_eq!(meta.height, 16);
    assert!(!meta.has_audio, "video only, no audio");

    let mut decoder = crate::decode::Decoder::open(&path).unwrap();
    let mut count = 0;
    while decoder.next_frame().unwrap().is_some() {
        count += 1;
    }
    // With B-frames (default x264 preset) the encoder/muxer reordering
    // can shorten by one frame what is read back from the container:
    // the same tolerance already used elsewhere in this crate for
    // GOP/container rounding, not an exact count.
    assert!((24..=25).contains(&count), "count={count}");
}

#[test]
fn encodes_video_and_audio_streams_together() {
    let dir = crate::test_support::TempDir::new("vv-media-encode-test");
    let path = dir.join("video_audio.mp4");

    let fps = vv_core::Rational::new(25, 1);
    let mut encoder = Encoder::new(
        &path,
        16,
        16,
        fps,
        &VideoSettings::default(),
        Some((48000, 2, &AudioSettings::default())),
    )
    .unwrap();
    let black = solid_i420(16, 16, [16, 128, 128]);
    for _ in 0..25 {
        encoder.write_video_frame(&black).unwrap();
    }
    // 1s of stereo audio at 48kHz, a simple hand-generated sine.
    let samples: Vec<f32> = (0..48000 * 2)
        .map(|i| {
            let t = (i / 2) as f32 / 48000.0;
            (t * 440.0 * std::f32::consts::TAU).sin() * 0.5
        })
        .collect();
    encoder.write_audio_samples(&samples).unwrap();
    encoder.finish().unwrap();

    let meta = crate::probe::probe(&path).unwrap();
    assert!(meta.has_audio);
    assert_eq!(meta.channels, 2);

    let audio = crate::audio::decode_audio_track(&path, 0)
        .unwrap()
        .expect("audio expected");
    let peak = audio.samples.iter().cloned().fold(0.0_f32, f32::max);
    assert!(peak > 0.1, "peak={peak}, expected a non-silent signal");
}

#[test]
fn default_audio_export_preserves_high_frequencies_and_stereo() {
    let dir = crate::test_support::TempDir::new("vv-media-audio-quality");
    let path = dir.join("stereo.mp4");
    let mut encoder = Encoder::new(
        &path,
        16,
        16,
        vv_core::Rational::new(25, 1),
        &VideoSettings::default(),
        Some((48_000, 2, &AudioSettings::default())),
    )
    .unwrap();
    let samples: Vec<f32> = (0..48_000)
        .flat_map(|i| {
            [440.0, 19_000.0]
                .map(|hz| (i as f64 * hz * std::f64::consts::TAU / 48_000.0).sin() as f32 * 0.25)
        })
        .collect();
    encoder
        .write_video_frame(&solid_i420(16, 16, [16, 128, 128]))
        .unwrap();
    for chunk in samples.chunks(3200) {
        encoder.write_audio_samples(chunk).unwrap();
    }
    encoder.finish().unwrap();

    let audio = crate::audio::decode_audio_track(&path, 0).unwrap().unwrap();
    assert_eq!((audio.sample_rate, audio.channels), (48_000, 2));
    let middle = &audio.samples[4800 * 2..43_200 * 2];
    for (channel, hz) in [440.0, 19_000.0].into_iter().enumerate() {
        let mut real = 0.0;
        let mut imag = 0.0;
        for (i, frame) in middle.chunks_exact(2).enumerate() {
            let phase = i as f64 * hz * std::f64::consts::TAU / 48_000.0;
            real += frame[channel] as f64 * phase.cos();
            imag += frame[channel] as f64 * phase.sin();
        }
        let amplitude = 2.0 * real.hypot(imag) / (middle.len() / 2) as f64;
        assert!(
            (0.20..0.30).contains(&amplitude),
            "channel {channel} at {hz} Hz: {amplitude}"
        );
    }
}

/// Every encoder combination available on this machine must produce a
/// file with both streams.
#[test]
fn encodes_with_every_available_codec_and_preset_choice() {
    let dir = crate::test_support::TempDir::new("vv-media-encode-test");
    let fps = vv_core::Rational::new(25, 1);
    let frame = solid_i420(256, 256, [63, 102, 240]);
    let samples = vec![0.1_f32; 48000 * 2];
    let audio_choices = [
        AudioSettings::default(),
        AudioSettings {
            fast_coder: true,
            bitrate_kbps: 192,
            ..AudioSettings::default()
        },
        AudioSettings {
            codec: AudioCodec::FdkAac,
            ..AudioSettings::default()
        },
    ];
    for codec in VideoCodec::ALL.into_iter().filter(|c| c.is_available()) {
        for (i, audio) in audio_choices
            .iter()
            .filter(|a| a.codec.is_available())
            .enumerate()
        {
            let path = dir.join(format!("{codec:?}-{i}.mp4"));
            let video = VideoSettings {
                codec,
                preset: codec.presets().first().copied().unwrap_or_default().into(),
                quality: codec.default_quality(),
                bitrate: None,
            };
            let mut encoder =
                Encoder::new(&path, 256, 256, fps, &video, Some((48000, 2, audio))).unwrap();
            for _ in 0..10 {
                encoder.write_video_frame(&frame).unwrap();
            }
            encoder.write_audio_samples(&samples).unwrap();
            encoder.finish().unwrap();

            let meta = crate::probe::probe(&path).unwrap();
            assert_eq!((meta.width, meta.height), (256, 256), "{codec:?} {audio:?}");
            assert!(meta.has_audio, "{codec:?} {audio:?}");
        }
    }
}

/// x264 writes the colors right: the check of `is_available` must not
/// reject a sound encoder.
#[test]
fn x264_passes_the_color_check() {
    assert_eq!(VideoCodec::X264.check_colors().unwrap(), None);
    assert!(VideoCodec::X264.is_available());
}

#[test]
fn encodes_with_a_bitrate_target_in_both_modes() {
    let dir = crate::test_support::TempDir::new("vv-media-encode-test");
    let fps = vv_core::Rational::new(25, 1);
    let frame = solid_i420(64, 64, [63, 102, 240]);
    for constant in [true, false] {
        let path = dir.join(format!("bitrate-{constant}.mp4"));
        let video = VideoSettings {
            bitrate: Some(VideoBitrate {
                kbps: 800,
                constant,
            }),
            ..VideoSettings::default()
        };
        let mut encoder = Encoder::new(&path, 64, 64, fps, &video, None).unwrap();
        for _ in 0..25 {
            encoder.write_video_frame(&frame).unwrap();
        }
        encoder.finish().unwrap();
        let meta = crate::probe::probe(&path).unwrap();
        assert_eq!((meta.width, meta.height), (64, 64), "constant={constant}");
    }
}

#[test]
fn writes_every_available_audio_file_format() {
    let dir = crate::test_support::TempDir::new("vv-media-encode-test");
    let samples: Vec<f32> = (0..48000 * 2)
        .map(|i| ((i / 2) as f32 / 48000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    for format in AudioFileFormat::ALL
        .into_iter()
        .filter(|f| f.is_available())
    {
        let path = dir.join(format!("out.{}", format.extension()));
        let mut encoder = AudioFileEncoder::new(&path, 48000, 2, format, 192).unwrap();
        encoder.write_samples(&samples).unwrap();
        encoder.finish().unwrap();
        let meta = crate::probe::probe(&path).unwrap();
        assert!(meta.has_audio, "{format:?}");
        assert!(!meta.has_video, "{format:?}");
        let audio = crate::audio::decode_audio_track(&path, 0).unwrap().unwrap();
        let peak = audio.samples.iter().cloned().fold(0.0_f32, f32::max);
        assert!(peak > 0.1, "{format:?}: peak={peak}");
    }
}
