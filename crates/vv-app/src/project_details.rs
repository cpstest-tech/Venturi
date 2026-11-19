//! «Dettagli» panel: shown in Properties when nothing is selected.

use super::*;
use crate::first_media::Ratio;

const LABEL_WIDTH: f32 = 130.0;
const SHORT_SIDES: [(u32, &str); 4] = [(720, "720p"), (1080, "1080p"), (1440, "2K"), (2160, "4K")];

/// `size` scaled to `short` on its shorter side, aspect kept, sides even.
pub(crate) fn with_short_side(size: (u32, u32), short: u32) -> (u32, u32) {
    let current = size.0.min(size.1).max(1);
    let scale = |v: u32| {
        (((u64::from(v) * u64::from(short) + u64::from(current) / 2) / u64::from(current)) as u32
            & !1)
            .max(2)
    };
    (scale(size.0), scale(size.1))
}

impl VenturiApp {
    pub(crate) fn show_project_details(&mut self, ui: &mut egui::Ui, timeline_id: TimelineId) {
        let timeline = &self.session.project.timelines[timeline_id];
        let (resolution, fps) = (timeline.resolution, timeline.fps);
        let ratio = Ratio::of(resolution);
        let name = self
            .session
            .path()
            .map(projects::project_name)
            .unwrap_or_else(|| timeline.name.clone());
        let path = self
            .session
            .path()
            .and_then(|p| p.parent())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "–".to_owned());
        let ratio_text = ratio
            .label()
            .map_or_else(|| t!("player.original").into_owned(), str::to_owned);

        ui.label(
            egui::RichText::new(t!("details.title"))
                .size(theme::FONT_BODY + 1.0)
                .strong(),
        );
        ui.add_space(theme::SPACE_2);
        let row = |ui: &mut egui::Ui, label: String, value: String| {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [LABEL_WIDTH, 18.0],
                    egui::Label::new(
                        egui::RichText::new(label)
                            .size(theme::FONT_SMALL)
                            .color(theme::TEXT_SECONDARY),
                    ),
                );
                ui.add(egui::Label::new(value).truncate());
            });
        };
        row(ui, t!("details.name").into_owned(), name);
        row(ui, t!("details.path").into_owned(), path);
        row(ui, t!("details.ratio").into_owned(), ratio_text);
        row(
            ui,
            t!("details.resolution").into_owned(),
            format!("{}x{}", resolution.0, resolution.1),
        );
        row(
            ui,
            t!("details.frame_rate").into_owned(),
            format!("{:.2} fps", fps.as_f64()),
        );
        row(
            ui,
            t!("details.color_space").into_owned(),
            "Rec.709".to_owned(),
        );

        if self.details_editing {
            ui.add_space(theme::SPACE_3);
            ui.separator();
            ui.label(t!("details.ratio"));
            ui.horizontal_wrapped(|ui| {
                for choice in Ratio::MENU {
                    let label = choice
                        .label()
                        .map_or_else(|| t!("player.original").into_owned(), str::to_owned);
                    if ui
                        .add(egui::Button::selectable(ratio == choice, label))
                        .clicked()
                    {
                        self.set_project_ratio(choice);
                    }
                }
            });
            ui.add_space(theme::SPACE_2);
            ui.label(t!("details.resolution"));
            ui.horizontal_wrapped(|ui| {
                for (short, label) in SHORT_SIDES {
                    let selected = resolution.0.min(resolution.1) == short;
                    if ui.add(egui::Button::selectable(selected, label)).clicked() {
                        self.set_project_resolution(with_short_side(resolution, short));
                    }
                }
            });
            ui.add_space(theme::SPACE_2);
            ui.label(t!("details.frame_rate"));
            ui.horizontal_wrapped(|ui| {
                for choice in first_media::STANDARD_FPS {
                    let choice = vv_core::Rational::new(choice, 1);
                    if ui
                        .add(egui::Button::selectable(
                            fps == choice,
                            choice.num.to_string(),
                        ))
                        .clicked()
                    {
                        self.set_project_fps(choice);
                    }
                }
            });
        }

        ui.add_space(theme::SPACE_3);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if self.details_editing {
                t!("details.done")
            } else {
                t!("details.edit")
            };
            if ui.button(label).clicked() {
                self.details_editing = !self.details_editing;
            }
        });
    }
}

#[cfg(test)]
#[path = "tests/project_details.rs"]
mod tests;
