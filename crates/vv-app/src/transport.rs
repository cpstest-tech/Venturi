//! Playback bar under the viewer: playhead, in/out markers and play/pause
//! button. It serves both the preview of a media pool item (in/out = the
//! portion to bring onto the timeline) and the timeline (in/out = the
//! portion to export).

use vv_core::FrameIdx;

/// In/out markers: an unset In follows the start of the bar, and an unset Out
/// follows its end when the content duration changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkRange {
    pub mark_in: Option<FrameIdx>,
    /// Exclusive: "o" on frame `p` includes it.
    pub mark_out: Option<FrameIdx>,
}

impl MarkRange {
    /// Effective `(in, out)` within `0..=total`, with `in <= out`.
    pub fn resolve(&self, total: FrameIdx) -> (FrameIdx, FrameIdx) {
        let total = total.max(0);
        let out = self.mark_out.unwrap_or(total).clamp(0, total);
        let mark_in = self.mark_in.unwrap_or(0).clamp(0, out);
        (mark_in, out)
    }

    pub fn set_in(&mut self, frame: FrameIdx, total: FrameIdx) {
        let frame = frame.clamp(0, total.max(0));
        self.mark_in = Some(frame);
        if self.mark_out.is_some_and(|out| out <= frame) {
            self.mark_out = None;
        }
    }

    pub fn set_out(&mut self, frame: FrameIdx, total: FrameIdx) {
        let out = (frame + 1).clamp(0, total.max(0));
        self.mark_out = Some(out);
        if self.mark_in.is_some_and(|mark_in| mark_in >= out) {
            self.mark_in = None;
        }
    }

    pub fn clear_in(&mut self) {
        self.mark_in = None;
    }

    pub fn clear_out(&mut self) {
        self.mark_out = None;
    }

    pub fn clear_all(&mut self) {
        *self = Self::default();
    }

    pub fn is_full(&self, total: FrameIdx) -> bool {
        self.resolve(total) == (0, total.max(0))
    }
}

#[derive(Default)]
pub struct TransportResponse {
    pub seek: Option<FrameIdx>,
    pub scrubbing: bool,
    /// New `(in, out)` while a marker handle is dragged.
    pub marks: Option<(FrameIdx, FrameIdx)>,
    pub toggle_play: bool,
    pub fit_view: bool,
    pub toggle_fullscreen: bool,
    pub ratio: Option<crate::first_media::Ratio>,
    pub quality: Option<crate::settings::PreviewResolution>,
    /// `Some(None)` switches the safe zones off.
    pub safe_zones: Option<Option<String>>,
}

/// Current values shown checked in the Player's menus.
#[derive(Clone)]
pub struct PlayerOptions {
    pub ratio: crate::first_media::Ratio,
    pub quality: crate::settings::PreviewResolution,
    /// Index into `safe_zone_platforms` when the overlay is on.
    pub safe_zones: Option<usize>,
    /// (id, name)
    pub safe_zone_platforms: Vec<(String, String)>,
}

const BAR_HEIGHT: f32 = 30.0;
const TRACK_Y: f32 = 12.0;
const SIDE_PADDING: f32 = 10.0;
const CONTROL_BAR_HEIGHT: f32 = 44.0;
/// Seek bar plus control bar, without the item spacing between them.
pub const HEIGHT: f32 = BAR_HEIGHT + CONTROL_BAR_HEIGHT;
const PLAY_HIT_SIZE: f32 = 28.0;
/// Half width of the grab area of an in/out handle.
const HANDLE_GRAB: f32 = 7.0;

/// `total` frames of content; the playhead can sit on `0..total`.
pub fn show_transport(
    ui: &mut egui::Ui,
    total: FrameIdx,
    playhead: FrameIdx,
    marks: (FrameIdx, FrameIdx),
    playing: bool,
    fps: f64,
    options: PlayerOptions,
) -> TransportResponse {
    let mut response = TransportResponse::default();
    let (rect, bar_resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), BAR_HEIGHT),
        egui::Sense::click_and_drag(),
    );
    let track = egui::Rect::from_min_max(
        egui::pos2(rect.left() + SIDE_PADDING, rect.top() + TRACK_Y - 2.0),
        egui::pos2(rect.right() - SIDE_PADDING, rect.top() + TRACK_Y + 2.0),
    );
    let span = total.max(1) as f32;
    let x_of =
        |frame: FrameIdx| track.left() + track.width() * (frame as f32 / span).clamp(0.0, 1.0);

    let (mark_in, mark_out) = marks;
    let handle_rect = |frame: FrameIdx| {
        egui::Rect::from_x_y_ranges(
            x_of(frame) - HANDLE_GRAB..=x_of(frame) + HANDLE_GRAB,
            track.top() - 4.0..=rect.bottom(),
        )
    };
    let handle_at = |pos: egui::Pos2| {
        // Both at the same spot: the one that can still move inwards.
        let on_in = handle_rect(mark_in).contains(pos);
        let on_out = handle_rect(mark_out).contains(pos);
        match (on_in, on_out) {
            (true, true) => Some(pos.x >= x_of(mark_in)),
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
        }
    };
    let grabbed_id = bar_resp.id.with("grabbed_mark");
    let mut grabbed: Option<bool> = ui.data(|d| d.get_temp(grabbed_id));
    if bar_resp.is_pointer_button_down_on() {
        if grabbed.is_none() && total > 0 {
            grabbed = ui.input(|i| i.pointer.press_origin()).and_then(handle_at);
        }
    } else {
        grabbed = None;
    }
    ui.data_mut(|d| d.insert_temp(grabbed_id, grabbed));
    if grabbed.is_some()
        || bar_resp
            .hover_pos()
            .is_some_and(|p| total > 0 && handle_at(p).is_some())
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }

    response.scrubbing = bar_resp.is_pointer_button_down_on();
    if total > 0
        && let Some(pos) = bar_resp.interact_pointer_pos()
    {
        let frac = ((pos.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
        let at = (frac * span).round() as FrameIdx;
        // The playhead shows the frame the handle now cuts at.
        let frame = match grabbed {
            Some(true) => {
                let new_in = at.min(mark_out - 1).max(0);
                response.marks = Some((new_in, mark_out));
                new_in
            }
            Some(false) => {
                let new_out = at.max(mark_in + 1).min(total);
                response.marks = Some((mark_in, new_out));
                new_out - 1
            }
            None => at.min(total - 1),
        };
        if frame != playhead {
            response.seek = Some(frame);
        }
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(track, crate::theme::RADIUS_TAG, crate::theme::BG_RAISED);
    let region = egui::Rect::from_min_max(
        egui::pos2(x_of(mark_in), track.top()),
        egui::pos2(x_of(mark_out), track.bottom()),
    );
    painter.rect_filled(
        region,
        crate::theme::RADIUS_TAG,
        crate::theme::TEXT_SECONDARY,
    );

    let marker_color = crate::theme::TEXT_SECONDARY;
    for (frame, is_in) in [(mark_in, true), (mark_out, false)] {
        let x = x_of(frame);
        painter.vline(
            x,
            track.top()..=track.bottom() + 6.0,
            egui::Stroke::new(1.0, marker_color),
        );
        // Flag pointing towards the inside of the region.
        let flag_w = if is_in { 7.0 } else { -7.0 };
        let top = track.bottom() + 6.0;
        let mut points = vec![
            egui::pos2(x, top),
            egui::pos2(x + flag_w, top + 3.0),
            egui::pos2(x + flag_w, top + 10.0),
            egui::pos2(x, top + 10.0),
        ];
        // egui's tessellator wants the vertices in clockwise order.
        if !is_in {
            points.reverse();
        }
        painter.add(egui::Shape::convex_polygon(
            points,
            marker_color,
            egui::Stroke::NONE,
        ));
    }

    let playhead_color = crate::theme::PLAYHEAD;
    let px = x_of(playhead);
    painter.vline(
        px,
        rect.top() + 2.0..=track.bottom() + 4.0,
        egui::Stroke::new(2.0, playhead_color),
    );
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(px, rect.top() + 6.0), egui::vec2(10.0, 10.0)),
        2.0,
        playhead_color,
    );

    show_control_bar(ui, total, playhead, fps, playing, options, &mut response);

    response
}

#[derive(Clone, Copy)]
enum Icon {
    SafeZones,
    Fit,
    Fullscreen,
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, hint: String) -> egui::Response {
    let size = crate::theme::ICON_SIZE + 8.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let color = if response.hovered() {
        egui::Color32::WHITE
    } else {
        crate::theme::TEXT_SECONDARY
    };
    paint_icon(ui.painter(), rect, icon, color);
    response.on_hover_text(hint)
}

fn paint_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    let stroke = egui::Stroke::new(crate::theme::ICON_STROKE, color);
    let half = crate::theme::ICON_SIZE / 2.0 - 1.0;
    let arm = 4.5;
    let c = rect.center();
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let corner = c + egui::vec2(sx * half, sy * half);
        painter.line_segment([corner, corner + egui::vec2(-sx * arm, 0.0)], stroke);
        painter.line_segment([corner, corner + egui::vec2(0.0, -sy * arm)], stroke);
    }
    match icon {
        Icon::SafeZones => {
            let inner = egui::Rect::from_center_size(c, egui::vec2(half, half * 1.5));
            painter.rect_stroke(inner, 1.0, stroke, egui::StrokeKind::Inside);
        }
        Icon::Fit => {
            painter.circle_stroke(c - egui::vec2(0.8, 0.8), 2.8, stroke);
            painter.line_segment([c + egui::vec2(1.2, 1.2), c + egui::vec2(3.6, 3.6)], stroke);
        }
        Icon::Fullscreen => {
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let from = c + egui::vec2(sx * 1.5, sy * 1.5);
                painter.line_segment([from, from + egui::vec2(sx * 2.5, sy * 2.5)], stroke);
            }
        }
    }
}

fn outlined_menu(ui: &mut egui::Ui, label: String, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        for state in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            state.weak_bg_fill = egui::Color32::TRANSPARENT;
            state.bg_stroke = egui::Stroke::new(1.0, crate::theme::BORDER_SUBTLE);
        }
        widgets.inactive.fg_stroke.color = crate::theme::TEXT_SECONDARY;
        let text = egui::RichText::new(label).size(crate::theme::FONT_SMALL);
        ui.menu_button(text, add_contents);
    });
}

fn show_control_bar(
    ui: &mut egui::Ui,
    total: FrameIdx,
    playhead: FrameIdx,
    fps: f64,
    playing: bool,
    options: PlayerOptions,
    response: &mut TransportResponse,
) {
    let (bar, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CONTROL_BAR_HEIGHT),
        egui::Sense::hover(),
    );
    let enabled = total > 0;
    let fps = fps.max(1e-9);
    let tc =
        |frames: FrameIdx| crate::timeline_ui::format_timecode(frames.max(0) as f64 / fps, fps);
    let font = egui::FontId::monospace(crate::theme::FONT_BODY);
    let painter = ui.painter();
    let (current_color, total_color) = if enabled {
        (crate::theme::ACCENT, crate::theme::TEXT_SECONDARY)
    } else {
        (crate::theme::TEXT_DISABLED, crate::theme::TEXT_DISABLED)
    };
    let left = painter.text(
        egui::pos2(bar.left() + crate::theme::SPACE_3, bar.center().y),
        egui::Align2::LEFT_CENTER,
        tc(playhead),
        font.clone(),
        current_color,
    );
    let timecode = painter.text(
        egui::pos2(left.right() + 4.0, bar.center().y),
        egui::Align2::LEFT_CENTER,
        format!("/ {}", tc(total)),
        font,
        total_color,
    );

    // Centred, unless a narrow player would put it under the right group
    // (measured last frame).
    let right_width_id = ui.id().with("control_bar_right_width");
    let right_width: f32 = ui.data(|d| d.get_temp(right_width_id)).unwrap_or(0.0);
    let half = PLAY_HIT_SIZE / 2.0 + crate::theme::SPACE_3;
    let max_x = bar.right() - crate::theme::SPACE_3 - right_width - half;
    let play_x = bar.center().x.min(max_x).max(timecode.right() + half);
    let play_rect = egui::Rect::from_center_size(
        egui::pos2(play_x, bar.center().y),
        egui::Vec2::splat(PLAY_HIT_SIZE),
    );
    let play = ui.interact(play_rect, ui.id().with("play_button"), egui::Sense::click());
    let color = if !enabled {
        crate::theme::TEXT_DISABLED
    } else if play.hovered() {
        egui::Color32::WHITE
    } else {
        crate::theme::TEXT_PRIMARY
    };
    let c = play_rect.center();
    if playing {
        for dx in [-3.5, 3.5] {
            ui.painter().rect_filled(
                egui::Rect::from_center_size(c + egui::vec2(dx, 0.0), egui::vec2(3.5, 13.0)),
                1.0,
                color,
            );
        }
    } else {
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                c + egui::vec2(-4.5, -7.0),
                c + egui::vec2(7.0, 0.0),
                c + egui::vec2(-4.5, 7.0),
            ],
            color,
            egui::Stroke::NONE,
        ));
    }
    if play.on_hover_text(t!("transport.play_hint")).clicked() {
        response.toggle_play = true;
    }

    let right = egui::Rect::from_min_max(
        egui::pos2(play_rect.right() + crate::theme::SPACE_3, bar.top()),
        egui::pos2(bar.right() - crate::theme::SPACE_3, bar.bottom()),
    );
    let mut right_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(right)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    right_ui.set_clip_rect(right.intersect(ui.clip_rect()));
    let ui = &mut right_ui;
    ui.spacing_mut().item_spacing.x = crate::theme::SPACE_3;
    if icon_button(ui, Icon::Fullscreen, t!("player.fullscreen").into_owned()).clicked() {
        response.toggle_fullscreen = true;
    }
    outlined_menu(ui, t!("player.ratio").into_owned(), |ui| {
        for ratio in crate::first_media::Ratio::MENU {
            let label = ratio
                .label()
                .map_or_else(|| t!("player.original").into_owned(), str::to_owned);
            if ui
                .add(egui::Button::selectable(options.ratio == ratio, label))
                .clicked()
            {
                response.ratio = Some(ratio);
                ui.close();
            }
        }
    });
    if icon_button(ui, Icon::Fit, t!("player.fit_view").into_owned()).clicked() {
        response.fit_view = true;
    }
    let quality_label =
        t!(format!("menu.preview_resolution_{}", options.quality.id())).into_owned();
    outlined_menu(ui, quality_label, |ui| {
        for quality in crate::settings::PreviewResolution::ALL {
            let label = t!(format!("menu.preview_resolution_{}", quality.id()));
            if ui
                .add(egui::Button::selectable(options.quality == quality, label))
                .clicked()
            {
                response.quality = Some(quality);
                ui.close();
            }
        }
    });
    safe_zones_menu(ui, &options, response);
    let used = ui.min_rect().width();
    if (used - right_width).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(right_width_id, used));
        ui.ctx().request_repaint();
    }
}

/// One button: its face shows the chosen platform while the overlay is on,
/// the menu switches platform or turns it off.
fn safe_zones_menu(ui: &mut egui::Ui, options: &PlayerOptions, response: &mut TransportResponse) {
    let chosen = options
        .safe_zones
        .and_then(|i| options.safe_zone_platforms.get(i));
    let size = crate::theme::ICON_SIZE + 8.0;
    let (rect, button) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let color = if chosen.is_some() {
        crate::theme::ACCENT
    } else if button.hovered() {
        egui::Color32::WHITE
    } else {
        crate::theme::TEXT_SECONDARY
    };
    match chosen {
        Some((id, _)) if crate::safe_zones::paint_icon(ui.painter(), rect.center(), id, color) => {}
        _ => paint_icon(ui.painter(), rect, Icon::SafeZones, color),
    }
    let button = button.on_hover_text(t!("player.safe_zones"));
    egui::Popup::menu(&button).show(|ui| {
        if menu_row(
            ui,
            chosen.is_none(),
            &t!("player.safe_zones_off"),
            |p, c, color| {
                paint_icon(
                    p,
                    egui::Rect::from_center_size(c, egui::Vec2::splat(size)),
                    Icon::SafeZones,
                    color,
                );
            },
        )
        .clicked()
        {
            response.safe_zones = Some(None);
        }
        for (i, (id, name)) in options.safe_zone_platforms.iter().enumerate() {
            let row = menu_row(ui, options.safe_zones == Some(i), name, |p, c, color| {
                crate::safe_zones::paint_icon(p, c, id, color);
            });
            if row.clicked() {
                response.safe_zones = Some(Some(id.clone()));
            }
        }
    });
}

fn menu_row(
    ui: &mut egui::Ui,
    selected: bool,
    label: &str,
    icon: impl FnOnce(&egui::Painter, egui::Pos2, egui::Color32),
) -> egui::Response {
    let height = crate::theme::CONTROL_HEIGHT;
    let (rect, row) = ui.allocate_exact_size(egui::vec2(170.0, height), egui::Sense::click());
    let visuals = ui.style().interact_selectable(&row, selected);
    if selected || row.hovered() {
        ui.painter()
            .rect_filled(rect, crate::theme::RADIUS_CONTROL, visuals.weak_bg_fill);
    }
    let color = if selected {
        crate::theme::ACCENT
    } else {
        visuals.text_color()
    };
    icon(
        ui.painter(),
        rect.left_center() + egui::vec2(14.0, 0.0),
        color,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(30.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(crate::theme::FONT_BODY),
        visuals.text_color(),
    );
    row
}

#[cfg(test)]
#[path = "tests/transport.rs"]
mod tests;
