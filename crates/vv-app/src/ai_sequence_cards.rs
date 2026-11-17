//! Cards of the infographic panel and of the AI properties section.
use crate::ai_sequence::Improvement;
use crate::theme;
use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

pub(crate) type Art = fn(&Painter, Rect);

const ART_HEIGHT: f32 = 44.0;

pub(crate) fn section_title(ui: &mut egui::Ui, number: &str, text: &str) {
    ui.add_space(theme::SPACE_2);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(number).color(theme::ACCENT).strong());
        ui.label(egui::RichText::new(text).strong().size(theme::FONT_SECTION));
    });
}

/// Width of one of three cards laid out in a row.
pub(crate) fn card_width(ui: &egui::Ui) -> f32 {
    let gap = ui.spacing().item_spacing.x;
    ((ui.available_width() - gap * 2.0) / 3.0).floor().max(40.0)
}

pub(crate) fn choice_card(
    ui: &mut egui::Ui,
    width: f32,
    selected: bool,
    label: &str,
    art: Art,
) -> egui::Response {
    let pad = 5.0;
    let text_color = if selected {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_SECONDARY
    };
    let text = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(theme::FONT_SMALL),
        text_color,
        (width - pad * 2.0).max(10.0),
    );
    let text_height = text.size().y;
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, pad * 3.0 + ART_HEIGHT + text_height),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hovered = response.hovered();
    let fill = if response.is_pointer_button_down_on() {
        theme::BG_SELECTED
    } else if hovered {
        theme::BG_RAISED_HOVER
    } else {
        theme::BG_RAISED
    };
    let stroke = if selected {
        Stroke::new(1.5, theme::ACCENT)
    } else if hovered {
        Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.5))
    } else {
        Stroke::new(1.0, theme::BORDER_SUBTLE)
    };
    let painter = ui.painter();
    painter.rect(rect, theme::RADIUS_PANEL, fill, stroke, StrokeKind::Inside);
    art(
        painter,
        Rect::from_min_size(
            rect.min + vec2(pad, pad),
            vec2((width - pad * 2.0).max(1.0), ART_HEIGHT),
        ),
    );
    painter.galley(
        pos2(rect.left() + pad, rect.top() + pad * 2.0 + ART_HEIGHT),
        text,
        text_color,
    );
    response
}

pub(crate) fn art_simple(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 3.0, Color32::from_gray(0xee));
    let x = rect.left() + rect.width() * 0.12;
    painter.rect_filled(
        Rect::from_min_size(
            pos2(x, rect.top() + rect.height() * 0.2),
            vec2(rect.width() * 0.45, 4.0),
        ),
        2.0,
        Color32::from_gray(0x22),
    );
    for (index, fraction) in [0.7_f32, 0.55].into_iter().enumerate() {
        painter.rect_filled(
            Rect::from_min_size(
                pos2(x, rect.top() + rect.height() * 0.48 + index as f32 * 8.0),
                vec2(rect.width() * fraction, 2.5),
            ),
            1.0,
            Color32::from_gray(0x99),
        );
    }
}

pub(crate) fn art_modern(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 3.0, Color32::from_rgb(0x12, 0x1f, 0x24));
    let base = rect.bottom() - 6.0;
    let bar_width = rect.width() * 0.14;
    for (index, height) in [0.35_f32, 0.6, 0.85].into_iter().enumerate() {
        let x = rect.left() + rect.width() * 0.2 + index as f32 * (bar_width + rect.width() * 0.08);
        let bar = (rect.height() - 12.0) * height;
        painter.rect_filled(
            Rect::from_min_max(pos2(x, base - bar), pos2(x + bar_width, base)),
            2.0,
            theme::ACCENT.gamma_multiply(0.5 + height * 0.5),
        );
    }
    painter.circle_filled(
        pos2(rect.right() - 10.0, rect.top() + 10.0),
        3.5,
        theme::ACCENT,
    );
}

pub(crate) fn art_vivid(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 3.0, Color32::from_rgb(0x5b, 0x2a, 0xa8));
    painter.circle_filled(
        pos2(rect.left() + rect.width() * 0.28, rect.center().y),
        rect.height() * 0.28,
        Color32::from_rgb(0xff, 0x8a, 0x3d),
    );
    painter.rect_filled(
        Rect::from_min_size(
            pos2(
                rect.left() + rect.width() * 0.5,
                rect.top() + rect.height() * 0.28,
            ),
            vec2(rect.width() * 0.38, 7.0),
        ),
        3.5,
        Color32::from_rgb(0xff, 0x4f, 0x9a),
    );
    painter.rect_filled(
        Rect::from_min_size(
            pos2(
                rect.left() + rect.width() * 0.5,
                rect.top() + rect.height() * 0.56,
            ),
            vec2(rect.width() * 0.26, 7.0),
        ),
        3.5,
        Color32::from_rgb(0xff, 0xd2, 0x3f),
    );
}

fn person(painter: &Painter, rect: Rect, x: f32) {
    let color = Color32::from_gray(0x78);
    painter.circle_filled(pos2(x, rect.top() + 14.0), 5.0, color);
    painter.rect_filled(
        Rect::from_center_size(pos2(x, rect.top() + 31.0), vec2(18.0, 12.0)),
        5.0,
        color,
    );
}

pub(crate) fn art_overlay(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 3.0, Color32::from_gray(0x2e));
    person(painter, rect, rect.center().x);
    painter.rect_filled(
        Rect::from_min_size(
            pos2(rect.left() + rect.width() * 0.18, rect.bottom() - 13.0),
            vec2(rect.width() * 0.64, 8.0),
        ),
        4.0,
        theme::ACCENT,
    );
}

pub(crate) fn art_side(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 3.0, Color32::from_gray(0x2e));
    person(painter, rect, rect.left() + rect.width() * 0.25);
    let panel = Rect::from_min_max(
        pos2(rect.left() + rect.width() * 0.52, rect.top() + 4.0),
        pos2(rect.right() - 4.0, rect.bottom() - 4.0),
    );
    painter.rect_filled(panel, 3.0, theme::ACCENT_FILL);
    for row in 0..3 {
        painter.rect_filled(
            Rect::from_min_size(
                pos2(panel.left() + 4.0, panel.top() + 6.0 + row as f32 * 10.0),
                vec2(
                    (panel.width() - 8.0) * if row == 0 { 0.9 } else { 0.6 },
                    3.0,
                ),
            ),
            1.5,
            Color32::from_white_alpha(200),
        );
    }
}

pub(crate) fn art_fullscreen(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 3.0, theme::ACCENT_FILL);
    let base = rect.bottom() - 7.0;
    for (index, height) in [0.4_f32, 0.65, 0.9].into_iter().enumerate() {
        let x = rect.left() + rect.width() * 0.25 + index as f32 * rect.width() * 0.2;
        painter.rect_filled(
            Rect::from_min_max(
                pos2(x, base - (rect.height() - 14.0) * height),
                pos2(x + rect.width() * 0.12, base),
            ),
            2.0,
            Color32::from_white_alpha(210),
        );
    }
}

enum Glyph {
    Sparkle,
    Motion,
    Text,
    Color,
}

fn glyph_for(idea: &Improvement) -> Glyph {
    let text = format!("{} {}", idea.title, idea.instruction).to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| text.contains(*word));
    if has(&[
        "anim",
        "appear",
        "one by one",
        "compar",
        "fluid",
        "motion",
        "movimento",
        "uno alla",
    ]) {
        Glyph::Motion
    } else if has(&[
        "color",
        "colour",
        "background",
        "contrast",
        "sfond",
        "colore",
        "palette",
        "tema",
    ]) {
        Glyph::Color
    } else if has(&[
        "text", "title", "font", "bigger", "larger", "readab", "testo", "titolo", "ingrand", "legg",
    ]) {
        Glyph::Text
    } else {
        Glyph::Sparkle
    }
}

fn paint_glyph(painter: &Painter, center: Pos2, glyph: &Glyph, color: Color32) {
    let stroke = Stroke::new(1.4, color);
    match glyph {
        Glyph::Sparkle => {
            for (dx, dy) in [(5.5, 0.0), (0.0, 5.5)] {
                painter.line_segment([center - vec2(dx, dy), center + vec2(dx, dy)], stroke);
            }
            for (dx, dy) in [(3.0, 3.0), (3.0, -3.0)] {
                painter.line_segment(
                    [center - vec2(dx, dy) * 0.6, center + vec2(dx, dy) * 0.6],
                    stroke,
                );
            }
        }
        Glyph::Motion => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + vec2(-3.0, -5.0),
                    center + vec2(-3.0, 5.0),
                    center + vec2(5.0, 0.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Glyph::Text => {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "Aa",
                egui::FontId::proportional(11.0),
                color,
            );
        }
        Glyph::Color => {
            painter.circle_stroke(center, 5.0, stroke);
            painter.circle_filled(center + vec2(2.0, 0.0), 2.6, color);
        }
    }
}

/// A suggested edit: clicking it is the whole interaction.
pub(crate) fn improvement_card(
    ui: &mut egui::Ui,
    idea: &Improvement,
    enabled: bool,
) -> egui::Response {
    let pad = theme::SPACE_2;
    let badge = 26.0;
    let text_width = (ui.available_width() - pad * 3.0 - badge).max(40.0);
    let title = ui.painter().layout(
        idea.title.clone(),
        egui::FontId::proportional(theme::FONT_SECTION),
        if enabled {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_DISABLED
        },
        text_width,
    );
    let body = ui.painter().layout(
        idea.description.clone(),
        egui::FontId::proportional(theme::FONT_BODY),
        if enabled {
            theme::TEXT_SECONDARY
        } else {
            theme::TEXT_DISABLED
        },
        text_width,
    );
    let title_height = title.size().y;
    let height = (pad * 2.0 + title_height + 2.0 + body.size().y).max(pad * 2.0 + badge);
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), height),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();
    let fill = if pressed {
        theme::ACCENT_FILL.gamma_multiply(0.6)
    } else if hovered {
        theme::BG_RAISED_HOVER
    } else {
        theme::BG_RAISED
    };
    let stroke = if hovered || pressed {
        Stroke::new(1.0, theme::ACCENT)
    } else {
        Stroke::new(1.0, theme::BORDER_SUBTLE)
    };
    let glyph = glyph_for(idea);
    let painter = ui.painter();
    painter.rect(rect, theme::RADIUS_PANEL, fill, stroke, StrokeKind::Inside);
    let center = pos2(
        rect.left() + pad + badge / 2.0,
        rect.top() + pad + badge / 2.0,
    );
    let accent = if enabled {
        theme::ACCENT
    } else {
        theme::TEXT_DISABLED
    };
    painter.circle_filled(
        center,
        badge / 2.0,
        accent.gamma_multiply(if hovered { 0.35 } else { 0.18 }),
    );
    paint_glyph(painter, center, &glyph, accent);
    let text_x = rect.left() + pad * 2.0 + badge;
    painter.galley(pos2(text_x, rect.top() + pad), title, theme::TEXT_PRIMARY);
    painter.galley(
        pos2(text_x, rect.top() + pad + title_height + 2.0),
        body,
        theme::TEXT_SECONDARY,
    );
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}
