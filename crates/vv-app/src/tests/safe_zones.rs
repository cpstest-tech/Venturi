use super::*;

#[test]
fn the_bundled_file_has_the_three_platforms() {
    let platforms = parse(BUNDLED).unwrap();
    let ids: Vec<_> = platforms.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["tiktok", "reels", "shorts"]);
    for p in &platforms {
        assert!(p.zones.len() >= 4, "{}", p.id);
        for z in &p.zones {
            let [l, t, r, b] = z.fractions();
            assert!(l < r && t < b, "{}: {:?}", p.id, z);
            assert!(!z.label("it").is_empty() && !z.label("en").is_empty());
        }
    }
}

#[test]
fn bad_files_are_refused() {
    assert!(parse("{").is_err());
    assert!(parse(r#"{"version": 2, "platforms": []}"#).is_err());
    assert!(parse(r#"{"version": 1, "platforms": []}"#).is_err());
}

#[test]
fn unknown_platform_falls_back_to_the_first() {
    let platforms = parse(BUNDLED).unwrap();
    assert_eq!(platform(&platforms, "nope").unwrap().id, "tiktok");
    assert_eq!(platform(&platforms, "shorts").unwrap().id, "shorts");
}

#[test]
fn text_in_the_caption_area_overlaps_and_centred_text_does_not() {
    let platforms = parse(BUNDLED).unwrap();
    let tiktok = platform(&platforms, "tiktok").unwrap();
    assert!(overlaps(tiktok, [0.1, 0.75, 0.6, 0.8]));
    assert!(!overlaps(tiktok, [0.2, 0.4, 0.7, 0.5]));
}

#[test]
fn values_out_of_range_are_clamped() {
    let zone = Zone {
        label: BTreeMap::new(),
        x: -10.0,
        y: 90.0,
        w: 50.0,
        h: 30.0,
    };
    assert_eq!(zone.fractions(), [0.0, 0.9, 0.4, 1.0]);
    assert_eq!(zone.label("it"), "");
}
