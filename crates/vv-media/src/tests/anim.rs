use super::*;
use crate::test_support::TempDir;

fn pixel(rgba: [u8; 4], count: usize) -> Vec<u8> {
    rgba.iter().copied().cycle().take(count * 4).collect()
}

#[test]
fn a_loop_repeats_until_it_is_long_enough() {
    let indices = loop_indices(&[0.1, 0.1], 10, 1.0);
    assert_eq!(indices.len(), 10);
    assert_eq!(&indices[..4], &[0, 1, 0, 1]);
}

#[test]
fn frame_delays_decide_how_long_each_frame_stays() {
    let indices = loop_indices(&[0.3, 0.1], 10, 0.4);
    assert_eq!(indices, vec![0, 0, 0, 1]);
}

#[test]
fn an_empty_animation_has_no_frames() {
    assert!(loop_indices(&[], 30, 3.0).is_empty());
}

#[test]
fn a_webm_keeps_the_alpha_the_frames_had() {
    let dir = TempDir::new("vv-media-anim");
    let path = dir.join("sticker.webm");
    let (w, h) = (32u32, 32u32);
    let frames = (0..6).map(|_| {
        let mut rgba = pixel([255, 0, 0, 255], (w * h) as usize);
        for px in rgba.as_chunks_mut::<4>().0.iter_mut().take((w * h / 2) as usize) {
            px[3] = 0;
        }
        rgba
    });
    encode_alpha_webm(&path, w, h, 30, frames).unwrap();

    assert!(crate::probe::has_alpha(&path));
    let mut decoder = crate::decode::Decoder::open(&path).unwrap();
    let (_, frame) = decoder.next_frame().unwrap().unwrap();
    let alpha = frame.alpha.as_ref().expect("alpha plane");
    let (top, bottom) = alpha.split_at(alpha.len() / 2);
    assert!(top.iter().all(|&a| a < 8), "top is transparent");
    assert!(bottom.iter().all(|&a| a > 247), "bottom is opaque");
    let luma = frame.y[frame.y.len() - 1];
    assert!(
        (i32::from(luma) - 82).abs() <= 4,
        "red in BT.601 limited, got {luma}"
    );
    assert_eq!(frame.matrix, crate::decode::ColorMatrix::Bt601);
    let mut count = 1;
    while decoder.next_frame().unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 6);
}

#[test]
fn an_opaque_video_has_no_alpha() {
    let dir = TempDir::new("vv-media-anim-opaque");
    let path = dir.join("plain.mp4");
    let fps = vv_core::Rational::new(25, 1);
    let mut encoder = crate::encode::Encoder::new(
        &path,
        16,
        16,
        fps,
        &crate::encode::VideoSettings::default(),
        None,
    )
    .unwrap();
    let grey = crate::encode::solid_i420(16, 16, [126, 128, 128]);
    for _ in 0..5 {
        encoder.write_video_frame(&grey).unwrap();
    }
    encoder.finish().unwrap();
    assert!(!crate::probe::has_alpha(&path));
}

#[test]
fn an_animated_gif_decodes_into_frames_with_delays() {
    let dir = TempDir::new("vv-media-anim-gif");
    let path = dir.join("a.gif");
    let status = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc=size=32x32:rate=10:duration=1")
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    let animation = decode_animation(&path).unwrap();
    assert_eq!((animation.width, animation.height), (32, 32));
    assert_eq!(animation.frames.len(), 10);
    assert_eq!(animation.frames[0].len(), 32 * 32 * 4);
    assert!(animation.delays.iter().all(|d| (d - 0.1).abs() < 0.02));
}

#[test]
fn an_odd_size_is_padded_to_an_even_one() {
    let dir = TempDir::new("vv-media-anim-odd");
    let path = dir.join("odd.webm");
    let frames = (0..3).map(|_| pixel([0, 255, 0, 255], 31 * 21));
    encode_alpha_webm(&path, 31, 21, 30, frames).unwrap();
    let meta = crate::probe::probe(&path).unwrap();
    assert_eq!((meta.width, meta.height), (32, 22));
}
