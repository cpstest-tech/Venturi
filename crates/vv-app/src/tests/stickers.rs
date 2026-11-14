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
    let heart = find("shape:heart:red").unwrap();
    assert!(heart.matches("heart red"));
    assert!(heart.matches("RED"));
    assert!(!heart.matches("heart blue"));
    assert!(heart.matches(""));
}

#[test]
fn every_shape_renders_in_every_color() {
    for sticker in all()
        .iter()
        .filter(|s| matches!(s.source, Source::Shape { .. }))
    {
        let Source::Shape { shape, color } = sticker.source else {
            unreachable!()
        };
        let png = rasterize(&shape_svg(shape, color), 32, sticker.halo()).unwrap();
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
    assert!(find("shape:star:red").unwrap().url(Size::Full).is_none());
}

#[test]
fn shapes_are_produced_without_network() {
    let dir = std::env::temp_dir().join(format!("vv-stickers-{}", std::process::id()));
    let sticker = find("shape:star:yellow").unwrap();
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
