use super::*;

fn card(name: &str, size_bytes: u64, modified_secs: u64, aspect: Aspect) -> HomeCard {
    HomeCard {
        summary: ProjectSummary {
            file: PathBuf::from(format!("/p/{name}/{name}.vvproj")),
            name: name.to_owned(),
            size_bytes,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs)),
            duration_secs: None,
            aspect,
            cover: None,
            error: None,
        },
        cover: None,
    }
}

fn names(cards: &[HomeCard], shown: &[usize]) -> Vec<String> {
    shown
        .iter()
        .map(|&i| cards[i].summary.name.clone())
        .collect()
}

#[test]
fn cards_sort_by_recency_name_or_size() {
    let cards = [
        card("beta", 10, 300, Aspect::Wide),
        card("Alfa", 30, 100, Aspect::Vertical),
        card("gamma", 20, 200, Aspect::Wide),
    ];
    let order = |sort| names(&cards, &visible(&cards, "", sort, None));
    assert_eq!(order(HomeSort::Recent), ["beta", "gamma", "Alfa"]);
    assert_eq!(order(HomeSort::Name), ["Alfa", "beta", "gamma"]);
    assert_eq!(order(HomeSort::Size), ["Alfa", "gamma", "beta"]);
}

#[test]
fn search_ignores_case_and_the_format_filter_narrows_further() {
    let cards = [
        card("Vacanze Mare", 1, 1, Aspect::Wide),
        card("vacanze montagna", 1, 2, Aspect::Vertical),
        card("Matrimonio", 1, 3, Aspect::Wide),
    ];
    let shown = visible(&cards, "  VACANZE ", HomeSort::Name, None);
    assert_eq!(names(&cards, &shown), ["Vacanze Mare", "vacanze montagna"]);
    let shown = visible(&cards, "vacanze", HomeSort::Name, Some(Aspect::Vertical));
    assert_eq!(names(&cards, &shown), ["vacanze montagna"]);
    assert!(visible(&cards, "", HomeSort::Name, Some(Aspect::Square)).is_empty());
}

#[test]
fn sizes_and_durations_read_naturally() {
    assert_eq!(format_size(2_400), "2.4 KB");
    assert_eq!(format_size(12_300_000), "12 MB");
    assert_eq!(format_size(3_500_000_000), "3.5 GB");
    assert_eq!(format_duration(65.4), "1:05");
    assert_eq!(format_duration(3_725.0), "1:02:05");
}

#[test]
fn utc_dates_match_the_calendar() {
    assert_eq!(utc_time(0), (1970, 1, 1, 0, 0));
    // 2024-02-29 13:45 UTC.
    assert_eq!(utc_time(1_709_214_300), (2024, 2, 29, 13, 45));
    // 2026-11-13 00:00 UTC.
    assert_eq!(utc_time(1_794_528_000), (2026, 11, 13, 0, 0));
}
