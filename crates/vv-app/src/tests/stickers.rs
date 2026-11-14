use super::*;
use std::collections::HashSet;

fn decode(png_bytes: &[u8]) -> (u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    (info.width, buf)
}

#[test]
fn ids_and_file_stems_are_unique() {
    let all = all();
    assert_eq!(
        all.iter().map(|s| &s.id).collect::<HashSet<_>>().len(),
        all.len()
    );
    let stems: HashSet<_> = all.iter().map(Sticker::file_stem).collect();
    assert_eq!(stems.len(), all.len());
}

#[test]
fn every_category_has_stickers() {
    for category in StickerCategory::ALL {
        let count = all().iter().filter(|s| s.category == category).count();
        match category {
            StickerCategory::Yours => assert_eq!(count, 0),
            _ => assert!(count > 0, "{category:?} is empty"),
        }
    }
}

#[test]
fn emoji_groups_stay_in_range() {
    for sticker in all() {
        if let Source::Noto { group, .. } = sticker.source {
            assert!((group as usize) < EMOJI_GROUPS);
        }
    }
}

#[test]
fn search_matches_every_word_of_the_query() {
    let heart = find("shape:heart").unwrap();
    assert!(heart.matches("shape heart"));
    assert!(heart.matches("HEART"));
    assert!(!heart.matches("heart blue"));
    assert!(heart.matches(""));
}

#[test]
fn every_shape_renders() {
    for sticker in all()
        .iter()
        .filter(|s| matches!(s.source, Source::Shape(_)))
    {
        let Source::Shape(shape) = sticker.source else {
            unreachable!()
        };
        let png = rasterize(&shape_svg(shape), 32, sticker.halo()).unwrap();
        let (width, pixels) = decode(&png);
        assert_eq!(width, 32);
        assert!(
            pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
            "{} is empty",
            sticker.id
        );
    }
}

#[test]
fn icons_render_white_with_a_dark_halo() {
    let lucide_heart = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5"/></svg>"#;
    let (_, with_halo) = decode(&rasterize(&tint_icon(lucide_heart), 128, true).unwrap());
    let (_, plain) = decode(&rasterize(&tint_icon(lucide_heart), 128, false).unwrap());
    assert!(
        with_halo
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] == 255 && p[0] > 240)
    );
    let covered = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count()
    };
    assert!(covered(&with_halo) > covered(&plain));
    assert!(
        with_halo
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] > 0 && p[0] < 20)
    );
}

#[test]
fn a_bad_svg_is_an_error() {
    assert!(rasterize("not svg", 32, false).is_err());
}

#[test]
fn urls_point_at_the_requested_size() {
    let smile = find("noto:1f600").unwrap();
    assert!(
        smile
            .url(Size::Thumb)
            .unwrap()
            .contains("/128/emoji_u1f600.png")
    );
    assert!(
        smile
            .url(Size::Full)
            .unwrap()
            .contains("/512/emoji_u1f600.png")
    );
    assert!(find("shape:star").unwrap().url(Size::Full).is_none());
}

#[test]
fn shapes_are_produced_without_network() {
    let dir = std::env::temp_dir().join(format!("vv-stickers-{}", std::process::id()));
    let sticker = find("shape:star").unwrap();
    produce(&dir, sticker, Size::Thumb).unwrap();
    assert!(image_path(&dir, sticker, Size::Thumb).is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs network access"]
fn downloads_each_source() {
    let dir = std::env::var_os("VV_STICKER_TEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("vv-stickers-net-{}", std::process::id()))
        });
    for id in [
        "noto:1f600",
        "lucide:heart",
        "phosphor:heart",
        "phosphor:instagram-logo",
    ] {
        let sticker = find(id).unwrap();
        for size in [Size::Thumb, Size::Full] {
            produce(&dir, sticker, size).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(image_path(&dir, sticker, size).is_file());
        }
    }
}

#[test]
fn animated_stickers_are_videos_and_the_rest_images() {
    let smile = find("noto:1f600").unwrap();
    assert!(smile.animated);
    assert_eq!(smile.file_name(), "noto-1f600.webm");
    let arrow = find("anim:arrow-right-bounce").unwrap();
    assert!(arrow.animated && arrow.matches("animated"));
    assert_eq!(arrow.file_name(), "anim-arrow-right-bounce.webm");
    let heart = find("shape:heart").unwrap();
    assert!(!heart.animated);
    assert_eq!(heart.file_name(), "shape-heart.png");
    let dir = Path::new("/s");
    assert_eq!(
        image_path(dir, arrow, Size::Thumb),
        Path::new("/s/thumb/anim-arrow-right-bounce.png")
    );
    assert_eq!(
        image_path(dir, arrow, Size::Full),
        Path::new("/s/full/anim-arrow-right-bounce.webm")
    );
}

#[test]
fn only_white_catalog_stickers_are_tintable() {
    for name in [
        "shape-star.png",
        "lucide-heart.png",
        "phosphor-heart.png",
        "anim-ripple.webm",
    ] {
        assert!(
            is_tintable_path(Path::new(&format!("/lib/{name}"))),
            "{name}"
        );
    }
    for name in ["noto-1f600.webm", "photo.png", "my-shape-star.png"] {
        assert!(
            !is_tintable_path(Path::new(&format!("/lib/{name}"))),
            "{name}"
        );
    }
}

#[test]
#[ignore = "needs network access"]
fn animated_stickers_encode_to_looping_webm() {
    let dir = std::env::var_os("VV_STICKER_TEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("vv-stickers-anim-{}", std::process::id()))
        });
    for id in ["anim:arrow-draw-arc", "anim:bell-ring", "noto:1f600"] {
        let sticker = find(id).unwrap();
        let started = std::time::Instant::now();
        produce(&dir, sticker, Size::Full).unwrap_or_else(|e| panic!("{id}: {e}"));
        eprintln!("{id}: {:?}", started.elapsed());
        let path = image_path(&dir, sticker, Size::Full);
        assert!(vv_media::has_alpha(&path), "{id} lost its alpha");
        let meta = vv_media::probe(&path).unwrap();
        let secs = meta.duration_frames as f64 / meta.fps.as_f64();
        assert!(secs >= 5.5, "{id} lasts {secs}");
    }
}

#[test]
#[ignore = "needs network access; writes every animated sticker still to VV_STICKER_TEST_DIR"]
fn every_animated_sticker_has_a_still() {
    let dir = PathBuf::from(std::env::var_os("VV_STICKER_TEST_DIR").expect("VV_STICKER_TEST_DIR"));
    for sticker in all().iter().filter(|s| matches!(s.source, Source::Anim(_))) {
        produce(&dir, sticker, Size::Thumb).unwrap_or_else(|e| panic!("{}: {e}", sticker.id));
    }
}
