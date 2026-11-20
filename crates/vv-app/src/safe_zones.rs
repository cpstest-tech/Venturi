//! Social safe zones over the player: the areas TikTok, Reels and Shorts
//! cover with their interface. Painted by the UI only, so they never reach
//! the export. Zones come from `assets/safe_zones.json`, or from a
//! `safe_zones.json` next to `settings.json` when that one reads.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::theme;

const BUNDLED: &str = include_str!("../assets/safe_zones.json");
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Zone {
    /// By language code; `en` is the fallback.
    pub label: BTreeMap<String, String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
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
    pub fn label(&self, lang: &str) -> &str {
        self.label
            .get(lang)
            .or_else(|| self.label.get("en"))
            .or_else(|| self.label.values().next())
            .map_or("", String::as_str)
    }

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

fn screen_rect(frame: egui::Rect, [l, t, r, b]: [f32; 4]) -> egui::Rect {
    egui::Rect::from_min_max(
        frame.lerp_inside(egui::vec2(l, t)),
        frame.lerp_inside(egui::vec2(r, b)),
    )
}

/// `texts`: frame fractions [left, top, right, bottom] of the text on screen.
pub(crate) fn paint(
    painter: &egui::Painter,
    frame: egui::Rect,
    platform: &Platform,
    texts: &[[f32; 4]],
) {
    let lang = rust_i18n::locale().to_string();
    let font = egui::FontId::proportional(theme::FONT_SMALL);
    for zone in &platform.zones {
        let rect = screen_rect(frame, zone.fractions());
        painter.rect_filled(rect, 0.0, theme::ERROR.gamma_multiply(0.18));
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, theme::ERROR.gamma_multiply(0.7)),
            egui::StrokeKind::Inside,
        );
        if rect.height() > 14.0 && rect.width() > 40.0 {
            painter.text(
                rect.left_top() + egui::vec2(4.0, 3.0),
                egui::Align2::LEFT_TOP,
                zone.label(&lang),
                font.clone(),
                theme::TEXT_PRIMARY,
            );
        }
    }
    for text in texts {
        if !overlaps(platform, *text) {
            continue;
        }
        let rect = screen_rect(frame, *text);
        painter.rect_stroke(
            rect,
            theme::RADIUS_TAG,
            egui::Stroke::new(2.0, theme::WARNING),
            egui::StrokeKind::Outside,
        );
        painter.text(
            rect.center_top() - egui::vec2(0.0, 4.0),
            egui::Align2::CENTER_BOTTOM,
            t!("player.safe_zone_warning"),
            font.clone(),
            theme::WARNING,
        );
    }
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
