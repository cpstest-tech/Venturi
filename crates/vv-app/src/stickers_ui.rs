//! The Sticker tab: a grid of the catalog (shapes, emoji, arrows, social,
//! icons) and of the user's own images. Catalog images are downloaded when
//! their tile is first shown, and copied into the library only when used.

use super::*;
use crate::library_ribbon::{
    self, Category, StickerCategory, asset_key, hover_buttons, is_favorite,
};
use crate::pool_view::{self, ItemsDrag, PoolEntry, Thumb};
use crate::stickers::{self, Size, Sticker, Store};
use std::collections::HashSet;

/// Hidden library folder holding the catalog stickers that were used.
pub(crate) const CATALOG_FOLDER: &str = "ribbon-sticker-catalog";
const MIN_TILE: f32 = 78.0;
const GAP: f32 = 6.0;
/// Decoding is cheap, but not 200 at once.
const TEXTURES_PER_FRAME: usize = 12;

#[derive(Default)]
pub(crate) struct StickerState {
    pub(crate) store: Option<Store>,
    textures: HashMap<String, egui::TextureHandle>,
    search: String,
    emoji_group: Option<u8>,
    view: Option<Category>,
    /// Catalog ids whose "+" waits for the image to reach the library.
    wants: Vec<String>,
    imported: HashSet<String>,
    previews: PreviewCache,
    /// Imported files being converted in the background.
    pub(crate) conversions: Vec<Conversion>,
}

/// Tiles of animated stickers that play, once the sticker is downloaded.
const PREVIEW_PIXELS: u32 = 96;
const MAX_PREVIEWS: usize = 40;

struct Playing {
    frames: Vec<egui::ColorImage>,
    texture: egui::TextureHandle,
    fps: f64,
    shown: usize,
    last_used: f64,
}

struct PreviewCache {
    playing: HashMap<String, Playing>,
    loading: HashSet<String>,
    jobs: std::sync::mpsc::Sender<(String, PathBuf)>,
    done: std::sync::mpsc::Receiver<(String, Result<vv_media::anim::Preview, String>)>,
}

impl Default for PreviewCache {
    fn default() -> Self {
        let (jobs, job_rx) = std::sync::mpsc::channel::<(String, PathBuf)>();
        let (done_tx, done) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((id, path)) = job_rx.recv() {
                let result = vv_media::anim::decode_preview(&path, PREVIEW_PIXELS)
                    .map_err(|e| e.to_string());
                if done_tx.send((id, result)).is_err() {
                    return;
                }
            }
        });
        Self {
            playing: HashMap::new(),
            loading: HashSet::new(),
            jobs,
            done,
        }
    }
}

impl PreviewCache {
    fn request(&mut self, id: &str, path: PathBuf) {
        if !self.playing.contains_key(id) && self.loading.insert(id.to_owned()) {
            let _ = self.jobs.send((id.to_owned(), path));
        }
    }

    /// Moves the decoded loops into textures.
    fn collect(&mut self, ctx: &egui::Context, now: f64) {
        while let Ok((id, result)) = self.done.try_recv() {
            // A failed decode stays in `loading`, so it is not retried every frame.
            let Ok(preview) = result else { continue };
            self.loading.remove(&id);
            let size = [preview.width as usize, preview.height as usize];
            let frames: Vec<egui::ColorImage> = preview
                .frames
                .iter()
                .map(|f| egui::ColorImage::from_rgba_unmultiplied(size, f))
                .collect();
            let Some(first) = frames.first().cloned() else {
                continue;
            };
            let texture = ctx.load_texture(
                format!("sticker-preview-{id}"),
                first,
                egui::TextureOptions::LINEAR,
            );
            self.playing.insert(
                id,
                Playing {
                    frames,
                    texture,
                    fps: preview.fps.max(1.0),
                    shown: 0,
                    last_used: now,
                },
            );
        }
        if self.playing.len() > MAX_PREVIEWS {
            let mut by_age: Vec<_> = self
                .playing
                .iter()
                .map(|(k, p)| (p.last_used, k.clone()))
                .collect();
            by_age.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, key) in by_age.into_iter().take(self.playing.len() - MAX_PREVIEWS) {
                self.playing.remove(&key);
            }
        }
    }

    /// The texture of `id` showing the frame for `now`.
    fn frame(&mut self, id: &str, now: f64) -> Option<egui::TextureHandle> {
        let playing = self.playing.get_mut(id)?;
        playing.last_used = now;
        let index = (now * playing.fps) as usize % playing.frames.len();
        if index != playing.shown {
            playing.shown = index;
            playing
                .texture
                .set(playing.frames[index].clone(), egui::TextureOptions::LINEAR);
        }
        Some(playing.texture.clone())
    }
}

pub(crate) struct Conversion {
    name: String,
    done: std::sync::mpsc::Receiver<Result<PathBuf, String>>,
}

enum Item {
    Catalog(&'static Sticker),
    Asset(Box<PoolEntry<String, String>>),
}

impl Item {
    fn id(&self) -> &str {
        match self {
            Self::Catalog(sticker) => &sticker.id,
            Self::Asset(entry) => &entry.id,
        }
    }

    fn favorite_key(&self) -> String {
        match self {
            Self::Catalog(sticker) => catalog_key(&sticker.id),
            Self::Asset(entry) => asset_key(&entry.id),
        }
    }
}

fn catalog_key(id: &str) -> String {
    format!("sticker:{id}")
}

enum Event {
    Favorite(String),
    Insert(String),
    Remove(String),
}

/// Where the user's images are, including the per-category folders of
/// earlier versions.
fn yours_folders() -> Vec<String> {
    StickerCategory::ALL
        .iter()
        .map(|c| format!("ribbon-sticker-{}", c.id()))
        .collect()
}

fn load_texture(ctx: &egui::Context, name: &str, path: &Path) -> Option<egui::TextureHandle> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let size = [info.width as usize, info.height as usize];
    let image = match info.color_type {
        png::ColorType::Rgba => egui::ColorImage::from_rgba_unmultiplied(size, data),
        png::ColorType::Rgb => egui::ColorImage::from_rgb(size, data),
        png::ColorType::GrayscaleAlpha => {
            let rgba: Vec<u8> = data
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect();
            egui::ColorImage::from_rgba_unmultiplied(size, &rgba)
        }
        png::ColorType::Grayscale => egui::ColorImage::from_gray(size, data),
        png::ColorType::Indexed => return None,
    };
    Some(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
}

impl VenturiApp {
    /// Converts an imported SVG, GIF or WebP in the background; the result
    /// is imported into Yours by `poll_sticker_conversions`.
    pub(crate) fn start_sticker_conversion(&mut self, path: PathBuf) {
        let dir = stickers::default_dir()
            .unwrap_or_else(|| std::env::temp_dir().join("venturi-stickers"));
        let (tx, done) = std::sync::mpsc::channel();
        let name = vv_session::file_label(&path);
        std::thread::spawn(move || {
            let _ = tx.send(stickers::convert_for_import(&path, &dir));
        });
        self.stickers.conversions.push(Conversion { name, done });
    }

    pub(crate) fn poll_sticker_conversions(&mut self) {
        let mut ready = Vec::new();
        self.stickers
            .conversions
            .retain(|c| match c.done.try_recv() {
                Ok(result) => {
                    ready.push((c.name.clone(), result));
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
            });
        for (name, result) in ready {
            match result {
                Ok(path) => {
                    self.import_into_category(vec![path], Category::Sticker(StickerCategory::Yours))
                }
                Err(e) => self
                    .import_warnings
                    .push(t!("ribbon.sticker_convert_failed", name = name, error = e).into_owned()),
            }
        }
    }

    pub(crate) fn show_sticker_tab(&mut self, ui: &mut egui::Ui, category: Category) {
        let ctx = ui.ctx().clone();
        self.poll_sticker_conversions();
        let now = ui.input(|i| i.time);
        self.stickers.previews.collect(&ctx, now);
        for conversion in &self.stickers.conversions {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t!("ribbon.sticker_converting", name = conversion.name));
            });
        }
        if !self.stickers.conversions.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.stickers.store.is_none()
            && let Some(dir) = stickers::default_dir()
        {
            self.stickers.store = Some(Store::new(dir, &ctx));
        }
        if self.stickers.view != Some(category) {
            self.stickers.view = Some(category);
            self.stickers.search.clear();
            self.stickers.emoji_group = None;
            self.stickers.textures.clear();
            self.stickers.previews.playing.clear();
            if let Some(store) = &mut self.stickers.store {
                store.new_view();
            }
        }
        let emoji = category == Category::Sticker(StickerCategory::Emoji);
        pool_view::search_field(ui, &mut self.stickers.search, "sticker");
        if emoji {
            self.show_emoji_groups(ui);
        }
        ui.add_space(theme::SPACE_1);

        let items = self.sticker_items(category);
        let mut events = Vec::new();
        if items.is_empty() {
            let text = match (category, self.stickers.search.is_empty()) {
                (_, false) => t!("ribbon.sticker_none"),
                (Category::Favorites, _) => t!("ribbon.favorites_empty"),
                _ => t!("ribbon.sticker_empty"),
            };
            ui.label(egui::RichText::new(text).color(theme::TEXT_SECONDARY));
        } else {
            let mut state = std::mem::take(&mut self.stickers);
            let favorites = &self.settings.favorites;
            show_grid(ui, &mut state, &items, favorites, &mut events);
            self.stickers = state;
        }
        for event in events {
            match event {
                Event::Favorite(key) => self.toggle_favorite_key(&key),
                Event::Insert(id) if stickers::find(&id).is_some() => self.stickers.wants.push(id),
                Event::Insert(id) => self.insert_assets_at_playhead(&[id], true),
                Event::Remove(id) => self.remove_ribbon_assets(&[id]),
            }
        }
        self.drive_sticker_wants();
        self.import_dragged_ribbon_assets(&ctx, true);
        if let Some(store) = &self.stickers.store
            && (store.has_pending() || !self.stickers.wants.is_empty())
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn show_emoji_groups(&mut self, ui: &mut egui::Ui) {
        let current = self.stickers.emoji_group;
        let label = |group: Option<u8>| match group {
            Some(g) => t!(format!("ribbon.emoji_group_{g}")),
            None => t!("ribbon.emoji_all"),
        };
        egui::ComboBox::from_id_salt("emoji_group")
            .selected_text(label(current))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.stickers.emoji_group, None, label(None));
                for g in 0..stickers::EMOJI_GROUPS as u8 {
                    ui.selectable_value(&mut self.stickers.emoji_group, Some(g), label(Some(g)));
                }
            });
    }

    fn sticker_items(&mut self, category: Category) -> Vec<Item> {
        let query = self.stickers.search.trim().to_lowercase();
        let mut items = Vec::new();
        let favorites_only = category == Category::Favorites;
        match category {
            Category::Sticker(StickerCategory::Yours) | Category::Favorites => {}
            Category::Sticker(c) => {
                let group = self.stickers.emoji_group;
                items.extend(
                    stickers::all()
                        .iter()
                        .filter(|s| s.category == c)
                        .filter(|s| match (&s.source, group) {
                            (stickers::Source::Noto { group: g, .. }, Some(want)) => *g == want,
                            _ => true,
                        })
                        .filter(|s| s.matches(&query))
                        .map(Item::Catalog),
                );
            }
            _ => return items,
        }
        if favorites_only {
            let favorites = &self.settings.favorites;
            items.extend(
                stickers::all()
                    .iter()
                    .filter(|s| is_favorite(favorites, &catalog_key(&s.id)))
                    .filter(|s| s.matches(&query))
                    .map(Item::Catalog),
            );
        }
        if favorites_only || category == Category::Sticker(StickerCategory::Yours) {
            let entries = self.ribbon_asset_entries(&yours_folders(), favorites_only);
            items.extend(
                entries
                    .into_iter()
                    .filter(|e| e.name.to_lowercase().contains(&query))
                    .map(|e| Item::Asset(Box::new(e))),
            );
        }
        items
    }

    /// The library asset of a catalog sticker, once its image is in the
    /// library; starts the download and the copy otherwise. Ids of the
    /// user's own assets pass through.
    pub(crate) fn sticker_asset_id(&mut self, id: &str) -> Option<String> {
        let Some(sticker) = stickers::find(id) else {
            return Some(id.to_owned());
        };
        let name = sticker.file_name();
        if let Some(asset) = self
            .library
            .assets
            .iter()
            .find(|a| a.folder.as_deref() == Some(CATALOG_FOLDER) && a.name == name)
        {
            return Some(asset.id.clone());
        }
        let store = self.stickers.store.as_mut()?;
        if !store.ensure(sticker, Size::Full) {
            return None;
        }
        if self.stickers.imported.insert(id.to_owned()) {
            let path = store.path(sticker, Size::Full);
            let result = self
                .library
                .ensure_folder(CATALOG_FOLDER, "Sticker catalog")
                .and_then(|()| {
                    let tree = vv_session::MediaTree::scan(&[path]);
                    let origin = self.clipboard_origin();
                    self.library
                        .import_tree(tree, Some(CATALOG_FOLDER.to_owned()), origin)
                });
            if let Err(e) = result {
                self.project_error = Some(t!("library.save_failed", error = e).into_owned());
            }
        }
        None
    }

    fn drive_sticker_wants(&mut self) {
        if let Some(store) = &mut self.stickers.store {
            store.poll();
        }
        for id in std::mem::take(&mut self.stickers.wants) {
            match self.sticker_asset_id(&id) {
                Some(asset) => self.insert_assets_at_playhead(&[asset], true),
                None if self.stickers.store.as_ref().is_some_and(|s| s.failed(&id)) => {
                    let name = stickers::find(&id).map_or(id.clone(), |s| s.name.clone());
                    self.import_warnings =
                        vec![t!("ribbon.sticker_download_failed", name = name).into_owned()];
                }
                None => self.stickers.wants.push(id),
            }
        }
    }
}

fn show_grid(
    ui: &mut egui::Ui,
    state: &mut StickerState,
    items: &[Item],
    favorites: &[String],
    events: &mut Vec<Event>,
) {
    let width = ui.available_width();
    let columns = (((width + GAP) / (MIN_TILE + GAP)) as usize).max(1);
    let tile = ((width - GAP * (columns - 1) as f32) / columns as f32).max(1.0);
    let rows = items.len().div_ceil(columns);
    let mut budget = TEXTURES_PER_FRAME;
    egui::ScrollArea::vertical()
        .id_salt("sticker_grid")
        .auto_shrink([false, false])
        .show_rows(ui, tile, rows, |ui, range| {
            ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
            for row in range {
                ui.horizontal(|ui| {
                    for item in items.iter().skip(row * columns).take(columns) {
                        let favorite = is_favorite(favorites, &item.favorite_key());
                        show_tile(ui, state, item, tile, favorite, &mut budget, events);
                    }
                });
            }
        });
    if budget == 0 {
        ui.ctx().request_repaint();
    }
}

fn show_tile(
    ui: &mut egui::Ui,
    state: &mut StickerState,
    item: &Item,
    size: f32,
    favorite: bool,
    budget: &mut usize,
    events: &mut Vec<Event>,
) {
    let id = item.id();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());
    let background = if resp.hovered() {
        theme::BG_RAISED_HOVER
    } else {
        theme::BG_RAISED
    };
    ui.painter().rect_filled(rect, 6.0, background);
    let now = ui.input(|i| i.time);
    let download = match item {
        Item::Catalog(sticker) if sticker.animated => animated_state(state, sticker),
        _ => Download::None,
    };
    let playing = match (item, download) {
        (Item::Catalog(sticker), Download::Ready) => {
            if let Some(store) = &state.store {
                state
                    .previews
                    .request(&sticker.id, store.path(sticker, Size::Full));
            }
            state.previews.frame(&sticker.id, now)
        }
        _ => None,
    };
    if playing.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    }
    let texture = match item {
        Item::Catalog(_) if playing.is_some() => playing,
        Item::Catalog(sticker) => catalog_texture(ui.ctx(), state, sticker, budget),
        Item::Asset(entry) => match &entry.thumb {
            Thumb::Texture(texture) => Some(texture.clone()),
            _ => None,
        },
    };
    let failed =
        matches!(item, Item::Catalog(s) if state.store.as_ref().is_some_and(|st| st.failed(&s.id)));
    match texture {
        Some(texture) => {
            let area = rect.shrink(size * 0.1);
            let scale =
                (area.width() / texture.size_vec2().x).min(area.height() / texture.size_vec2().y);
            egui::Image::new(&texture)
                .fit_to_exact_size(texture.size_vec2() * scale)
                .paint_at(
                    ui,
                    egui::Rect::from_center_size(area.center(), texture.size_vec2() * scale),
                );
        }
        None => {
            let text = if failed { "!" } else { "…" };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(size * 0.3),
                theme::TEXT_DISABLED,
            );
        }
    }
    if favorite {
        library_ribbon::paint_star(
            ui.painter(),
            rect.left_top() + egui::vec2(9.0, 9.0),
            5.0,
            true,
            theme::FAVORITE,
        );
    }
    match download {
        Download::Remote => paint_download_badge(ui.painter(), rect),
        Download::Fetching => paint_fetching_badge(ui, rect),
        Download::Ready | Download::None => {}
    }
    let name = match item {
        Item::Catalog(sticker) => sticker.name.as_str(),
        Item::Asset(entry) => entry.name.as_str(),
    };
    let resp = if download == Download::Remote {
        resp.on_hover_text(format!("{name} — {}", t!("ribbon.sticker_download_hint")))
    } else {
        resp.on_hover_text(name)
    };
    let interact_id = ui.id().with("sticker_tile").with(id);
    if ui.rect_contains_pointer(rect) && !resp.dragged() {
        let (star, plus) = hover_buttons(ui, interact_id, rect, favorite);
        if star {
            events.push(Event::Favorite(item.favorite_key()));
        }
        if plus {
            events.push(Event::Insert(id.to_owned()));
        }
    }
    if resp.clicked()
        && (failed || download == Download::Remote)
        && let (Item::Catalog(sticker), Some(store)) = (item, &mut state.store)
    {
        store.retry(&sticker.id);
        store.ensure(sticker, Size::Full);
    }
    if resp.double_clicked() && !failed {
        events.push(Event::Insert(id.to_owned()));
    }
    if resp.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), ItemsDrag(vec![id.to_owned()]));
    }
    if resp.dragged() {
        crate::media_pool_ui::show_drag_ghost(ui, interact_id, name);
    }
    if let Item::Asset(entry) = item {
        resp.context_menu(|ui| {
            if ui.button(t!("ribbon.remove")).clicked() {
                events.push(Event::Remove(entry.id.clone()));
                ui.close();
            }
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Download {
    /// Not an animated sticker.
    None,
    /// Animated, not on this computer yet.
    Remote,
    Fetching,
    Ready,
}

fn animated_state(state: &mut StickerState, sticker: &Sticker) -> Download {
    let Some(store) = &mut state.store else {
        return Download::Remote;
    };
    if store.has(sticker, Size::Full) {
        Download::Ready
    } else if store.is_pending(&sticker.id, Size::Full) {
        Download::Fetching
    } else {
        Download::Remote
    }
}

/// A circle with an arrow down in the corner of a sticker to download.
fn paint_download_badge(painter: &egui::Painter, rect: egui::Rect) {
    let c = rect.right_bottom() - egui::vec2(12.0, 12.0);
    painter.circle_filled(c, 8.5, egui::Color32::from_black_alpha(190));
    let stroke = egui::Stroke::new(1.6, egui::Color32::WHITE);
    painter.line_segment(
        [c + egui::vec2(0.0, -4.0), c + egui::vec2(0.0, 3.0)],
        stroke,
    );
    painter.line_segment(
        [c + egui::vec2(-3.0, 0.0), c + egui::vec2(0.0, 3.5)],
        stroke,
    );
    painter.line_segment([c + egui::vec2(3.0, 0.0), c + egui::vec2(0.0, 3.5)], stroke);
    painter.line_segment(
        [c + egui::vec2(-4.0, 5.5), c + egui::vec2(4.0, 5.5)],
        stroke,
    );
}

fn paint_fetching_badge(ui: &egui::Ui, rect: egui::Rect) {
    let c = rect.right_bottom() - egui::vec2(12.0, 12.0);
    let painter = ui.painter();
    painter.circle_filled(c, 8.5, egui::Color32::from_black_alpha(190));
    let t = ui.input(|i| i.time) as f32;
    let points: Vec<egui::Pos2> = (0..=10)
        .map(|i| {
            let a = t * 6.0 + i as f32 * 0.22;
            c + egui::vec2(a.cos(), a.sin()) * 5.0
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.8, theme::ACCENT),
    ));
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(16));
}

fn catalog_texture(
    ctx: &egui::Context,
    state: &mut StickerState,
    sticker: &'static Sticker,
    budget: &mut usize,
) -> Option<egui::TextureHandle> {
    if let Some(texture) = state.textures.get(&sticker.id) {
        return Some(texture.clone());
    }
    let store = state.store.as_mut()?;
    if !store.ensure(sticker, Size::Thumb) || *budget == 0 {
        return None;
    }
    *budget -= 1;
    let texture = load_texture(ctx, &sticker.id, &store.path(sticker, Size::Thumb))?;
    state.textures.insert(sticker.id.clone(), texture.clone());
    Some(texture)
}

const SWATCHES: [(&str, [u8; 3]); 10] = [
    ("white", [255, 255, 255]),
    ("red", [239, 68, 68]),
    ("orange", [249, 115, 22]),
    ("yellow", [250, 204, 21]),
    ("green", [34, 197, 94]),
    ("cyan", [6, 182, 212]),
    ("blue", [59, 130, 246]),
    ("purple", [168, 85, 247]),
    ("pink", [236, 72, 153]),
    ("black", [17, 17, 17]),
];

fn to_rgba([r, g, b]: [u8; 3]) -> vv_core::Rgba {
    let f = |v: u8| f32::from(v) / 255.0;
    vv_core::Rgba {
        r: f(r),
        g: f(g),
        b: f(b),
        a: 1.0,
    }
}

fn same_color(a: vv_core::Rgba, b: vv_core::Rgba) -> bool {
    let near = |x: f32, y: f32| (x - y).abs() < 0.5 / 255.0;
    near(a.r, b.r) && near(a.g, b.g) && near(a.b, b.b)
}

/// Swatches and a free picker for the color of a sticker. `Some(None)`: back
/// to white; `Some(Some(c))`: the new color; `None`: untouched.
pub(crate) fn sticker_color_section(
    ui: &mut egui::Ui,
    current: Option<vv_core::Rgba>,
) -> Option<Option<vv_core::Rgba>> {
    let mut choice = None;
    ui.label(egui::RichText::new(t!("props.sticker_color")).strong());
    let shown = current.unwrap_or(vv_core::Rgba::WHITE);
    ui.horizontal_wrapped(|ui| {
        for (name, rgb) in SWATCHES {
            let color = to_rgba(rgb);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
            let fill = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            ui.painter().circle_filled(rect.center(), 8.0, fill);
            let selected = same_color(shown, color);
            let stroke = if selected {
                egui::Stroke::new(2.0, theme::ACCENT)
            } else {
                egui::Stroke::new(1.0, theme::BORDER_SUBTLE)
            };
            ui.painter().circle_stroke(rect.center(), 9.5, stroke);
            if resp
                .on_hover_text(t!(format!("props.swatch_{name}")))
                .clicked()
            {
                choice = Some((name != "white").then_some(color));
            }
        }
        let mut rgba: [f32; 4] = shown.into();
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            rgba[3] = 1.0;
            choice = Some(Some(rgba.into()));
        }
    });
    choice
}

#[cfg(test)]
#[path = "tests/stickers_ui.rs"]
mod tests;
