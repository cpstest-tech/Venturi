use super::*;

#[test]
fn frames_are_matted_within_the_side_with_their_aspect() {
    assert_eq!(matte_size(1920, 1080), (960, 540));
    assert_eq!(matte_size(1080, 1920), (540, 960));
    assert_eq!(matte_size(640, 360), (640, 360));
}

#[test]
fn the_model_input_is_padded_to_its_strides() {
    assert_eq!(padded((960, 540)), (960, 544));
    assert_eq!(padded((540, 960)), (544, 960));
}

#[test]
fn the_recurrent_states_follow_the_downsampled_input() {
    assert_eq!(
        state_shapes((960, 544)),
        vec![
            [1, 16, 136, 240],
            [1, 20, 68, 120],
            [1, 40, 34, 60],
            [1, 64, 17, 30]
        ]
    );
}

/// `VV_MATTING_VIDEO=<file> VV_MATTING_MODELS=<dir>`: mattes the first two
/// seconds of a real video.
#[test]
#[ignore]
fn mattes_a_real_video() {
    let (Some(video), Some(models)) = (
        std::env::var_os("VV_MATTING_VIDEO"),
        std::env::var_os("VV_MATTING_MODELS"),
    ) else {
        return;
    };
    let hash = 0x7E57_3A77_E000_0002;
    let _ = std::fs::remove_dir_all(matte::matte_dir(hash));
    let progress = Progress::default();
    let started = std::time::Instant::now();
    matte_media(
        Path::new(&video),
        hash,
        60,
        30.0,
        Path::new(&models),
        &progress,
        &AtomicBool::new(false),
    )
    .unwrap();
    eprintln!("60 frames in {:?}", started.elapsed());
    assert!(matte::mattes_complete(hash));
    assert!(matte::load_matte(hash, 59).is_some());
    let _ = std::fs::remove_dir_all(matte::matte_dir(hash));
}
