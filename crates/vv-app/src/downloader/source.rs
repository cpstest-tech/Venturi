//! Which site a pasted link points to.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    YouTube,
    Instagram,
    TikTok,
    Spotify,
    /// Anything else yt-dlp may know.
    Other,
}

impl Site {
    pub fn label(self) -> &'static str {
        match self {
            Self::YouTube => "YouTube",
            Self::Instagram => "Instagram",
            Self::TikTok => "TikTok",
            Self::Spotify => "Spotify",
            Self::Other => "Web",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotifyKind {
    Track,
    Album,
    Playlist,
}

impl SpotifyKind {
    pub fn path(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Album => "album",
            Self::Playlist => "playlist",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    pub site: Site,
}

/// A downloadable link, or `None` for plain text. `spotify:` URIs become
/// their web URL.
pub fn parse(text: &str) -> Option<Link> {
    let text = text.trim();
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return None;
    }
    if let Some(rest) = text.strip_prefix("spotify:") {
        let (kind, id) = rest.split_once(':')?;
        spotify_kind(kind)?;
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        return Some(Link {
            url: format!("https://open.spotify.com/{kind}/{id}"),
            site: Site::Spotify,
        });
    }
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))?;
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .split(':')
        .next()?
        .to_ascii_lowercase();
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    let site = site_of(&host);
    if site == Site::Spotify && spotify_item(text).is_none() {
        return None;
    }
    Some(Link {
        url: text.to_owned(),
        site,
    })
}

/// Only the known sites: a pasted link to anything else stays a paste.
pub fn parse_known(text: &str) -> Option<Link> {
    parse(text).filter(|link| link.site != Site::Other)
}

fn site_of(host: &str) -> Site {
    let is = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if is("youtube.com") || is("youtu.be") || is("youtube-nocookie.com") {
        Site::YouTube
    } else if is("instagram.com") {
        Site::Instagram
    } else if is("tiktok.com") {
        Site::TikTok
    } else if host == "open.spotify.com" {
        Site::Spotify
    } else {
        Site::Other
    }
}

fn spotify_kind(segment: &str) -> Option<SpotifyKind> {
    match segment {
        "track" => Some(SpotifyKind::Track),
        "album" => Some(SpotifyKind::Album),
        "playlist" => Some(SpotifyKind::Playlist),
        _ => None,
    }
}

/// `open.spotify.com/[intl-xx/][embed/]<kind>/<id>`.
pub fn spotify_item(url: &str) -> Option<(SpotifyKind, String)> {
    let path = url.split_once("open.spotify.com/")?.1;
    let path = path.split(['?', '#']).next()?;
    let mut segments = path
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with("intl-") && *s != "embed");
    let kind = spotify_kind(segments.next()?)?;
    let id = segments.next()?;
    (id.chars().all(|c| c.is_ascii_alphanumeric()) && !id.is_empty()).then(|| (kind, id.to_owned()))
}

#[cfg(test)]
#[path = "../tests/downloader/source.rs"]
mod tests;
