use super::*;

#[test]
fn time_round_trips() {
    assert_eq!(format_time(72.04), "1:12.0");
    assert_eq!(format_time(3725.55), "1:02:05.6");
    assert_eq!(parse_time("1:12.5"), Some(72.5));
    assert_eq!(parse_time(" 90 "), Some(90.0));
    assert_eq!(parse_time("1:02:05"), Some(3725.0));
    assert_eq!(parse_time(&format_time(42.3)), Some(42.3));
}

#[test]
fn rejects_malformed_times() {
    assert_eq!(parse_time(""), None);
    assert_eq!(parse_time("1:75"), None);
    assert_eq!(parse_time("-3"), None);
    assert_eq!(parse_time("1.5:00"), None);
    assert_eq!(parse_time("1:2:3:4"), None);
    assert_eq!(parse_time("abc"), None);
}

#[test]
fn download_bar_only_on_music_sfx_and_project_media() {
    assert_eq!(
        RibbonTab::Audio.download_destination(Category::Sfx),
        Some(Destination::Sfx)
    );
    assert_eq!(
        RibbonTab::Media.download_destination(Category::Project),
        Some(Destination::Project)
    );
    assert_eq!(
        RibbonTab::Audio.download_destination(Category::Favorites),
        None
    );
    assert_eq!(
        RibbonTab::Media.download_destination(Category::Favorites),
        None
    );
}
