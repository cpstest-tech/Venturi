//! Command bar above the timeline, the empty-timeline placeholder and the
//! hand-drawn icons shared with the track headers (some platforms lack the
//! glyphs in egui's fonts).

use crate::settings::{Action, Keymap};
use crate::timeline_ui::{TimelineCommand, TimelineDrag, TimelineState, TimelineTool};

const BUTTON_SIZE: egui::Vec2 = egui::vec2(26.0, 22.0);
const ZOOM_SLIDER_WIDTH: f32 = 110.0;

const TOOLS: [(TimelineTool, Action); 5] = [
    (TimelineTool::Select, Action::SelectTool),
    (TimelineTool::Blade, Action::BladeTool),
    (TimelineTool::SelectLeft, Action::SelectLeftTool),
    (TimelineTool::SelectRight, Action::SelectRightTool),
    (TimelineTool::Slip, Action::SlipTool),
];

fn tool_label(tool: TimelineTool) -> std::borrow::Cow<'static, str> {
    match tool {
        TimelineTool::Select => t!("timeline_bar.mode_select"),
        TimelineTool::Blade => t!("timeline_bar.mode_blade"),
        TimelineTool::SelectLeft => t!("timeline_bar.mode_select_left"),
        TimelineTool::SelectRight => t!("timeline_bar.mode_select_right"),
        TimelineTool::Slip => t!("toolbar.slip_tool"),
    }
}

/// The bar above the timeline. Requests go to `state.commands`, the
/// toggles are flipped in place.
pub(crate) fn show_bar(
    ui: &mut egui::Ui,
    state: &mut TimelineState,
    snapping: &mut bool,
    keymap: &Keymap,
    can_undo: bool,
    can_redo: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        mode_menu(ui, state, keymap);
        ui.separator();

        let tip = |text: std::borrow::Cow<str>, action| keymap.menu_label(&text, action);
        if icon_button(ui, can_undo, paint_undo)
            .on_hover_text(tip(t!("timeline_bar.undo"), Action::Undo))
            .clicked()
        {
            state.commands.push(TimelineCommand::Undo);
        }
        if icon_button(ui, can_redo, paint_redo)
            .on_hover_text(tip(t!("timeline_bar.redo"), Action::Redo))
            .clicked()
        {
            state.commands.push(TimelineCommand::Redo);
        }
        ui.separator();

        if icon_button(ui, true, paint_split)
            .on_hover_text(tip(t!("timeline_bar.split"), Action::Split))
            .clicked()
        {
            state.commands.push(TimelineCommand::SplitAtPlayhead);
        }
        if icon_button(ui, true, |p, c, col| paint_delete_side(p, c, col, true))
            .on_hover_text(tip(t!("timeline_bar.delete_left"), Action::DeleteLeft))
            .clicked()
        {
            state.commands.push(TimelineCommand::DeleteLeft);
        }
        if icon_button(ui, true, |p, c, col| paint_delete_side(p, c, col, false))
            .on_hover_text(tip(t!("timeline_bar.delete_right"), Action::DeleteRight))
            .clicked()
        {
            state.commands.push(TimelineCommand::DeleteRight);
        }
        if icon_button(ui, true, paint_trash)
            .on_hover_text(tip(t!("timeline_bar.delete"), Action::Delete))
            .clicked()
        {
            state.commands.push(TimelineCommand::Delete);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let mut fraction = state.zoom_fraction();
            ui.spacing_mut().slider_width = ZOOM_SLIDER_WIDTH;
            let slider = egui::Slider::new(&mut fraction, 0.0..=1.0).show_value(false);
            if ui
                .add(slider)
                .on_hover_text(keymap.menu_label(&t!("timeline_bar.zoom"), Action::ZoomIn))
                .changed()
            {
                state.set_zoom_fraction(fraction);
            }
            // Right to left: the last added is the leftmost.
            toggle(
                ui,
                &mut state.preview_axis,
                &t!("timeline_bar.preview_axis"),
                &keymap.menu_label(
                    &t!("timeline_bar.preview_axis_hint"),
                    Action::TogglePreviewAxis,
                ),
            );
            toggle(
                ui,
                &mut state.linkage,
                &t!("timeline_bar.linkage"),
                &t!("timeline_bar.linkage_hint"),
            );
            toggle(
                ui,
                snapping,
                &t!("timeline_bar.snapping"),
                &keymap.menu_label(&t!("timeline_bar.snapping_hint"), Action::ToggleSnapping),
            );
            toggle(
                ui,
                &mut state.magnet,
                &t!("timeline_bar.magnet"),
                &keymap.menu_label(&t!("timeline_bar.magnet_hint"), Action::ToggleMagnet),
            );
        });
    });
}

fn toggle(ui: &mut egui::Ui, value: &mut bool, label: &str, hint: &str) {
    if ui
        .selectable_label(*value, label)
        .on_hover_text(hint)
        .clicked()
    {
        *value = !*value;
    }
}

fn mode_menu(ui: &mut egui::Ui, state: &mut TimelineState, keymap: &Keymap) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, BUTTON_SIZE.y), egui::Sense::hover());
    paint_tool_icon(
        ui.painter(),
        rect.center(),
        state.tool,
        ui.visuals().text_color(),
    );
    egui::ComboBox::from_id_salt("timeline_mouse_mode")
        .width(150.0)
        .selected_text(tool_label(state.tool))
        .show_ui(ui, |ui| {
            for (tool, action) in TOOLS {
                let label = keymap.menu_label(&tool_label(tool), action);
                if ui.selectable_label(state.tool == tool, label).clicked() {
                    state.tool = tool;
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text(t!("timeline_bar.mode"));
}

fn icon_button(
    ui: &mut egui::Ui,
    enabled: bool,
    paint: impl FnOnce(&egui::Painter, egui::Pos2, egui::Color32),
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(BUTTON_SIZE, sense);
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if enabled && response.hovered() {
            ui.painter()
                .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        }
        let color = if enabled {
            visuals.fg_stroke.color
        } else {
            ui.visuals()
                .widgets
                .noninteractive
                .fg_stroke
                .color
                .gamma_multiply(0.5)
        };
        paint(ui.painter(), rect.center(), color);
    }
    response
}

fn stroke(color: egui::Color32) -> egui::Stroke {
    egui::Stroke::new(1.5, color)
}

fn arc(center: egui::Pos2, radius: f32, from: f32, to: f32) -> Vec<egui::Pos2> {
    (0..=12)
        .map(|i| {
            let a = from + (to - from) * (i as f32 / 12.0);
            center + egui::vec2(a.cos(), a.sin()) * radius
        })
        .collect()
}

fn arrow_head(tip: egui::Pos2, dir: egui::Vec2, size: f32, color: egui::Color32) -> egui::Shape {
    let side = egui::vec2(-dir.y, dir.x);
    egui::Shape::convex_polygon(
        vec![
            tip,
            tip - dir * size + side * (size * 0.6),
            tip - dir * size - side * (size * 0.6),
        ],
        color,
        egui::Stroke::NONE,
    )
}

fn paint_undo(p: &egui::Painter, c: egui::Pos2, color: egui::Color32) {
    use std::f32::consts::PI;
    p.add(egui::Shape::line(
        arc(c, 6.0, -PI * 0.9, PI * 0.1),
        stroke(color),
    ));
    let tip = c + egui::vec2(-6.0 * 0.95, -6.0 * 0.3);
    p.add(arrow_head(tip, egui::vec2(0.0, -1.0), 5.0, color));
}

fn paint_redo(p: &egui::Painter, c: egui::Pos2, color: egui::Color32) {
    use std::f32::consts::PI;
    p.add(egui::Shape::line(
        arc(c, 6.0, PI * 0.9, PI * -0.1 + 2.0 * PI),
        stroke(color),
    ));
    let tip = c + egui::vec2(6.0 * 0.95, -6.0 * 0.3);
    p.add(arrow_head(tip, egui::vec2(0.0, -1.0), 5.0, color));
}

/// Two pieces of clip and the cut between them.
fn paint_split(p: &egui::Painter, c: egui::Pos2, color: egui::Color32) {
    for dx in [-8.0, 2.0] {
        let r = egui::Rect::from_min_size(c + egui::vec2(dx, -4.0), egui::vec2(6.0, 8.0));
        p.rect_stroke(r, 1.0, stroke(color), egui::StrokeKind::Inside);
    }
    p.line_segment(
        [c + egui::vec2(0.0, -8.0), c + egui::vec2(0.0, 8.0)],
        egui::Stroke::new(2.0, color),
    );
}

/// The playhead with the discarded side filled and crossed.
fn paint_delete_side(p: &egui::Painter, c: egui::Pos2, color: egui::Color32, left: bool) {
    let side = if left { -1.0 } else { 1.0 };
    let gone = egui::Rect::from_two_pos(c + egui::vec2(side * 9.0, -5.0), c + egui::vec2(0.0, 5.0));
    p.rect_filled(
        gone,
        1.0,
        egui::Color32::from_rgb(200, 70, 70).gamma_multiply(0.6),
    );
    p.line_segment([gone.left_top(), gone.right_bottom()], stroke(color));
    p.line_segment([gone.right_top(), gone.left_bottom()], stroke(color));
    p.line_segment(
        [c + egui::vec2(0.0, -8.0), c + egui::vec2(0.0, 8.0)],
        egui::Stroke::new(2.0, color),
    );
}

fn paint_trash(p: &egui::Painter, c: egui::Pos2, color: egui::Color32) {
    let s = stroke(color);
    p.line_segment([c + egui::vec2(-6.0, -5.0), c + egui::vec2(6.0, -5.0)], s);
    p.line_segment([c + egui::vec2(-2.0, -7.0), c + egui::vec2(2.0, -7.0)], s);
    p.add(egui::Shape::line(
        vec![
            c + egui::vec2(-5.0, -5.0),
            c + egui::vec2(-4.0, 7.0),
            c + egui::vec2(4.0, 7.0),
            c + egui::vec2(5.0, -5.0),
        ],
        s,
    ));
    for x in [-1.5, 1.5] {
        p.line_segment(
            [c + egui::vec2(x, -2.0), c + egui::vec2(x, 4.0)],
            egui::Stroke::new(1.0, color),
        );
    }
}

pub(crate) fn paint_tool_icon(
    p: &egui::Painter,
    c: egui::Pos2,
    tool: TimelineTool,
    color: egui::Color32,
) {
    let s = stroke(color);
    match tool {
        TimelineTool::Select => {
            let tip = c + egui::vec2(-3.5, -7.0);
            let outline = [
                (0.0, 0.0),
                (0.0, 12.0),
                (3.0, 9.2),
                (5.2, 13.8),
                (7.2, 12.9),
                (5.0, 8.4),
                (8.8, 8.4),
            ]
            .map(|(x, y)| tip + egui::vec2(x, y));
            p.add(egui::Shape::closed_line(outline.to_vec(), s));
        }
        TimelineTool::Blade => {
            p.line_segment([c + egui::vec2(-6.0, 7.0), c + egui::vec2(6.0, -7.0)], s);
            p.line_segment(
                [c + egui::vec2(-1.0, 3.0), c + egui::vec2(3.0, 5.0)],
                egui::Stroke::new(2.5, color),
            );
            p.circle_stroke(
                c + egui::vec2(-5.0, -5.0),
                2.2,
                egui::Stroke::new(1.2, color),
            );
            p.line_segment([c + egui::vec2(-3.5, -3.5), c + egui::vec2(5.0, 6.0)], s);
        }
        TimelineTool::SelectLeft | TimelineTool::SelectRight => {
            let towards = if tool == TimelineTool::SelectLeft {
                1.0
            } else {
                -1.0
            };
            let x = c.x - towards * 6.0;
            p.add(egui::Shape::line(
                vec![
                    egui::pos2(x + towards * 4.0, c.y - 7.0),
                    egui::pos2(x, c.y - 7.0),
                    egui::pos2(x, c.y + 7.0),
                    egui::pos2(x + towards * 4.0, c.y + 7.0),
                ],
                egui::Stroke::new(2.0, color),
            ));
            let tip = c + egui::vec2(-towards * 8.0, 0.0);
            p.line_segment([tip, c + egui::vec2(towards * 2.0, 0.0)], s);
            p.add(arrow_head(tip, egui::vec2(-towards, 0.0), 4.0, color));
        }
        TimelineTool::Slip => {
            for (x, towards) in [(c.x - 7.0, 1.0), (c.x + 7.0, -1.0)] {
                p.add(egui::Shape::line(
                    vec![
                        egui::pos2(x + towards * 3.0, c.y - 6.0),
                        egui::pos2(x, c.y - 6.0),
                        egui::pos2(x, c.y + 6.0),
                        egui::pos2(x + towards * 3.0, c.y + 6.0),
                    ],
                    s,
                ));
            }
            p.line_segment([c - egui::vec2(3.0, 0.0), c + egui::vec2(3.0, 0.0)], s);
            for dir in [-1.0f32, 1.0] {
                p.add(arrow_head(
                    c + egui::vec2(dir * 4.5, 0.0),
                    egui::vec2(dir, 0.0),
                    3.0,
                    color,
                ));
            }
        }
    }
}

/// Camera: the track holds picture.
pub(crate) fn paint_camera_icon(p: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let c = rect.center();
    let body = egui::Rect::from_center_size(c + egui::vec2(-1.5, 0.0), egui::vec2(10.0, 8.0));
    p.rect_stroke(body, 1.5, stroke(color), egui::StrokeKind::Inside);
    p.add(egui::Shape::convex_polygon(
        vec![
            c + egui::vec2(4.0, -1.0),
            c + egui::vec2(8.0, -4.0),
            c + egui::vec2(8.0, 4.0),
            c + egui::vec2(4.0, 1.0),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

/// Music note: the track holds sound.
pub(crate) fn paint_note_icon(p: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let c = rect.center();
    p.circle_filled(c + egui::vec2(-2.5, 4.5), 2.8, color);
    p.line_segment(
        [c + egui::vec2(0.0, 4.0), c + egui::vec2(0.0, -6.0)],
        stroke(color),
    );
    p.add(egui::Shape::line(
        vec![
            c + egui::vec2(0.0, -6.0),
            c + egui::vec2(5.0, -4.0),
            c + egui::vec2(5.0, -1.0),
        ],
        stroke(color),
    ));
}

pub(crate) fn paint_text_icon(p: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "T",
        egui::FontId::proportional(15.0),
        color,
    );
}

/// A pencil.
pub(crate) fn paint_pencil_icon(p: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let c = rect.center();
    let s = stroke(color);
    let tip = c + egui::vec2(-5.5, 5.5);
    p.add(egui::Shape::closed_line(
        vec![
            tip,
            c + egui::vec2(-5.5, 2.5),
            c + egui::vec2(3.0, -6.0),
            c + egui::vec2(6.0, -3.0),
            c + egui::vec2(-2.5, 5.5),
        ],
        s,
    ));
    p.line_segment([c + egui::vec2(0.5, -3.0), c + egui::vec2(3.5, 0.0)], s);
}

/// An eye, crossed out when the layer is hidden.
pub(crate) fn paint_eye_icon(
    p: &egui::Painter,
    rect: egui::Rect,
    visible: bool,
    color: egui::Color32,
) {
    let c = rect.center();
    let s = stroke(color);
    let upper: Vec<egui::Pos2> = (0..=12)
        .map(|i| {
            let x = -7.0 + 14.0 * (i as f32 / 12.0);
            c + egui::vec2(x, -4.5 * (1.0 - (x / 7.0).powi(2)))
        })
        .collect();
    let lower: Vec<egui::Pos2> = upper
        .iter()
        .map(|pt| egui::pos2(pt.x, 2.0 * c.y - pt.y))
        .collect();
    p.add(egui::Shape::line(upper, s));
    p.add(egui::Shape::line(lower, s));
    p.circle_filled(c, 2.0, color);
    if !visible {
        p.line_segment([c + egui::vec2(-6.0, 6.0), c + egui::vec2(6.0, -6.0)], s);
    }
}

/// A speaker, crossed out when muted.
pub(crate) fn paint_speaker_icon(
    p: &egui::Painter,
    rect: egui::Rect,
    audible: bool,
    color: egui::Color32,
) {
    use std::f32::consts::PI;
    let c = rect.center();
    let s = stroke(color);
    p.add(egui::Shape::convex_polygon(
        vec![
            c + egui::vec2(-7.0, -2.5),
            c + egui::vec2(-4.0, -2.5),
            c + egui::vec2(0.0, -6.0),
            c + egui::vec2(0.0, 6.0),
            c + egui::vec2(-4.0, 2.5),
            c + egui::vec2(-7.0, 2.5),
        ],
        color,
        egui::Stroke::NONE,
    ));
    if audible {
        p.add(egui::Shape::line(
            arc(c + egui::vec2(0.0, 0.0), 4.0, -PI * 0.28, PI * 0.28),
            s,
        ));
        p.add(egui::Shape::line(
            arc(c + egui::vec2(0.0, 0.0), 7.0, -PI * 0.28, PI * 0.28),
            s,
        ));
    } else {
        p.line_segment([c + egui::vec2(2.5, -3.0), c + egui::vec2(7.5, 3.0)], s);
        p.line_segment([c + egui::vec2(2.5, 3.0), c + egui::vec2(7.5, -3.0)], s);
    }
}

/// Thick bracket on a clip edge while the pointer is over its trim handle.
pub(crate) fn paint_trim_bracket(p: &egui::Painter, clip_rect: egui::Rect, start: bool) {
    const TICK: f32 = 5.0;
    let towards = if start { 1.0 } else { -1.0 };
    let x = if start {
        clip_rect.left() + 1.0
    } else {
        clip_rect.right() - 1.0
    };
    let (top, bottom) = (clip_rect.top() + 1.0, clip_rect.bottom() - 1.0);
    let points = vec![
        egui::pos2(x + towards * TICK, top),
        egui::pos2(x, top),
        egui::pos2(x, bottom),
        egui::pos2(x + towards * TICK, bottom),
    ];
    p.add(egui::Shape::line(
        points.clone(),
        egui::Stroke::new(6.0, egui::Color32::BLACK),
    ));
    p.add(egui::Shape::line(
        points,
        egui::Stroke::new(4.0, egui::Color32::WHITE),
    ));
}

/// What the timeline shows before the first media: ruler, an empty Main
/// Track and the hint. A media dragged in is returned.
pub(crate) fn show_empty(ui: &mut egui::Ui, header_width: f32) -> Option<TimelineDrag> {
    let rect = ui.available_rect_before_wrap();
    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    let muted = ui.visuals().widgets.noninteractive.fg_stroke.color;

    let ruler = egui::Rect::from_min_size(
        rect.min + egui::vec2(header_width, 0.0),
        egui::vec2(rect.width() - header_width, 20.0),
    );
    painter.rect_filled(
        ruler,
        0.0,
        if dark {
            egui::Color32::from_gray(38)
        } else {
            egui::Color32::from_gray(225)
        },
    );
    let mut x = ruler.left();
    let mut second = 0;
    while x < ruler.right() {
        painter.line_segment(
            [
                egui::pos2(x, ruler.bottom() - 6.0),
                egui::pos2(x, ruler.bottom()),
            ],
            egui::Stroke::new(1.0, muted),
        );
        painter.text(
            egui::pos2(x + 3.0, ruler.top() + 2.0),
            egui::Align2::LEFT_TOP,
            format!("{second}s"),
            egui::FontId::proportional(10.0),
            muted,
        );
        x += 60.0 * 5.0;
        second += 5;
    }

    let track_height = 40.0;
    let free = (rect.height() - ruler.height()).max(track_height);
    let track = egui::Rect::from_min_size(
        egui::pos2(
            rect.left(),
            ruler.bottom() + ((free - track_height) / 2.0).floor(),
        ),
        egui::vec2(rect.width(), track_height),
    );
    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(track.left() + header_width, track.top()),
            track.max,
        ),
        0.0,
        if dark {
            egui::Color32::from_gray(32)
        } else {
            egui::Color32::from_gray(238)
        },
    );
    let header = egui::Rect::from_min_size(track.min, egui::vec2(header_width, track_height));
    paint_camera_icon(
        &painter,
        egui::Rect::from_min_size(header.min + egui::vec2(2.0, 11.0), egui::vec2(20.0, 18.0)),
        muted,
    );
    painter.text(
        header.min + egui::vec2(26.0, track_height / 2.0),
        egui::Align2::LEFT_CENTER,
        t!("timeline.main_track"),
        egui::FontId::proportional(12.0),
        muted,
    );

    let drop_zone = egui::Rect::from_min_max(
        egui::pos2(rect.left() + header_width, ruler.bottom()),
        rect.max,
    );
    let response = ui.interact(
        drop_zone,
        ui.id().with("timeline_drop_zone_empty"),
        egui::Sense::hover(),
    );
    if TimelineDrag::hovered(&response).is_some() {
        let accent = crate::theme::ACCENT;
        painter.rect_filled(track, 0.0, accent.gamma_multiply(0.2));
        painter.rect_stroke(
            track,
            0.0,
            egui::Stroke::new(2.0, accent),
            egui::StrokeKind::Inside,
        );
    }
    painter.text(
        egui::pos2(drop_zone.center().x, track.center().y),
        egui::Align2::CENTER_CENTER,
        t!("timeline.empty_hint"),
        egui::FontId::proportional(14.0),
        muted.gamma_multiply(0.7),
    );
    ui.allocate_rect(rect, egui::Sense::hover());
    TimelineDrag::released(&response)
}
