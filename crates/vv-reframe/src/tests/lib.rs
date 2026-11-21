use super::*;
use vv_media::{Chroma, ColorMatrix};

fn flat_frame(width: u32, height: u32, luma: u8, chroma: u8) -> FrameYuv420 {
    let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
    FrameYuv420 {
        width,
        height,
        y: vec![luma; (width * height) as usize],
        chroma: Chroma::Planar {
            u: vec![chroma; (cw * ch) as usize],
            v: vec![chroma; (cw * ch) as usize],
        },
        chroma_width: cw,
        chroma_height: ch,
        matrix: ColorMatrix::Bt709,
        full_range: false,
        alpha: None,
    }
}

#[test]
fn the_model_outputs_are_in_the_expected_order() {
    let model = tract_onnx::onnx().model_for_read(&mut &*MODEL).unwrap();
    let names: Vec<String> = model
        .outputs
        .iter()
        .map(|o| {
            model
                .outlet_label(*o)
                .map_or_else(|| model.node(o.node).name.clone(), str::to_owned)
        })
        .collect();
    let expected: Vec<String> = ["cls", "obj", "bbox", "kps"]
        .iter()
        .flat_map(|kind| STRIDES.iter().map(move |s| format!("{kind}_{s}")))
        .collect();
    assert_eq!(names, expected);
}

#[test]
fn a_frame_without_faces_gives_none() {
    let frame = flat_frame(640, 360, 120, 128);
    assert!(detect(&frame).unwrap().is_empty());
}

#[test]
fn overlapping_detections_keep_the_best() {
    let d = |x: f32, score: f32| Detection {
        rect: [x, 0.0, 100.0, 100.0],
        score,
        landmarks: [[0.0; 2]; 5],
    };
    let kept = non_max_suppression(vec![d(0.0, 0.7), d(10.0, 0.9), d(300.0, 0.8)]);
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].score, 0.9);
}

#[test]
fn frames_are_analysed_within_the_side_with_their_aspect() {
    assert_eq!(analysis_size(1920, 1080), (640, 360));
    assert_eq!(analysis_size(1080, 1920), (360, 640));
    assert_eq!(analysis_size(320, 240), (320, 240));
}

#[test]
fn a_hard_cut_is_found_and_a_fade_is_not() {
    let dark = FrameHistogram::of(&flat_frame(64, 36, 30, 110));
    let bright = FrameHistogram::of(&flat_frame(64, 36, 200, 150));
    let mut histograms: Vec<(FrameIdx, FrameHistogram)> = (0..60)
        .map(|f| (f, dark.clone()))
        .chain((60..120).map(|f| (f, bright.clone())))
        .collect();
    assert_eq!(find_cuts(&histograms, 30.0), vec![60]);

    histograms = (0..120)
        .map(|f| {
            let mut frame = flat_frame(64, 36, 0, 128);
            for (i, y) in frame.y.iter_mut().enumerate() {
                *y = (30 + f * 120 / 120 + (i % 64) as i64 * 100 / 64) as u8;
            }
            (f, FrameHistogram::of(&frame))
        })
        .collect();
    let fade = find_cuts(&histograms, 30.0);
    assert!(fade.is_empty(), "{fade:?}");
}

#[test]
fn cuts_closer_than_half_a_second_count_once() {
    let a = FrameHistogram::of(&flat_frame(64, 36, 30, 110));
    let b = FrameHistogram::of(&flat_frame(64, 36, 220, 160));
    let histograms: Vec<(FrameIdx, FrameHistogram)> = (0..90)
        .map(|f| {
            (
                f,
                if (30..35).contains(&f) || f >= 60 {
                    b.clone()
                } else {
                    a.clone()
                },
            )
        })
        .collect();
    assert_eq!(find_cuts(&histograms, 30.0), vec![30, 60]);
}

#[test]
fn mouth_motion_ignores_a_change_of_the_whole_face() {
    let before = flat_frame(200, 200, 100, 128);
    let mut now = flat_frame(200, 200, 100, 128);
    let d = Detection {
        rect: [50.0, 50.0, 100.0, 100.0],
        score: 0.9,
        landmarks: [
            [80.0, 80.0],
            [120.0, 80.0],
            [100.0, 100.0],
            [80.0, 130.0],
            [120.0, 130.0],
        ],
    };
    for y in 120..140 {
        for x in 75..125 {
            now.y[y * 200 + x] = 180;
        }
    }
    let talking = to_face(&d, &now, Some(&before));
    assert!(talking.mouth > 0.05, "{}", talking.mouth);
    let lit = flat_frame(200, 200, 160, 128);
    assert!(to_face(&d, &lit, Some(&before)).mouth < 0.01);
    assert!((talking.center[0] - 0.5).abs() < 1e-6);
}

/// `VV_REFRAME_VIDEO=<file>`: analyses the first five seconds of a real video.
#[test]
#[ignore]
fn analyses_a_real_video() {
    let Some(video) = std::env::var_os("VV_REFRAME_VIDEO") else {
        return;
    };
    let path = Path::new(&video);
    let probe = Decoder::open(path).unwrap();
    let size = (probe.width(), probe.height());
    let fps = probe.fps().as_f64();
    let started = std::time::Instant::now();
    let analysis = analyze(
        path,
        size,
        (0, (fps * 5.0) as FrameIdx),
        fps,
        5,
        &[],
        None,
        &Progress::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let faces: usize = analysis.samples.iter().map(|s| s.faces.len()).sum();
    eprintln!(
        "{} samples, {faces} faces, cuts {:?}, {:?}",
        analysis.samples.len(),
        analysis.cuts,
        started.elapsed()
    );
}
