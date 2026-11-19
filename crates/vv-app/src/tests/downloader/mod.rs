use super::*;

#[test]
fn clean_error_keeps_the_yt_dlp_error_line() {
    let dump = "Local process exited with exit status: 1: [youtube] abc: Downloading webpage\n\
                ERROR: [youtube] abc: Video unavailable\nmore";
    assert_eq!(clean_error(dump), "[youtube] abc: Video unavailable");
    assert_eq!(clean_error("Cancelled"), "Cancelled");
}

#[test]
fn prefs_fill_missing_fields_with_defaults() {
    let prefs: Prefs = serde_json::from_str(r#"{"max_height": 720}"#).unwrap();
    assert_eq!(prefs.max_height, 720);
    assert!(prefs.prefer_h264);
    assert_eq!(prefs.audio_format, AudioFormat::Original);
}

#[test]
fn a_cancelled_job_waiting_for_a_slot_gives_up() {
    let shared = Shared {
        jobs: Mutex::new(Vec::new()),
        running: Mutex::new(SLOTS),
        freed: Condvar::new(),
        ctx: egui::Context::default(),
    };
    let cancel = AtomicBool::new(true);
    assert_eq!(shared.acquire(&cancel).err().as_deref(), Some(CANCELLED));
    *shared.running.lock().unwrap() = 0;
    let guard = shared.acquire(&AtomicBool::new(false)).ok();
    assert_eq!(*shared.running.lock().unwrap(), 1);
    drop(guard);
    assert_eq!(*shared.running.lock().unwrap(), 0);
}

#[test]
fn cut_keeps_only_the_section_and_replaces_the_file() {
    use vv_media::test_support::{TempDir, ffmpeg};
    let dir = TempDir::new("vv-app-download-cut");
    let source = dir.join("Song [abc].m4a");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=10",
            "-c:a",
            "aac",
        ],
        &source,
    );
    let ffmpeg_path = ytdlp::on_path("ffmpeg").expect("ffmpeg CLI not found");
    let cut_file = cut(
        &ffmpeg_path,
        &source,
        (2.0, 5.0),
        MediaKind::Audio,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(cut_file, source);
    let meta = vv_media::probe(&cut_file).unwrap();
    let secs = meta.duration_frames as f64 * f64::from(meta.fps.den) / f64::from(meta.fps.num);
    assert!((secs - 3.0).abs() < 0.2, "{secs}");
    assert!(!dir.join("Song [abc].cut.m4a").exists());
}
