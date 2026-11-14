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
    store: Option<Store>,
    textures: HashMap<String, egui::TextureHandle>,
    search: String,
    emoji_group: Option<u8>,
    view: Option<Category>,
    /// Catalog ids whose "+" waits for the image to reach the library.
    wants: Vec<String>,
    imported: HashSet<String>,
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
    pub(crate) fn show_sticker_tab(&mut self, ui: &mut egui::Ui, category: Category) {
        let ctx = ui.ctx().clone();
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
        let name = format!("{}.png", sticker.file_stem());
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
    let texture = match item {
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
    let name = match item {
        Item::Catalog(sticker) => sticker.name.as_str(),
        Item::Asset(entry) => entry.name.as_str(),
    };
    let resp = resp.on_hover_text(name);
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
    if failed
        && resp.clicked()
        && let (Item::Catalog(sticker), Some(store)) = (item, &mut state.store)
    {
        store.retry(&sticker.id);
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
