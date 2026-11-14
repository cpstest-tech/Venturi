//! The sticker catalog and its on-demand store. Only names are bundled
//! (`assets/stickers`); images are downloaded the first time they are shown
//! and kept on disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use crate::library_ribbon::StickerCategory;

const EMOJI: &str = include_str!("../assets/stickers/emoji.tsv");
const LUCIDE: &str = include_str!("../assets/stickers/lucide.txt");
const PHOSPHOR: &str = include_str!("../assets/stickers/phosphor.txt");

pub(crate) const EMOJI_GROUPS: usize = 9;
const WORKERS: usize = 4;

const NOTO_URL: &str = "https://raw.githubusercontent.com/googlefonts/noto-emoji/main/3D/png";
const LUCIDE_URL: &str = "https://raw.githubusercontent.com/lucide-icons/lucide/main/icons";
const PHOSPHOR_URL: &str = "https://raw.githubusercontent.com/phosphor-icons/core/main/assets/fill";

const SHAPE_COLORS: [(&str, &str); 10] = [
    ("red", "#ef4444"),
    ("orange", "#f97316"),
    ("yellow", "#facc15"),
    ("green", "#22c55e"),
    ("cyan", "#06b6d4"),
    ("blue", "#3b82f6"),
    ("purple", "#a855f7"),
    ("pink", "#ec4899"),
    ("white", "#ffffff"),
    ("black", "#111111"),
];

/// SVG bodies in a 100x100 box; `FILL` is the color.
const SHAPES: &[(&str, &str)] = &[
    ("circle", r#"<circle cx="50" cy="50" r="44" fill="FILL"/>"#),
    (
        "square",
        r#"<rect x="8" y="8" width="84" height="84" fill="FILL"/>"#,
    ),
    (
        "rounded square",
        r#"<rect x="8" y="8" width="84" height="84" rx="22" fill="FILL"/>"#,
    ),
    (
        "triangle",
        r#"<polygon points="50,12 91,84 9,84" fill="FILL" stroke="FILL" stroke-width="10" stroke-linejoin="round"/>"#,
    ),
    (
        "diamond",
        r#"<polygon points="50,8 92,50 50,92 8,50" fill="FILL" stroke="FILL" stroke-width="8" stroke-linejoin="round"/>"#,
    ),
    (
        "pentagon",
        r#"<polygon points="50,8 92,38 76,88 24,88 8,38" fill="FILL" stroke="FILL" stroke-width="8" stroke-linejoin="round"/>"#,
    ),
    (
        "hexagon",
        r#"<polygon points="50,6 90,28 90,72 50,94 10,72 10,28" fill="FILL" stroke="FILL" stroke-width="8" stroke-linejoin="round"/>"#,
    ),
    (
        "star",
        r#"<polygon points="50,6 62,36 94,38 69,58 78,90 50,72 22,90 31,58 6,38 38,36" fill="FILL" stroke="FILL" stroke-width="6" stroke-linejoin="round"/>"#,
    ),
    (
        "sparkle",
        r#"<path d="M50 4C54 34 66 46 96 50C66 54 54 66 50 96C46 66 34 54 4 50C34 46 46 34 50 4Z" fill="FILL"/>"#,
    ),
    (
        "burst",
        r#"<polygon points="50,4 58,20 74,10 76,28 94,28 86,44 98,56 82,64 88,82 70,80 66,96 52,86 40,98 32,82 14,86 18,68 2,58 16,46 8,30 26,28 28,10 44,18" fill="FILL" stroke="FILL" stroke-width="3" stroke-linejoin="round"/>"#,
    ),
    (
        "heart",
        r#"<path d="M50 90C20 66 6 48 6 30C6 16 17 8 29 8C38 8 46 13 50 21C54 13 62 8 71 8C83 8 94 16 94 30C94 48 80 66 50 90Z" fill="FILL"/>"#,
    ),
    (
        "cross",
        r#"<path d="M38 8H62V38H92V62H62V92H38V62H8V38H38Z" fill="FILL" stroke="FILL" stroke-width="6" stroke-linejoin="round"/>"#,
    ),
    (
        "ring",
        r#"<circle cx="50" cy="50" r="40" fill="none" stroke="FILL" stroke-width="14"/>"#,
    ),
    (
        "dashed ring",
        r#"<circle cx="50" cy="50" r="40" fill="none" stroke="FILL" stroke-width="12" stroke-dasharray="16 11" stroke-linecap="round"/>"#,
    ),
    (
        "frame",
        r#"<rect x="10" y="10" width="80" height="80" rx="14" fill="none" stroke="FILL" stroke-width="10"/>"#,
    ),
    (
        "moon",
        r#"<path d="M64 8A44 44 0 1 0 92 66A38 38 0 0 1 64 8Z" fill="FILL"/>"#,
    ),
    (
        "sun",
        r#"<circle cx="50" cy="50" r="20" fill="FILL"/><g stroke="FILL" stroke-width="8" stroke-linecap="round"><path d="M50 8V20M50 80V92M8 50H20M80 50H92M20 20L28 28M72 72L80 80M80 20L72 28M28 72L20 80"/></g>"#,
    ),
    (
        "lightning",
        r#"<polygon points="58,4 20,56 46,56 38,96 80,40 54,40" fill="FILL" stroke="FILL" stroke-width="6" stroke-linejoin="round"/>"#,
    ),
    (
        "drop",
        r#"<path d="M50 6C50 6 14 46 14 64A36 36 0 0 0 86 64C86 46 50 6 50 6Z" fill="FILL"/>"#,
    ),
    (
        "cloud",
        r#"<g fill="FILL"><circle cx="30" cy="62" r="20"/><circle cx="52" cy="44" r="26"/><circle cx="74" cy="60" r="22"/><rect x="30" y="58" width="44" height="24"/></g>"#,
    ),
    (
        "speech bubble",
        r#"<rect x="6" y="10" width="88" height="62" rx="22" fill="FILL"/><polygon points="22,66 20,92 52,68" fill="FILL"/>"#,
    ),
    (
        "arrow",
        r#"<polygon points="6,36 56,36 56,14 94,50 56,86 56,64 6,64" fill="FILL" stroke="FILL" stroke-width="4" stroke-linejoin="round"/>"#,
    ),
    (
        "wave",
        r#"<path d="M6 50Q20 24 34 50T62 50T90 50" fill="none" stroke="FILL" stroke-width="12" stroke-linecap="round"/>"#,
    ),
    (
        "check",
        r##"<circle cx="50" cy="50" r="44" fill="FILL"/><path d="M28 52L44 68L74 34" fill="none" stroke="#ffffff" stroke-width="11" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "cross mark",
        r##"<circle cx="50" cy="50" r="44" fill="FILL"/><path d="M33 33L67 67M67 33L33 67" fill="none" stroke="#ffffff" stroke-width="11" stroke-linecap="round"/>"##,
    ),
];

/// Phosphor icons that are not brand logos but belong to Social.
const SOCIAL_ICONS: &[&str] = &[
    "heart",
    "thumbs-up",
    "thumbs-down",
    "chat-circle",
    "chat-teardrop",
    "chats",
    "share-network",
    "share-fat",
    "bell",
    "bell-ringing",
    "bookmark-simple",
    "star",
    "fire",
    "eye",
    "user-plus",
    "users",
    "hand-waving",
    "hand-heart",
    "hash",
    "at",
    "play-circle",
    "video-camera",
    "camera",
    "music-notes",
    "microphone",
    "trophy",
    "crown",
    "gift",
    "confetti",
    "smiley",
    "megaphone",
];

const ARROW_PREFIXES: &[&str] = &[
    "arrow", "caret", "chevron", "corner", "move", "undo", "redo", "refresh",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    Noto { code: String, group: u8 },
    Lucide(String),
    Phosphor(String),
    Shape { shape: usize, color: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sticker {
    /// Stable key: used for favorites and drags.
    pub id: String,
    pub name: String,
    pub category: StickerCategory,
    pub source: Source,
    search: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Size {
    Thumb,
    Full,
}

impl Size {
    fn pixels(self) -> u32 {
        match self {
            Self::Thumb => 128,
            Self::Full => 512,
        }
    }

    fn dir(self) -> &'static str {
        match self {
            Self::Thumb => "thumb",
            Self::Full => "full",
        }
    }
}

impl Sticker {
    fn new(id: String, name: String, category: StickerCategory, source: Source) -> Self {
        let search = format!("{} {}", id, name).to_lowercase();
        Self {
            id,
            name,
            category,
            source,
            search,
        }
    }

    /// The file name stem of the full-size image, also the name of the
    /// library asset made from it.
    pub fn file_stem(&self) -> String {
        self.id.replace(':', "-")
    }

    pub fn matches(&self, query: &str) -> bool {
        query
            .split_whitespace()
            .all(|word| self.search.contains(&word.to_lowercase()))
    }

    fn halo(&self) -> bool {
        match &self.source {
            Source::Shape { color, .. } => SHAPE_COLORS[*color].0 == "white",
            Source::Lucide(_) | Source::Phosphor(_) => true,
            Source::Noto { .. } => false,
        }
    }

    fn url(&self, size: Size) -> Option<String> {
        match &self.source {
            Source::Noto { code, .. } => {
                Some(format!("{NOTO_URL}/{}/emoji_u{code}.png", size.pixels()))
            }
            Source::Lucide(name) => Some(format!("{LUCIDE_URL}/{name}.svg")),
            Source::Phosphor(name) => Some(format!("{PHOSPHOR_URL}/{name}-fill.svg")),
            Source::Shape { .. } => None,
        }
    }
}

fn humanize(slug: &str) -> String {
    let mut text = slug.replace(['-', '_'], " ");
    if let Some(first) = text.get(..1) {
        let upper = first.to_uppercase();
        text.replace_range(..1, &upper);
    }
    text
}

fn icon_category(name: &str, lucide: bool) -> StickerCategory {
    if name.ends_with("-logo") || SOCIAL_ICONS.contains(&name) {
        return StickerCategory::Social;
    }
    let first = name.split('-').next().unwrap_or(name);
    let arrowish = ARROW_PREFIXES.iter().any(|p| first.starts_with(p));
    if arrowish && !(lucide && name.starts_with("move-") && name.len() > 20) {
        StickerCategory::Arrows
    } else {
        StickerCategory::Other
    }
}

pub(crate) fn all() -> &'static [Sticker] {
    static ALL: OnceLock<Vec<Sticker>> = OnceLock::new();
    ALL.get_or_init(build)
}

pub(crate) fn find(id: &str) -> Option<&'static Sticker> {
    all().iter().find(|s| s.id == id)
}

fn build() -> Vec<Sticker> {
    let mut out = Vec::new();
    for (shape, (name, _)) in SHAPES.iter().enumerate() {
        for (color, (color_name, _)) in SHAPE_COLORS.iter().enumerate() {
            out.push(Sticker::new(
                format!("shape:{}:{color_name}", name.replace(' ', "-")),
                humanize(&format!("{name} {color_name}")),
                StickerCategory::Shapes,
                Source::Shape { shape, color },
            ));
        }
    }
    for line in EMOJI.lines() {
        let mut parts = line.split('\t');
        if let (Some(code), Some(name), Some(group)) = (parts.next(), parts.next(), parts.next()) {
            out.push(Sticker::new(
                format!("noto:{code}"),
                humanize(name),
                StickerCategory::Emoji,
                Source::Noto {
                    code: code.to_owned(),
                    group: group.parse().unwrap_or(0),
                },
            ));
        }
    }
    for name in PHOSPHOR.lines().filter(|l| !l.is_empty()) {
        out.push(Sticker::new(
            format!("phosphor:{name}"),
            humanize(name),
            icon_category(name, false),
            Source::Phosphor(name.to_owned()),
        ));
    }
    for name in LUCIDE.lines().filter(|l| !l.is_empty()) {
        out.push(Sticker::new(
            format!("lucide:{name}"),
            humanize(name),
            icon_category(name, true),
            Source::Lucide(name.to_owned()),
        ));
    }
    out
}

fn shape_svg(shape: usize, color: usize) -> String {
    let body = SHAPES[shape].1.replace("FILL", SHAPE_COLORS[color].1);
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{body}</svg>"#)
}

/// A white glyph from an `currentColor` icon: the `color` attribute is what
/// `currentColor` resolves to.
fn tint_icon(svg: &str) -> String {
    svg.replacen("<svg", r##"<svg color="#ffffff""##, 1)
        .replace(r#"stroke-width="2""#, r#"stroke-width="2.5""#)
}

/// Renders `svg` into a `size` square PNG; icons get a dark halo so white
/// glyphs stay visible on bright footage.
pub(crate) fn rasterize(svg: &str, size: u32, halo: bool) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let margin = if halo { size / 32 + 1 } else { size / 64 };
    let inner = (size - 2 * margin) as f32;
    let bounds = tree.size();
    let scale = inner / bounds.width().max(bounds.height());
    let dx = margin as f32 + (inner - bounds.width() * scale) / 2.0;
    let dy = margin as f32 + (inner - bounds.height() * scale) / 2.0;
    let mut pixmap = tiny_skia::Pixmap::new(size, size).ok_or("empty size")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    if halo {
        add_halo(&mut pixmap, margin as usize);
    }
    pixmap.encode_png().map_err(|e| e.to_string())
}

/// Black shadow under the glyph, `radius` pixels wide.
fn add_halo(pixmap: &mut resvg::tiny_skia::Pixmap, radius: usize) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let alpha: Vec<u8> = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[3])
        .collect();
    let spread = |src: &[u8], horizontal: bool| {
        let mut out = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let (lo, hi, fixed) = if horizontal {
                    (x.saturating_sub(radius), (x + radius).min(w - 1), y)
                } else {
                    (y.saturating_sub(radius), (y + radius).min(h - 1), x)
                };
                out[y * w + x] = (lo..=hi)
                    .map(|i| {
                        if horizontal {
                            src[fixed * w + i]
                        } else {
                            src[i * w + fixed]
                        }
                    })
                    .max()
                    .unwrap_or(0);
            }
        }
        out
    };
    let shadow = spread(&spread(&alpha, true), false);
    for (pixel, shadow) in pixmap
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(shadow)
    {
        let cover = u32::from(shadow) * 140 / 255;
        let behind = cover * (255 - u32::from(pixel[3])) / 255;
        pixel[3] = (u32::from(pixel[3]) + behind).min(255) as u8;
    }
}

fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    response.body_mut().read_to_vec().map_err(|e| e.to_string())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let part = path.with_extension("part");
    std::fs::write(&part, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

fn svg_text(dir: &Path, sticker: &Sticker) -> Result<String, String> {
    if let Source::Shape { shape, color } = sticker.source {
        return Ok(shape_svg(shape, color));
    }
    let cached = dir.join("svg").join(format!("{}.svg", sticker.file_stem()));
    if let Ok(text) = std::fs::read_to_string(&cached) {
        return Ok(tint_icon(&text));
    }
    let url = sticker.url(Size::Full).ok_or("no source")?;
    let bytes = http_get(&url)?;
    write_atomic(&cached, &bytes)?;
    Ok(tint_icon(&String::from_utf8_lossy(&bytes)))
}

fn produce(dir: &Path, sticker: &Sticker, size: Size) -> Result<(), String> {
    let target = image_path(dir, sticker, size);
    if target.is_file() {
        return Ok(());
    }
    let png = match &sticker.source {
        Source::Noto { .. } => {
            let url = sticker.url(size).ok_or("no source")?;
            http_get(&url)?
        }
        _ => rasterize(&svg_text(dir, sticker)?, size.pixels(), sticker.halo())?,
    };
    write_atomic(&target, &png)
}

pub(crate) fn image_path(dir: &Path, sticker: &Sticker, size: Size) -> PathBuf {
    dir.join(size.dir())
        .join(format!("{}.png", sticker.file_stem()))
}

pub(crate) fn default_dir() -> Option<PathBuf> {
    let library = crate::library::Library::default_root()?;
    Some(library.parent()?.join("stickers"))
}

struct Job {
    sticker: &'static Sticker,
    size: Size,
    generation: u64,
}

pub(crate) struct Done {
    pub id: String,
    pub size: Size,
    /// `None`: dropped from the queue because the view moved on.
    pub result: Option<Result<(), String>>,
}

/// Downloads and renders stickers in background threads.
pub(crate) struct Store {
    dir: PathBuf,
    jobs: mpsc::Sender<Job>,
    done: mpsc::Receiver<Done>,
    generation: Arc<AtomicU64>,
    pending: HashSet<(String, Size)>,
    ready: HashSet<(String, Size)>,
    failed: HashSet<String>,
}

impl Store {
    pub fn new(dir: PathBuf, ctx: &egui::Context) -> Self {
        let (jobs, job_rx) = mpsc::channel::<Job>();
        let (done_tx, done) = mpsc::channel();
        let job_rx = Arc::new(Mutex::new(job_rx));
        let generation = Arc::new(AtomicU64::new(0));
        for _ in 0..WORKERS {
            let (job_rx, done_tx) = (job_rx.clone(), done_tx.clone());
            let (dir, generation, ctx) = (dir.clone(), generation.clone(), ctx.clone());
            std::thread::spawn(move || {
                loop {
                    let job = match job_rx.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => return,
                    };
                    let Ok(job) = job else { return };
                    let stale = job.size == Size::Thumb
                        && job.generation != generation.load(Ordering::Relaxed);
                    let result = (!stale).then(|| produce(&dir, job.sticker, job.size));
                    let sent = done_tx.send(Done {
                        id: job.sticker.id.clone(),
                        size: job.size,
                        result,
                    });
                    if sent.is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Self {
            dir,
            jobs,
            done,
            generation,
            pending: HashSet::new(),
            ready: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    pub fn path(&self, sticker: &Sticker, size: Size) -> PathBuf {
        image_path(&self.dir, sticker, size)
    }

    /// Whether the image is on disk; otherwise queues it (once).
    pub fn ensure(&mut self, sticker: &'static Sticker, size: Size) -> bool {
        let key = (sticker.id.clone(), size);
        if self.ready.contains(&key) {
            return true;
        }
        if self.path(sticker, size).is_file() {
            self.ready.insert(key);
            return true;
        }
        if !self.pending.contains(&key) && !self.failed.contains(&sticker.id) {
            self.pending.insert(key);
            let _ = self.jobs.send(Job {
                sticker,
                size,
                generation: self.generation.load(Ordering::Relaxed),
            });
        }
        false
    }

    pub fn failed(&self, id: &str) -> bool {
        self.failed.contains(id)
    }

    pub fn retry(&mut self, id: &str) {
        self.failed.remove(id);
    }

    /// Thumbnails still queued are skipped from now on.
    pub fn new_view(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Finished images, as `(id, size)`.
    pub fn poll(&mut self) -> Vec<(String, Size)> {
        let mut finished = Vec::new();
        while let Ok(done) = self.done.try_recv() {
            self.pending.remove(&(done.id.clone(), done.size));
            match done.result {
                Some(Ok(())) => {
                    self.ready.insert((done.id.clone(), done.size));
                    finished.push((done.id, done.size));
                }
                Some(Err(e)) => {
                    eprintln!("[stickers] {}: {e}", done.id);
                    self.failed.insert(done.id);
                }
                None => {}
            }
        }
        finished
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[cfg(test)]
#[path = "tests/stickers.rs"]
mod tests;
