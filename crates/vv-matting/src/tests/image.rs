use super::*;

fn frame(width: u32, height: u32, alpha: Option<Vec<u8>>) -> FrameYuv420 {
    let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
    let chroma = vec![128; (cw * ch) as usize];
    FrameYuv420 {
        width,
        height,
        y: vec![128; (width * height) as usize],
        chroma: vv_media::Chroma::Planar {
            u: chroma.clone(),
            v: chroma,
        },
        chroma_width: cw,
        chroma_height: ch,
        matrix: ColorMatrix::Bt709,
        full_range: true,
        alpha,
    }
}

#[test]
fn large_images_keep_a_matte_within_the_side_with_their_aspect() {
    assert_eq!(stored_size(4096, 2048), (2048, 1024));
    assert_eq!(stored_size(512, 512), (512, 512));
}

#[test]
fn the_prediction_is_stretched_back_to_the_image_and_rescaled_to_its_range() {
    // Left half background, right half subject, never reaching 0 or 1.
    let prediction = [0.2, 0.6, 0.2, 0.6];
    let matte = resample(&prediction, 2, (4, 1), &frame(4, 1, None));
    assert_eq!(matte.width, 4);
    assert_eq!(matte.data[0], 0);
    assert_eq!(matte.data[3], 255);
    assert!(matte.data[1] < matte.data[2]);
}

#[test]
fn transparent_pixels_of_the_image_stay_transparent() {
    let prediction = [1.0, 1.0, 1.0, 0.0];
    let matte = resample(
        &prediction,
        2,
        (2, 2),
        &frame(2, 2, Some(vec![255, 0, 255, 255])),
    );
    assert_eq!(matte.data[0], 255);
    assert_eq!(matte.data[1], 0);
}

/// `VV_MATTING_IMAGE=<file> VV_MATTING_MODELS=<dir>`: mattes a real image.
#[test]
#[ignore]
fn mattes_a_real_image() {
    let (Some(image), Some(models)) = (
        std::env::var_os("VV_MATTING_IMAGE"),
        std::env::var_os("VV_MATTING_MODELS"),
    ) else {
        return;
    };
    let hash = 0x7E57_3A77_E000_0003;
    let _ = std::fs::remove_dir_all(matte::matte_dir(hash));
    let started = std::time::Instant::now();
    matte_image(
        Path::new(&image),
        hash,
        Path::new(&models),
        &Progress::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    eprintln!("matted in {:?}", started.elapsed());
    assert!(matte::mattes_complete(hash));
    let matte = matte::load_matte(hash, 0).unwrap();
    assert!(matte.data.contains(&0) && matte.data.contains(&255));
    let _ = std::fs::remove_dir_all(matte::matte_dir(hash));
}
