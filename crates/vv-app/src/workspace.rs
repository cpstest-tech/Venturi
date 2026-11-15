//! The editor's blocks: tiled in the main window, or each in a window of
//! its own.

use std::borrow::Cow;
use std::collections::HashMap;

use egui_tiles::{Container, Linear, LinearDir, Tile, TileId, Tiles, Tree};
use serde::{Deserialize, Serialize};

const TREE_ID: &str = "tandem_workspace";
const DEFAULT_WINDOW_SIZE: [f32; 2] = [800.0, 500.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Block {
    Library,
    Player,
    Properties,
    Timeline,
}

impl Block {
    pub const ALL: [Block; 4] = [
        Block::Library,
        Block::Player,
        Block::Properties,
        Block::Timeline,
    ];

    pub fn title(self) -> Cow<'static, str> {
        match self {
            Block::Library => t!("workspace.library"),
            Block::Player => t!("workspace.player"),
            Block::Properties => t!("workspace.properties"),
            Block::Timeline => t!("workspace.timeline"),
        }
    }

    pub fn viewport_id(self) -> egui::ViewportId {
        egui::ViewportId::from_hash_of(("tandem_block", self))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    /// `None` where the windowing system does not tell it (Wayland).
    pub pos: Option<[f32; 2]>,
    pub size: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detached {
    pub block: Block,
    pub window: WindowGeometry,
    /// The geometry the window was opened with: the builder must stay the
    /// same frame after frame, or egui would move the window back to it.
    #[serde(skip)]
    opened_with: Option<WindowGeometry>,
}

pub type BlockRects = HashMap<Block, egui::Rect>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    tree: Tree<Block>,
    detached: Vec<Detached>,
}

impl Default for Workspace {
    fn default() -> Self {
        fn tab(tiles: &mut Tiles<Block>, block: Block) -> TileId {
            let pane = tiles.insert_pane(block);
            tiles.insert_tab_tile(vec![pane])
        }
        let mut tiles = Tiles::default();
        let library = tab(&mut tiles, Block::Library);
        let player = tab(&mut tiles, Block::Player);
        let properties = tab(&mut tiles, Block::Properties);
        let timeline = tab(&mut tiles, Block::Timeline);
        let mut top = Linear::new(LinearDir::Horizontal, vec![library, player, properties]);
        top.shares.set_share(library, 1.0);
        top.shares.set_share(player, 2.0);
        top.shares.set_share(properties, 1.0);
        let top = tiles.insert_container(top);
        let mut column = Linear::new(LinearDir::Vertical, vec![top, timeline]);
        column.shares.set_share(top, 3.0);
        column.shares.set_share(timeline, 2.0);
        let root = tiles.insert_container(column);
        Self {
            tree: Tree::new(TREE_ID, root, tiles),
            detached: Vec::new(),
        }
    }
}

impl Workspace {
    /// The saved arrangement, or the default one if it cannot be read or
    /// holds a block twice.
    pub fn from_value(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value::<Self>(v.clone()).ok())
            .filter(Self::is_consistent)
            .unwrap_or_default()
    }

    pub fn to_value(&self) -> Option<serde_json::Value> {
        serde_json::to_value(self).ok()
    }

    fn is_consistent(&self) -> bool {
        let mut all = self.docked_blocks();
        all.extend(self.detached.iter().map(|d| d.block));
        let unique: std::collections::HashSet<Block> = all.iter().copied().collect();
        unique.len() == all.len()
    }

    fn docked_blocks(&self) -> Vec<Block> {
        self.tree
            .tiles
            .tiles()
            .filter_map(|tile| match tile {
                Tile::Pane(block) => Some(*block),
                Tile::Container(_) => None,
            })
            .collect()
    }

    pub fn is_docked(&self, block: Block) -> bool {
        self.tree.tiles.find_pane(&block).is_some()
    }

    pub fn is_detached(&self, block: Block) -> bool {
        self.detached.iter().any(|d| d.block == block)
    }

    pub fn is_shown(&self, block: Block) -> bool {
        self.is_docked(block) || self.is_detached(block)
    }

    pub fn closed_blocks(&self) -> Vec<Block> {
        Block::ALL
            .into_iter()
            .filter(|&b| !self.is_shown(b))
            .collect()
    }

    pub fn has_docked_blocks(&self) -> bool {
        self.tree.root.is_some() && !self.docked_blocks().is_empty()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn reopen(&mut self, block: Block) {
        if !self.is_shown(block) {
            self.insert_docked(block);
        }
    }

    pub fn detach(&mut self, block: Block, size: egui::Vec2) {
        if !self.undock(block) {
            return;
        }
        let size = if size.x >= 100.0 && size.y >= 100.0 {
            [size.x, size.y]
        } else {
            DEFAULT_WINDOW_SIZE
        };
        self.detached.push(Detached {
            block,
            window: WindowGeometry { pos: None, size },
            opened_with: None,
        });
    }

    pub fn redock(&mut self, block: Block) {
        let before = self.detached.len();
        self.detached.retain(|d| d.block != block);
        if self.detached.len() != before {
            self.insert_docked(block);
        }
    }

    fn undock(&mut self, block: Block) -> bool {
        match self.tree.tiles.find_pane(&block) {
            Some(tile) => {
                self.tree.remove_recursively(tile);
                true
            }
            None => false,
        }
    }

    /// Timeline goes back at the bottom, the others in the top row.
    fn insert_docked(&mut self, block: Block) {
        let tiles = &mut self.tree.tiles;
        let pane = tiles.insert_pane(block);
        let new = tiles.insert_tab_tile(vec![pane]);
        let Some(root) = self.tree.root else {
            self.tree.root = Some(new);
            return;
        };
        let dir = if block == Block::Timeline {
            LinearDir::Vertical
        } else {
            LinearDir::Horizontal
        };
        let target = match tiles.get(root) {
            Some(Tile::Container(Container::Linear(column)))
                if dir == LinearDir::Horizontal && column.dir == LinearDir::Vertical =>
            {
                column.children.first().copied().unwrap_or(root)
            }
            _ => root,
        };
        if let Some(Tile::Container(Container::Linear(linear))) = tiles.get_mut(target)
            && linear.dir == dir
        {
            linear.add_child(new);
            return;
        }
        let wrapper = tiles.insert_container(Container::new_linear(dir, vec![target, new]));
        if target == root {
            self.tree.root = Some(wrapper);
        } else if let Some(Tile::Container(parent)) = tiles.get_mut(root) {
            let _ = parent.replace_child(target, wrapper);
        }
    }

    /// Lays out the docked blocks in `ui`, tab bars and dividers included,
    /// and returns where each one goes: the caller draws them in the order
    /// the frame needs, not in the tree's.
    pub fn layout_ui(&mut self, ui: &mut egui::Ui) -> BlockRects {
        let mut behavior = LayoutBehavior::default();
        self.tree.ui(&mut behavior, ui);
        if let Some((block, size)) = behavior.detach {
            self.detach(block, size);
            ui.ctx().request_repaint();
        }
        behavior.rects
    }

    pub fn detached_blocks(&self) -> Vec<Block> {
        self.detached.iter().map(|d| d.block).collect()
    }

    pub fn viewport_builder(&mut self, block: Block) -> Option<egui::ViewportBuilder> {
        let detached = self.detached.iter_mut().find(|d| d.block == block)?;
        let opened = *detached.opened_with.get_or_insert(detached.window);
        let mut builder = egui::ViewportBuilder::default()
            .with_title(format!("{} — Tandem", block.title()))
            .with_app_id("tandem")
            .with_inner_size(opened.size);
        if let Some(pos) = opened.pos {
            builder = builder.with_position(pos);
        }
        Some(builder)
    }

    /// Called inside the block's window: remembers where it is and how big.
    pub fn track_window(&mut self, block: Block, ui: &egui::Ui) {
        let (inner, outer) = ui.input(|i| (i.viewport().inner_rect, i.viewport().outer_rect));
        let Some(detached) = self.detached.iter_mut().find(|d| d.block == block) else {
            return;
        };
        if let Some(inner) = inner {
            detached.window.size = [inner.width(), inner.height()];
        }
        if let Some(outer) = outer {
            detached.window.pos = Some([outer.min.x, outer.min.y]);
        }
    }
}

#[derive(Default)]
struct LayoutBehavior {
    rects: BlockRects,
    detach: Option<(Block, egui::Vec2)>,
}

impl egui_tiles::Behavior<Block> for LayoutBehavior {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        block: &mut Block,
    ) -> egui_tiles::UiResponse {
        self.rects.insert(*block, ui.max_rect());
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, block: &Block) -> egui::WidgetText {
        block.title().into()
    }

    fn is_tab_closable(&self, _tiles: &Tiles<Block>, _tile_id: TileId) -> bool {
        true
    }

    fn top_bar_right_ui(
        &mut self,
        tiles: &Tiles<Block>,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        tabs: &egui_tiles::Tabs,
        _scroll_offset: &mut f32,
    ) {
        let Some(active) = tabs.active else {
            return;
        };
        let Some(&block) = tiles.get_pane(&active) else {
            return;
        };
        if ui
            .small_button("🗗")
            .on_hover_text(t!("workspace.detach"))
            .clicked()
        {
            let size = tiles.rect(active).map_or(egui::Vec2::ZERO, |r| r.size());
            self.detach = Some((block, size));
        }
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        crate::theme::PANEL_GAP
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        28.0
    }

    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> egui::Color32 {
        crate::theme::BG_PANEL
    }

    fn resize_stroke(&self, style: &egui::Style, state: egui_tiles::ResizeState) -> egui::Stroke {
        match state {
            egui_tiles::ResizeState::Idle => {
                egui::Stroke::new(self.gap_width(style), crate::theme::BG_WINDOW)
            }
            _ => egui::Stroke::new(self.gap_width(style), crate::theme::ACCENT),
        }
    }

    fn tab_bg_color(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<Block>,
        _tile_id: TileId,
        _state: &egui_tiles::TabState,
    ) -> egui::Color32 {
        egui::Color32::TRANSPARENT
    }

    fn tab_outline_stroke(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<Block>,
        _tile_id: TileId,
        _state: &egui_tiles::TabState,
    ) -> egui::Stroke {
        egui::Stroke::NONE
    }

    fn tab_bar_hline_stroke(&self, _visuals: &egui::Visuals) -> egui::Stroke {
        egui::Stroke::new(1.0, crate::theme::BORDER_SUBTLE)
    }

    fn tab_text_color(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<Block>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Color32 {
        if state.active {
            crate::theme::TEXT_PRIMARY
        } else {
            crate::theme::TEXT_SECONDARY
        }
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }

    // On top: the blocks are drawn after the tree, over its own layer.
    fn paint_drag_preview(
        &self,
        visuals: &egui::Visuals,
        painter: &egui::Painter,
        parent_rect: Option<egui::Rect>,
        preview_rect: egui::Rect,
    ) {
        let painter = painter.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("workspace_drag_preview"),
        ));
        let stroke = egui::Stroke::new(1.0, visuals.selection.stroke.color);
        if let Some(parent_rect) = parent_rect {
            painter.rect_stroke(parent_rect, 1.0, stroke, egui::StrokeKind::Inside);
        }
        painter.rect(
            preview_rect,
            1.0,
            visuals.selection.bg_fill.gamma_multiply(0.5),
            stroke,
            egui::StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
#[path = "tests/workspace.rs"]
mod tests;
