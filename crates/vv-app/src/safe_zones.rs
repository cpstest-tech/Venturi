//! Social safe zones over the player: a see-through mock of the interface
//! TikTok, Reels and Shorts lay over the video. Painted by the UI only, so they never reach
//! the export. Zones come from `assets/safe_zones.json`, or from a
//! `safe_zones.json` next to `settings.json` when that one reads.

use std::path::PathBuf;

use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};
use serde::Deserialize;

use crate::theme;

const BUNDLED: &str = include_str!("../assets/safe_zones.json");
const VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Tabs,
    Header,
    TopIcons,
    SideButtons,
    Caption,
    Progress,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Zone {
    pub kind: Kind,
    /// Buttons in a `side_buttons` column.
    #[serde(default = "default_count")]
    pub count: u8,
    /// A Subscribe button next to the channel name in a `caption`.
    #[serde(default)]
    pub button: bool,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn default_count() -> u8 {
    5
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Platform {
    pub id: String,
    pub name: String,
    pub zones: Vec<Zone>,
}

#[derive(Deserialize)]
struct File {
    version: u32,
    platforms: Vec<Platform>,
}

impl Zone {
    /// [left, top, right, bottom] as fractions of the frame, top-left origin.
    pub fn fractions(&self) -> [f32; 4] {
        let c = |v: f32| v.clamp(0.0, 100.0) / 100.0;
        [
            c(self.x),
            c(self.y),
            c(self.x + self.w.max(0.0)),
            c(self.y + self.h.max(0.0)),
        ]
    }
}

pub(crate) fn parse(text: &str) -> Result<Vec<Platform>, String> {
    let file: File = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if file.version != VERSION {
        return Err(format!("version {} (expected {VERSION})", file.version));
    }
    if file.platforms.is_empty() {
        return Err("no platforms".into());
    }
    Ok(file.platforms)
}

pub(crate) fn user_file() -> Option<PathBuf> {
    Some(
        crate::settings::Settings::default_path()?
            .parent()?
            .join("safe_zones.json"),
    )
}

/// The user's file if it reads, else the bundled one; the error says why
/// the user's file was ignored.
pub(crate) fn load() -> (Vec<Platform>, Option<String>) {
    let bundled = || parse(BUNDLED).expect("bundled safe_zones.json");
    match user_file().and_then(|p| std::fs::read_to_string(p).ok()) {
        None => (bundled(), None),
        Some(text) => match parse(&text) {
            Ok(platforms) => (platforms, None),
            Err(e) => (bundled(), Some(e)),
        },
    }
}

pub(crate) fn platform<'a>(platforms: &'a [Platform], id: &str) -> Option<&'a Platform> {
    platforms
        .iter()
        .find(|p| p.id == id)
        .or_else(|| platforms.first())
}

fn screen_rect(frame: Rect, [l, t, r, b]: [f32; 4]) -> Rect {
    Rect::from_min_max(frame.lerp_inside(vec2(l, t)), frame.lerp_inside(vec2(r, b)))
}

/// `texts`: frame fractions [left, top, right, bottom] of the text on screen.
pub(crate) fn paint(painter: &Painter, frame: Rect, platform: &Platform, texts: &[[f32; 4]]) {
    // One pixel of a 1080x1920 frame, so the mock keeps its proportions.
    let u = (frame.width() / 1080.0).min(frame.height() / 1920.0);
    for zone in &platform.zones {
        paint_zone(painter, screen_rect(frame, zone.fractions()), zone, u);
    }
    let font = egui::FontId::proportional(theme::FONT_SMALL);
    for text in texts {
        if !overlaps(platform, *text) {
            continue;
        }
        let rect = screen_rect(frame, *text);
        painter.rect_stroke(
            rect,
            theme::RADIUS_TAG,
            Stroke::new(2.0, theme::WARNING),
            egui::StrokeKind::Outside,
        );
        painter.text(
            rect.center_top() - vec2(0.0, 4.0),
            egui::Align2::CENTER_BOTTOM,
            t!("player.safe_zone_warning"),
            font.clone(),
            theme::WARNING,
        );
    }
}

const GHOST: Color32 = Color32::from_rgba_premultiplied(100, 100, 100, 100);
const GHOST_STRONG: Color32 = Color32::from_rgba_premultiplied(150, 150, 150, 150);

fn pill(painter: &Painter, left_center: Pos2, w: f32, h: f32, color: Color32) {
    let rect = Rect::from_min_size(left_center - vec2(0.0, h / 2.0), vec2(w, h));
    painter.rect_filled(rect, h / 2.0, color);
}

fn magnifier(painter: &Painter, c: Pos2, u: f32) {
    let stroke = Stroke::new(6.0 * u, GHOST);
    painter.circle_stroke(c, 16.0 * u, stroke);
    painter.line_segment([c + vec2(12.0, 12.0) * u, c + vec2(26.0, 26.0) * u], stroke);
}

fn paint_zone(painter: &Painter, r: Rect, zone: &Zone, u: f32) {
    let cy = r.center().y;
    match zone.kind {
        Kind::Tabs => {
            let (a, b, gap) = (170.0 * u, 140.0 * u, 50.0 * u);
            let x = r.center().x - (a + gap + b) / 2.0;
            pill(painter, pos2(x, cy), a, 28.0 * u, GHOST);
            pill(painter, pos2(x + a + gap, cy), b, 28.0 * u, GHOST_STRONG);
            let under = pos2(x + a + gap + b / 2.0 - 30.0 * u, cy + 30.0 * u);
            pill(painter, under, 60.0 * u, 6.0 * u, GHOST_STRONG);
            pill(
                painter,
                pos2(r.left() + 40.0 * u, cy),
                70.0 * u,
                34.0 * u,
                GHOST,
            );
            magnifier(painter, pos2(r.right() - 70.0 * u, cy - 4.0 * u), u);
        }
        Kind::Header => {
            pill(
                painter,
                pos2(r.left() + 45.0 * u, cy),
                130.0 * u,
                36.0 * u,
                GHOST_STRONG,
            );
            let camera =
                Rect::from_center_size(pos2(r.right() - 75.0 * u, cy), vec2(52.0, 40.0) * u);
            let stroke = Stroke::new(6.0 * u, GHOST);
            painter.rect_stroke(camera, 10.0 * u, stroke, egui::StrokeKind::Inside);
            painter.circle_stroke(camera.center(), 9.0 * u, stroke);
        }
        Kind::TopIcons => {
            magnifier(painter, pos2(r.right() - 170.0 * u, cy - 4.0 * u), u);
            for dy in [-16.0, 0.0, 16.0] {
                painter.circle_filled(pos2(r.right() - 60.0 * u, cy + dy * u), 5.0 * u, GHOST);
            }
        }
        Kind::SideButtons => {
            let n = zone.count.max(1);
            let slot = r.height() / f32::from(n);
            let x = r.center().x;
            for i in 0..n {
                let top = r.top() + slot * f32::from(i);
                if i == 0 {
                    painter.circle_filled(pos2(x, top + slot * 0.45), 42.0 * u, GHOST);
                    continue;
                }
                painter.circle_filled(pos2(x, top + slot * 0.38), 30.0 * u, GHOST);
                if i + 1 < n {
                    pill(
                        painter,
                        pos2(x - 25.0 * u, top + slot * 0.38 + 50.0 * u),
                        50.0 * u,
                        14.0 * u,
                        GHOST,
                    );
                }
            }
        }
        Kind::Caption => {
            let y = |f: f32| r.top() + r.height() * f;
            let name = pos2(r.left(), y(0.15));
            if zone.button {
                painter.circle_filled(name + vec2(28.0 * u, 0.0), 28.0 * u, GHOST);
                pill(
                    painter,
                    name + vec2(72.0 * u, 0.0),
                    200.0 * u,
                    28.0 * u,
                    GHOST_STRONG,
                );
                pill(
                    painter,
                    name + vec2(290.0 * u, 0.0),
                    170.0 * u,
                    54.0 * u,
                    GHOST_STRONG,
                );
            } else {
                pill(painter, name, 240.0 * u, 30.0 * u, GHOST_STRONG);
            }
            pill(
                painter,
                pos2(r.left(), y(0.45)),
                r.width() * 0.9,
                22.0 * u,
                GHOST,
            );
            pill(
                painter,
                pos2(r.left(), y(0.65)),
                r.width() * 0.6,
                22.0 * u,
                GHOST,
            );
            painter.circle_filled(pos2(r.left() + 12.0 * u, y(0.88)), 12.0 * u, GHOST);
            pill(
                painter,
                pos2(r.left() + 36.0 * u, y(0.88)),
                300.0 * u,
                20.0 * u,
                GHOST,
            );
        }
        Kind::Progress => {
            pill(painter, pos2(r.left(), cy), r.width(), 4.0 * u, GHOST);
            pill(
                painter,
                pos2(r.left(), cy),
                r.width() * 0.35,
                4.0 * u,
                GHOST_STRONG,
            );
        }
    }
}

/// The platform's mark in a 16-point square; false for an unknown id.
pub(crate) fn paint_icon(painter: &Painter, c: Pos2, id: &str, color: Color32) -> bool {
    let stroke = Stroke::new(theme::ICON_STROKE, color);
    match id {
        "tiktok" => {
            let stem = c + vec2(1.0, 0.0);
            painter.circle_stroke(c + vec2(-2.2, 3.6), 2.6, stroke);
            painter.line_segment([stem + vec2(-0.6, 3.6), stem + vec2(-0.6, -6.5)], stroke);
            painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
                [
                    stem + vec2(-0.6, -6.5),
                    stem + vec2(0.6, -2.6),
                    stem + vec2(5.0, -2.4),
                ],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        "reels" => {
            let body = Rect::from_center_size(c, vec2(13.0, 13.0));
            painter.rect_stroke(body, 4.0, stroke, egui::StrokeKind::Inside);
            painter.circle_stroke(c, 3.0, stroke);
            painter.circle_filled(c + vec2(3.6, -3.6), 0.9, color);
        }
        "shorts" => {
            let body = Rect::from_center_size(c, vec2(15.0, 11.0));
            painter.rect_stroke(body, 3.5, stroke, egui::StrokeKind::Inside);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + vec2(-2.0, -3.0),
                    c + vec2(3.2, 0.0),
                    c + vec2(-2.0, 3.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
        _ => return false,
    }
    true
}

pub(crate) fn overlaps(platform: &Platform, [l, t, r, b]: [f32; 4]) -> bool {
    platform.zones.iter().any(|z| {
        let [zl, zt, zr, zb] = z.fractions();
        l < zr && r > zl && t < zb && b > zt
    })
}

#[cfg(test)]
#[path = "tests/safe_zones.rs"]
mod tests;
