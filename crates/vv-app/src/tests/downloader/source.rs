use super::*;

#[test]
fn recognises_the_supported_sites() {
    let site = |text: &str| parse(text).map(|link| link.site);
    assert_eq!(
        site("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
        Some(Site::YouTube)
    );
    assert_eq!(
        site("https://youtu.be/dQw4w9WgXcQ?t=3"),
        Some(Site::YouTube)
    );
    assert_eq!(
        site("https://music.youtube.com/watch?v=x"),
        Some(Site::YouTube)
    );
    assert_eq!(
        site("https://www.instagram.com/reel/Cabc/"),
        Some(Site::Instagram)
    );
    assert_eq!(site("https://vm.tiktok.com/ZMabc/"), Some(Site::TikTok));
    assert_eq!(
        site("https://open.spotify.com/intl-it/track/4uLU6hMCjMI75M1A2tKUQC?si=1"),
        Some(Site::Spotify)
    );
    assert_eq!(site("https://vimeo.com/123"), Some(Site::Other));
}

#[test]
fn rejects_plain_text_and_lookalike_hosts() {
    assert_eq!(parse("hello world"), None);
    assert_eq!(parse("https://youtube.com/watch?v=a b"), None);
    assert_eq!(parse("ftp://youtube.com/x"), None);
    assert_eq!(parse("https://localhost/x"), None);
    assert_eq!(
        parse("https://notyoutube.com/watch").map(|l| l.site),
        Some(Site::Other)
    );
    assert_eq!(parse_known("https://notyoutube.com/watch"), None);
    // Artist pages and the like have no tracks to fetch.
    assert_eq!(
        parse("https://open.spotify.com/artist/0gxyHStUsqpMadRV0Di1Qt"),
        None
    );
}

#[test]
fn spotify_uris_become_web_links() {
    let link = parse("spotify:album:1ATL5GLyefJaxhQzSPVrLX").unwrap();
    assert_eq!(link.site, Site::Spotify);
    assert_eq!(
        link.url,
        "https://open.spotify.com/album/1ATL5GLyefJaxhQzSPVrLX"
    );
    assert_eq!(parse("spotify:artist:abc"), None);
}

#[test]
fn spotify_item_skips_locale_and_embed_segments() {
    assert_eq!(
        spotify_item("https://open.spotify.com/intl-it/embed/playlist/37i9dQZF1DXcBWIGoYBM5M?si=x"),
        Some((SpotifyKind::Playlist, "37i9dQZF1DXcBWIGoYBM5M".into()))
    );
    assert_eq!(spotify_item("https://open.spotify.com/track/"), None);
}
