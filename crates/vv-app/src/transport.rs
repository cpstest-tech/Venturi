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
    pub scrub_started: bool,
    pub toggle_play: bool,
    pub fit_view: bool,
    pub toggle_fullscreen: bool,
    pub ratio: Option<crate::first_media::Ratio>,
    pub quality: Option<crate::settings::PreviewResolution>,
}

/// Current values shown checked in the Player's menus.
#[derive(Clone, Copy)]
pub struct PlayerOptions {
    pub ratio: crate::first_media::Ratio,
    pub quality: crate::settings::PreviewResolution,
}

const BAR_HEIGHT: f32 = 30.0;
const TRACK_Y: f32 = 12.0;
const SIDE_PADDING: f32 = 10.0;
const CONTROL_BAR_HEIGHT: f32 = 44.0;
const PLAY_HIT_SIZE: f32 = 28.0;

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

    response.scrub_started = bar_resp.drag_started();
    if total > 0
        && let Some(pos) = bar_resp.interact_pointer_pos()
    {
        let frac = ((pos.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
        let frame = ((frac * span).round() as FrameIdx).min(total - 1);
        if frame != playhead {
            response.seek = Some(frame);
        }
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(track, crate::theme::RADIUS_TAG, crate::theme::BG_RAISED);
    let (mark_in, mark_out) = marks;
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
    let stroke = egui::Stroke::new(crate::theme::ICON_STROKE, color);
    let painter = ui.painter();
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
    response.on_hover_text(hint)
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
    painter.text(
        egui::pos2(left.right() + 4.0, bar.center().y),
        egui::Align2::LEFT_CENTER,
        format!("/ {}", tc(total)),
        font,
        total_color,
    );

    let play_rect = egui::Rect::from_center_size(bar.center(), egui::Vec2::splat(PLAY_HIT_SIZE));
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
    ui.add_enabled_ui(false, |ui| {
        icon_button(ui, Icon::SafeZones, t!("player.safe_zones").into_owned())
    })
    .inner
    .on_disabled_hover_text(t!("common.coming_soon"));
}

#[cfg(test)]
#[path = "tests/transport.rs"]
mod tests;
