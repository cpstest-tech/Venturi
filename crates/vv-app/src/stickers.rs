//! The sticker catalog and its on-demand store. Only names are bundled
//! (`assets/stickers`); images are downloaded the first time they are shown
//! and kept on disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use crate::library_ribbon::StickerCategory;
use crate::sticker_anim;

const EMOJI: &str = include_str!("../assets/stickers/emoji.tsv");
const LUCIDE: &str = include_str!("../assets/stickers/lucide.txt");
const PHOSPHOR: &str = include_str!("../assets/stickers/phosphor.txt");

pub(crate) const EMOJI_GROUPS: usize = 9;
const WORKERS: usize = 4;

const NOTO_ANIMATED_URL: &str = "https://fonts.gstatic.com/s/e/notoemoji/latest";
const NOTO_URL: &str = "https://raw.githubusercontent.com/googlefonts/noto-emoji/main/3D/png";
const LUCIDE_URL: &str = "https://raw.githubusercontent.com/lucide-icons/lucide/main/icons";
const PHOSPHOR_URL: &str = "https://raw.githubusercontent.com/phosphor-icons/core/main/assets/fill";

const SHAPE_COLOR: &str = "#ffffff";

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
    Noto {
        code: String,
        group: u8,
        animated: bool,
    },
    Lucide(String),
    Phosphor(String),
    Shape(usize),
    /// Index in `sticker_anim::DEFS`.
    Anim(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sticker {
    /// Stable key: used for favorites and drags.
    pub id: String,
    pub name: String,
    pub category: StickerCategory,
    pub source: Source,
    pub animated: bool,
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
        let animated = matches!(
            source,
            Source::Anim(_) | Source::Noto { animated: true, .. }
        );
        let search =
            format!("{} {} {}", id, name, if animated { "animated" } else { "" }).to_lowercase();
        Self {
            id,
            name,
            category,
            source,
            animated,
            search,
        }
    }

    /// Extension of the full-size file: animated stickers are videos.
    pub fn full_extension(&self) -> &'static str {
        if self.animated { "webm" } else { "png" }
    }

    /// Name of the library asset made from the full-size file.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.file_stem(), self.full_extension())
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
            Source::Noto { .. } => false,
            Source::Anim(index) => !sticker_anim::DEFS[*index].colored(),
            _ => true,
        }
    }

    fn url(&self, size: Size) -> Option<String> {
        match &self.source {
            Source::Noto { code, .. } => {
                Some(format!("{NOTO_URL}/{}/emoji_u{code}.png", size.pixels()))
            }
            Source::Lucide(name) => Some(format!("{LUCIDE_URL}/{name}.svg")),
            Source::Phosphor(name) => Some(format!("{PHOSPHOR_URL}/{name}-fill.svg")),
            Source::Shape(_) | Source::Anim(_) => None,
        }
    }

    fn animated_noto_url(&self) -> Option<String> {
        match &self.source {
            Source::Noto {
                code,
                animated: true,
                ..
            } => Some(format!("{NOTO_ANIMATED_URL}/{code}/512.webp")),
            _ => None,
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

/// Whether the media is a catalog sticker drawn in white to be recolored.
pub(crate) fn is_tintable_path(path: &Path) -> bool {
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    !stem.starts_with("anim-pastel-")
        && ["shape-", "lucide-", "phosphor-", "anim-"]
            .iter()
            .any(|prefix| stem.starts_with(prefix))
}

pub(crate) fn find(id: &str) -> Option<&'static Sticker> {
    all().iter().find(|s| s.id == id)
}

fn build() -> Vec<Sticker> {
    let mut out = Vec::new();
    for (shape, (name, _)) in SHAPES.iter().enumerate() {
        out.push(Sticker::new(
            format!("shape:{}", name.replace(' ', "-")),
            humanize(name),
            StickerCategory::Shapes,
            Source::Shape(shape),
        ));
    }
    for (index, def) in sticker_anim::DEFS.iter().enumerate() {
        out.push(Sticker::new(
            format!("anim:{}", def.slug),
            def.name.to_owned(),
            def.category,
            Source::Anim(index),
        ));
    }
    for line in EMOJI.lines() {
        let mut parts = line.split('\t');
        if let (Some(code), Some(name), Some(group), Some(animated)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        {
            out.push(Sticker::new(
                format!("noto:{code}"),
                humanize(name),
                StickerCategory::Emoji,
                Source::Noto {
                    code: code.to_owned(),
                    group: group.parse().unwrap_or(0),
                    animated: animated == "1",
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

fn shape_svg(shape: usize) -> String {
    let body = SHAPES[shape].1.replace("FILL", SHAPE_COLOR);
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
    render_pixmap(svg, size, halo)?
        .encode_png()
        .map_err(|e| e.to_string())
}

/// The same as straight (not premultiplied) RGBA, for the video encoder.
pub(crate) fn render_rgba(svg: &str, size: u32, halo: bool) -> Result<Vec<u8>, String> {
    let pixmap = render_pixmap(svg, size, halo)?;
    let mut rgba = Vec::with_capacity(pixmap.pixels().len() * 4);
    for pixel in pixmap.pixels() {
        let c = pixel.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(rgba)
}

fn render_pixmap(svg: &str, size: u32, halo: bool) -> Result<resvg::tiny_skia::Pixmap, String> {
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
    Ok(pixmap)
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

fn phosphor_svg(dir: &Path, name: &str) -> Result<String, String> {
    let cached = dir.join("svg").join(format!("phosphor-{name}.svg"));
    if let Ok(text) = std::fs::read_to_string(&cached) {
        return Ok(text);
    }
    let bytes = http_get(&format!("{PHOSPHOR_URL}/{name}-fill.svg"))?;
    write_atomic(&cached, &bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn svg_text(dir: &Path, sticker: &Sticker) -> Result<String, String> {
    match &sticker.source {
        Source::Shape(shape) => Ok(shape_svg(*shape)),
        Source::Phosphor(name) => phosphor_svg(dir, name).map(|svg| tint_icon(&svg)),
        Source::Lucide(name) => {
            let cached = dir.join("svg").join(format!("{}.svg", sticker.file_stem()));
            if let Ok(text) = std::fs::read_to_string(&cached) {
                return Ok(tint_icon(&text));
            }
            let bytes = http_get(&format!("{LUCIDE_URL}/{name}.svg"))?;
            write_atomic(&cached, &bytes)?;
            Ok(tint_icon(&String::from_utf8_lossy(&bytes)))
        }
        _ => Err("no svg".to_owned()),
    }
}

/// Minimum length of an animated sticker, so that it can be stretched on the
/// timeline beyond its default length.
const ANIMATED_MIN_SECS: f64 = 6.0;
const ANIMATED_SIZE: u32 = 512;

fn encode_error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The frames of one loop, rendered on all cores.
fn render_loop(
    def: &sticker_anim::AnimDef,
    icon: Option<&str>,
    frames: usize,
) -> Result<Vec<Vec<u8>>, String> {
    let threads = std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .min(frames.max(1));
    let render = |i: usize| {
        let svg = sticker_anim::frame(def, icon, i as f32 / frames as f32);
        render_rgba(&svg, ANIMATED_SIZE, !def.colored())
    };
    let mut out: Vec<Result<Vec<u8>, String>> = Vec::with_capacity(frames);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|k| {
                let render = &render;
                scope.spawn(move || {
                    (k..frames)
                        .step_by(threads)
                        .map(|i| (i, render(i)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut indexed: Vec<_> = workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect();
        indexed.sort_by_key(|(i, _)| *i);
        out.extend(indexed.into_iter().map(|(_, r)| r));
    });
    if out.len() != frames {
        return Err("a render thread failed".to_owned());
    }
    out.into_iter().collect()
}

fn produce_animated(dir: &Path, sticker: &Sticker, target: &Path) -> Result<(), String> {
    let part = target.with_extension("part.webm");
    if let Some(parent) = part.parent() {
        std::fs::create_dir_all(parent).map_err(encode_error)?;
    }
    let result = match &sticker.source {
        Source::Noto { .. } => {
            let url = sticker.animated_noto_url().ok_or("no animation")?;
            let source = dir
                .join("src")
                .join(format!("{}.webp", sticker.file_stem()));
            write_atomic(&source, &http_get(&url)?)?;
            let animation = vv_media::anim::decode_animation(&source).map_err(encode_error)?;
            let order = vv_media::anim::loop_indices(
                &animation.delays,
                sticker_anim::FRAME_RATE,
                ANIMATED_MIN_SECS,
            );
            let frames = order.into_iter().map(|i| animation.frames[i].clone());
            vv_media::anim::encode_alpha_webm(
                &part,
                animation.width,
                animation.height,
                sticker_anim::FRAME_RATE,
                frames,
            )
            .map_err(encode_error)
        }
        Source::Anim(index) => {
            let def = &sticker_anim::DEFS[*index];
            let icon = match def.body {
                sticker_anim::Body::Icon(name, _) => Some(phosphor_svg(dir, name)?),
                sticker_anim::Body::Draw(_) => None,
            };
            let per_loop =
                (f64::from(def.secs) * f64::from(sticker_anim::FRAME_RATE)).round() as usize;
            let loops = (ANIMATED_MIN_SECS / f64::from(def.secs)).ceil() as usize;
            let one_loop = render_loop(def, icon.as_deref(), per_loop)?;
            let frames = (0..loops).flat_map(|_| one_loop.iter().cloned());
            vv_media::anim::encode_alpha_webm(
                &part,
                ANIMATED_SIZE,
                ANIMATED_SIZE,
                sticker_anim::FRAME_RATE,
                frames,
            )
            .map_err(encode_error)
        }
        _ => Err("not animated".to_owned()),
    };
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, target).map_err(encode_error)
}

fn produce(dir: &Path, sticker: &Sticker, size: Size) -> Result<(), String> {
    let target = image_path(dir, sticker, size);
    if target.is_file() {
        return Ok(());
    }
    if sticker.animated && size == Size::Full {
        return produce_animated(dir, sticker, &target);
    }
    let png = match &sticker.source {
        Source::Noto { .. } => {
            let url = sticker.url(size).ok_or("no source")?;
            http_get(&url)?
        }
        Source::Anim(index) => {
            let def = &sticker_anim::DEFS[*index];
            let icon = match def.body {
                sticker_anim::Body::Icon(name, _) => Some(phosphor_svg(dir, name)?),
                sticker_anim::Body::Draw(_) => None,
            };
            rasterize(
                &sticker_anim::frame(def, icon.as_deref(), def.still),
                size.pixels(),
                sticker.halo(),
            )?
        }
        _ => rasterize(&svg_text(dir, sticker)?, size.pixels(), sticker.halo())?,
    };
    write_atomic(&target, &png)
}

/// Whether an imported sticker file needs converting first.
pub(crate) fn needs_conversion(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["svg", "gif", "webp"]
            .iter()
            .any(|x| x.eq_ignore_ascii_case(e))
    })
}

const IMPORT_SVG_SIZE: u32 = 1024;

/// Turns a user's SVG into a PNG and an animated GIF or WebP into a looping
/// WebM, under `dir/imports`. A still WebP is returned as it is.
pub(crate) fn convert_for_import(src: &Path, dir: &Path) -> Result<PathBuf, String> {
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sticker");
    let extension = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let out = |ext: &str| dir.join("imports").join(format!("{stem}.{ext}"));
    if extension == "svg" {
        let text = std::fs::read_to_string(src).map_err(encode_error)?;
        let png = rasterize(&text, IMPORT_SVG_SIZE, false)?;
        let target = out("png");
        write_atomic(&target, &png)?;
        return Ok(target);
    }
    let animation = vv_media::anim::decode_animation(src).map_err(encode_error)?;
    if animation.frames.len() < 2 {
        return if extension == "webp" {
            Ok(src.to_path_buf())
        } else {
            let png = {
                let mut bytes = Vec::new();
                let mut encoder = png::Encoder::new(&mut bytes, animation.width, animation.height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header().map_err(encode_error)?;
                writer
                    .write_image_data(&animation.frames[0])
                    .map_err(encode_error)?;
                writer.finish().map_err(encode_error)?;
                bytes
            };
            let target = out("png");
            write_atomic(&target, &png)?;
            Ok(target)
        };
    }
    let order = vv_media::anim::loop_indices(
        &animation.delays,
        sticker_anim::FRAME_RATE,
        ANIMATED_MIN_SECS,
    );
    let frames = order.into_iter().map(|i| animation.frames[i].clone());
    let target = out("webm");
    let part = target.with_extension("part.webm");
    if let Some(parent) = part.parent() {
        std::fs::create_dir_all(parent).map_err(encode_error)?;
    }
    vv_media::anim::encode_alpha_webm(
        &part,
        animation.width,
        animation.height,
        sticker_anim::FRAME_RATE,
        frames,
    )
    .map_err(encode_error)?;
    std::fs::rename(&part, &target).map_err(encode_error)?;
    Ok(target)
}

pub(crate) fn image_path(dir: &Path, sticker: &Sticker, size: Size) -> PathBuf {
    let extension = match size {
        Size::Thumb => "png",
        Size::Full => sticker.full_extension(),
    };
    dir.join(size.dir())
        .join(format!("{}.{extension}", sticker.file_stem()))
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

    /// Whether the image is on disk, without fetching it.
    pub fn has(&mut self, sticker: &Sticker, size: Size) -> bool {
        let key = (sticker.id.clone(), size);
        if self.ready.contains(&key) {
            return true;
        }
        let present = self.path(sticker, size).is_file();
        if present {
            self.ready.insert(key);
        }
        present
    }

    pub fn is_pending(&self, id: &str, size: Size) -> bool {
        self.pending.contains(&(id.to_owned(), size))
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
