use super::*;

// Trimmed copies of real embed pages (November 2026).
const TRACK: &str = r#"<html><script id="__NEXT_DATA__" type="application/json">{"props": {"pageProps": {"state": {"data": {"entity": {"type": "track", "name": "Never Gonna Give You Up", "title": "Never Gonna Give You Up", "artists": [{"name": "Rick Astley"}], "duration": 213573, "isPlayable": true}}}}}}</script></html>"#;
const PLAYLIST: &str = "<script id=\"__NEXT_DATA__\" type=\"application/json\">{\"props\": {\"pageProps\": {\"state\": {\"data\": {\"entity\": {\"type\": \"playlist\", \"name\": \"Today’s Top Hits\", \"duration\": 0, \"trackList\": [{\"title\": \"BbY WOW\", \"subtitle\": \"KAROL G,\u{a0}Judeline,\u{a0}rusowsky\", \"duration\": 225834, \"isPlayable\": true}, {\"title\": \"Gone\", \"subtitle\": \"X\", \"duration\": 1000, \"isPlayable\": false}, {\"title\": \"Housewife\", \"subtitle\": \"Tate McRae\", \"duration\": 208810, \"isPlayable\": true}]}}}}}}</script>";

fn candidate(title: &str, channel: &str, duration: f64) -> Candidate {
    Candidate {
        url: format!("https://www.youtube.com/watch?v={title}"),
        title: title.into(),
        channel: channel.into(),
        duration: Some(duration),
    }
}

#[test]
fn parses_a_track_page() {
    let collection = parse_embed(TRACK).unwrap();
    assert_eq!(
        collection.tracks,
        vec![Track {
            title: "Never Gonna Give You Up".into(),
            artists: "Rick Astley".into(),
            duration: 213.573,
        }]
    );
    assert_eq!(
        collection.tracks[0].search_query(),
        "ytsearch8:Rick Astley - Never Gonna Give You Up"
    );
}

#[test]
fn parses_a_playlist_skipping_unplayable_tracks() {
    let collection = parse_embed(PLAYLIST).unwrap();
    assert_eq!(collection.name, "Today’s Top Hits");
    let labels: Vec<String> = collection.tracks.iter().map(Track::label).collect();
    assert_eq!(
        labels,
        [
            "KAROL G, Judeline, rusowsky - BbY WOW",
            "Tate McRae - Housewife"
        ]
    );
}

#[test]
fn page_without_data_is_an_error() {
    assert!(parse_embed("<html></html>").is_err());
}

#[test]
fn match_prefers_the_original_recording() {
    let track = Track {
        title: "Never Gonna Give You Up".into(),
        artists: "Rick Astley".into(),
        duration: 213.5,
    };
    let candidates = [
        candidate("Never Gonna Give You Up (Live)", "Rick Astley", 214.0),
        candidate(
            "Rick Astley - Never Gonna Give You Up (Official Video)",
            "Rick Astley",
            212.0,
        ),
        candidate("Never Gonna Give You Up", "Rick Astley - Topic", 214.0),
        candidate("Never Gonna Give You Up 10 hours", "Someone", 36000.0),
    ];
    assert_eq!(best_match(&track, &candidates), Some(2));
    assert_eq!(best_match(&track, &candidates[3..]), None);
}

#[test]
fn match_keeps_a_variant_the_title_asks_for() {
    let track = Track {
        title: "Song (Live)".into(),
        artists: "Band".into(),
        duration: 200.0,
    };
    let candidates = [
        candidate("Band - Song", "Band", 200.0),
        candidate("Band - Song (Live)", "Band", 201.0),
    ];
    assert_eq!(best_match(&track, &candidates), Some(1));
}
