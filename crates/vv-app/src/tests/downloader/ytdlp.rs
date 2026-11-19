use super::*;
use crate::downloader::Destination;

fn request(kind: MediaKind) -> Request {
    Request {
        url: "https://youtu.be/abc".into(),
        title: "Song".into(),
        kind,
        audio_format: AudioFormat::Wav,
        max_height: 720,
        prefer_h264: true,
        range: None,
        destination: Destination::Music,
        spotify: None,
        epoch: 0,
    }
}

fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
    args.windows(2).any(|w| w[0] == flag && w[1] == value)
}

#[test]
fn checksum_lookup_matches_the_exact_asset() {
    let sums = "aa  yt-dlp\nBB  yt-dlp_linux\ncc  yt-dlp_linux.zip\n";
    assert_eq!(checksum_for(sums, "yt-dlp_linux").as_deref(), Some("bb"));
    assert_eq!(checksum_for(sums, "yt-dlp_macos"), None);
}

#[test]
fn audio_download_extracts_to_the_chosen_format() {
    let args = download_args(
        &Env::default(),
        &request(MediaKind::Audio),
        "U",
        Path::new("/out"),
        true,
    );
    assert!(has_pair(&args, "--audio-format", "wav"));
    assert!(args.contains(&"-x".to_owned()));
    assert!(has_pair(&args, "-P", "/out"));
    assert_eq!(args[args.len() - 2..], ["--".to_owned(), "U".to_owned()]);
    assert!(!args.iter().any(|a| a == "--download-sections"));
}

#[test]
fn original_audio_is_not_transcoded() {
    let mut req = request(MediaKind::Audio);
    req.audio_format = AudioFormat::Original;
    let args = download_args(&Env::default(), &req, "U", Path::new("/out"), true);
    assert!(!args.iter().any(|a| a == "--audio-format"));
}

#[test]
fn video_download_caps_height_and_prefers_h264() {
    let args = download_args(
        &Env::default(),
        &request(MediaKind::Video),
        "U",
        Path::new("/o"),
        true,
    );
    assert!(has_pair(&args, "-S", "res:720,vcodec:h264,acodec:aac"));
    assert!(has_pair(&args, "--merge-output-format", "mp4/mkv"));
    let mut req = request(MediaKind::Video);
    req.prefer_h264 = false;
    let args = download_args(&Env::default(), &req, "U", Path::new("/o"), true);
    assert!(has_pair(&args, "-S", "res:720,vcodec:vp9,acodec:aac"));
}

#[test]
fn a_range_downloads_only_that_section_with_exact_video_cuts() {
    let mut req = request(MediaKind::Video);
    req.range = Some((72.0, 91.5));
    let args = download_args(&Env::default(), &req, "U", Path::new("/o"), true);
    assert!(has_pair(&args, "--download-sections", "*72.000-91.500"));
    assert!(args.contains(&"--force-keyframes-at-cuts".to_owned()));
    assert!(has_pair(
        &args,
        "-o",
        "%(title).80B 1m12s-1m32s [%(id)s].%(ext)s"
    ));
    req.kind = MediaKind::Audio;
    let args = download_args(&Env::default(), &req, "U", Path::new("/o"), true);
    assert!(!args.contains(&"--force-keyframes-at-cuts".to_owned()));
    // Cut locally afterwards: the name still tells the section apart.
    let args = download_args(&Env::default(), &req, "U", Path::new("/o"), false);
    assert!(!args.iter().any(|a| a == "--download-sections"));
    assert!(has_pair(
        &args,
        "-o",
        "%(title).80B 1m12s-1m32s [%(id)s].%(ext)s"
    ));
}

#[test]
fn https_support_is_read_from_the_input_protocols() {
    let with = "Supported file protocols:\nInput:\n  file\n  https\nOutput:\n  file\n";
    let without = "Supported file protocols:\nInput:\n  file\n  http\nOutput:\n  https\n";
    assert!(reads_https(with));
    assert!(!reads_https(without));
}

#[test]
fn env_adds_ffmpeg_node_and_cookies() {
    let env = Env {
        ffmpeg: Some("/app/ffmpeg".into()),
        node: Some("/n/node".into()),
        cookies_browser: Some("firefox".into()),
    };
    let args = probe_args(&env, "U");
    assert!(has_pair(&args, "--ffmpeg-location", "/app/ffmpeg"));
    assert!(has_pair(&args, "--js-runtimes", "node:/n/node"));
    assert!(has_pair(&args, "--cookies-from-browser", "firefox"));
    let bare = probe_args(&Env::default(), "U");
    assert!(
        !bare
            .iter()
            .any(|a| a == "--cookies-from-browser" || a == "--js-runtimes")
    );
}

#[test]
fn parses_progress_lines() {
    assert_eq!(
        parse_line("VVDL 500 1000 NA 2048.5 3"),
        Some(Line::Progress {
            fraction: Some(0.5),
            speed: Some(2048.5),
            eta: Some(3.0)
        })
    );
    assert_eq!(
        parse_line("VVDL 250 NA 1000 NA NA"),
        Some(Line::Progress {
            fraction: Some(0.25),
            speed: None,
            eta: None
        })
    );
    assert_eq!(
        parse_line("VVDL NA NA NA NA NA"),
        Some(Line::Progress {
            fraction: None,
            speed: None,
            eta: None
        })
    );
    assert_eq!(
        parse_line("VVPP ExtractAudio"),
        Some(Line::PostProcess("ExtractAudio".into()))
    );
    assert_eq!(
        parse_line("VVFILE /tmp/a b [x].opus"),
        Some(Line::File("/tmp/a b [x].opus".into()))
    );
    assert_eq!(
        parse_line("ERROR: [tiktok] 1: Unable to extract"),
        Some(Line::Error("[tiktok] 1: Unable to extract".into()))
    );
    assert_eq!(parse_line("[download] Destination: x"), None);
}

#[test]
fn split_lines_handles_carriage_returns() {
    let mut lines = Vec::new();
    split_lines(&b"a\r\nb\rc\nlast"[..], |line| {
        lines.push(line);
        true
    });
    assert_eq!(lines, ["a", "b", "c", "last"]);
}

#[test]
fn parses_probe_output() {
    let output = r#"{"id": "abc", "title": "Track", "uploader": null, "channel": "Chan", "duration": 222.5, "thumbnail": "https://i.ytimg.com/vi/abc/maxresdefault.webp", "chapters": [{"start_time": 0.0, "end_time": 60.0, "title": "Intro"}, {"start_time": 60.0, "end_time": 222.5, "title": "Drop"}], "heatmap": [{"start_time": 0.0, "end_time": 2.2, "value": 1.0}]}"#;
    let info = parse_info(output).unwrap();
    assert_eq!(info.title, "Track");
    assert_eq!(info.uploader, "Chan");
    assert_eq!(info.duration, Some(222.5));
    assert_eq!(info.chapters[1].title, "Drop");
    assert_eq!(info.heatmap, vec![(0.0, 2.2, 1.0)]);
    assert_eq!(info.items, 1);
    let carousel = format!("{output}\n{output}\n");
    assert_eq!(parse_info(&carousel).unwrap().items, 2);
    assert!(parse_info("").is_err());
}

#[test]
fn parses_search_results() {
    let output = r#"{"_type": "playlist", "entries": [{"id": "a1", "url": "https://www.youtube.com/watch?v=a1", "title": "Song", "channel": "Band - Topic", "duration": 200.0}, {"id": "b2", "title": "Other", "uploader": "U", "duration": null}]}"#;
    let found = parse_search(output).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].channel, "Band - Topic");
    assert_eq!(found[1].url, "https://www.youtube.com/watch?v=b2");
    assert_eq!(found[1].duration, None);
}
