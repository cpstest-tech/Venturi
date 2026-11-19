//! Canvas section of the Video tab: what fills the frame behind a clip.

use super::*;
use crate::properties_panel::section_header;
use vv_core::{CANVAS_BLUR_RADII, Canvas, Rgba};

const SWATCHES: [Rgba; 8] = [
    Rgba::BLACK,
    Rgba::WHITE,
    Rgba {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 1.0,
    },
    Rgba {
        r: 0.9,
        g: 0.25,
        b: 0.25,
        a: 1.0,
    },
    Rgba {
        r: 0.98,
        g: 0.7,
        b: 0.2,
        a: 1.0,
    },
    Rgba {
        r: 0.25,
        g: 0.7,
        b: 0.4,
        a: 1.0,
    },
    Rgba {
        r: 0.2,
        g: 0.45,
        b: 0.9,
        a: 1.0,
    },
    Rgba {
        r: 0.6,
        g: 0.3,
        b: 0.85,
        a: 1.0,
    },
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    None,
    Blur,
    Color,
    Image,
}

fn kind_of(canvas: Option<&Canvas>) -> Kind {
    match canvas {
        None => Kind::None,
        Some(Canvas::Blur { .. }) => Kind::Blur,
        Some(Canvas::Color(_)) => Kind::Color,
        Some(Canvas::Image(_)) => Kind::Image,
    }
}

fn to_color32(c: Rgba) -> egui::Color32 {
    egui::Rgba::from_rgba_unmultiplied(c.r, c.g, c.b, c.a).into()
}

impl VenturiApp {
    pub(crate) fn show_canvas_section(
        &mut self,
        ui: &mut egui::Ui,
        targets: &[PanelTarget],
        pending: &mut Vec<BoxedCommand>,
    ) {
        ui.add_space(6.0);
        let (open, _) = section_header(ui, &t!("props.canvas"), false);
        if !open {
            return;
        }
        let Some(first) = targets.first() else {
            return;
        };
        let timeline = &self.session.project.timelines[first.timeline];
        let current = timeline
            .clip(first.track_index, first.clip_id)
            .and_then(|c| c.effects.canvas.clone());
        let kind = kind_of(current.as_ref());
        let mut chosen: Option<Option<Canvas>> = None;
        let mut pick_image = false;

        ui.horizontal_wrapped(|ui| {
            for (option, label) in [
                (Kind::None, t!("props.canvas_none")),
                (Kind::Blur, t!("props.canvas_blur")),
                (Kind::Color, t!("props.canvas_color")),
                (Kind::Image, t!("props.canvas_image")),
            ] {
                if ui
                    .add(egui::Button::selectable(kind == option, label))
                    .clicked()
                    && kind != option
                {
                    match option {
                        Kind::None => chosen = Some(None),
                        Kind::Blur => {
                            chosen = Some(Some(Canvas::Blur {
                                radius: CANVAS_BLUR_RADII[1],
                            }));
                        }
                        Kind::Color => chosen = Some(Some(Canvas::Color(Rgba::BLACK))),
                        Kind::Image => pick_image = true,
                    }
                }
            }
        });

        match &current {
            Some(Canvas::Blur { radius }) => {
                ui.label(t!("props.canvas_strength"));
                ui.horizontal(|ui| {
                    for (level, &value) in CANVAS_BLUR_RADII.iter().enumerate() {
                        let selected = (radius - value).abs() < 0.5;
                        if ui
                            .add(egui::Button::selectable(selected, (level + 1).to_string()))
                            .clicked()
                        {
                            chosen = Some(Some(Canvas::Blur { radius: value }));
                        }
                    }
                });
            }
            Some(Canvas::Color(color)) => {
                ui.horizontal_wrapped(|ui| {
                    for swatch in SWATCHES {
                        let size = egui::vec2(theme::CONTROL_HEIGHT, theme::CONTROL_HEIGHT);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                        ui.painter()
                            .rect_filled(rect, theme::RADIUS_CONTROL, to_color32(swatch));
                        if swatch == *color {
                            ui.painter().rect_stroke(
                                rect,
                                theme::RADIUS_CONTROL,
                                egui::Stroke::new(2.0, theme::ACCENT),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if response.clicked() {
                            chosen = Some(Some(Canvas::Color(swatch)));
                        }
                    }
                    let mut rgba = [color.r, color.g, color.b, 1.0];
                    if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                        chosen = Some(Some(Canvas::Color(Rgba {
                            r: rgba[0],
                            g: rgba[1],
                            b: rgba[2],
                            a: 1.0,
                        })));
                    }
                });
            }
            Some(Canvas::Image(path)) => {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(vv_session::file_label(path)).truncate());
                    if ui.button(t!("props.canvas_change_image")).clicked() {
                        pick_image = true;
                    }
                });
            }
            None => {}
        }

        if current.is_some()
            && ui
                .button(t!("props.canvas_apply_all"))
                .on_hover_text(t!("props.canvas_apply_all_hint"))
                .clicked()
        {
            let track = &timeline.tracks[first.track_index];
            for clip in &track.clips {
                if clip.effects.canvas != current {
                    pending.push(Box::new(vv_core::set_clip_canvas(
                        first.timeline,
                        first.track_index,
                        clip.id,
                        current.clone(),
                    )));
                }
            }
        }

        if pick_image {
            self.pick_canvas_image(targets);
        }
        if let Some(canvas) = chosen {
            pending.extend(canvas_commands(targets, &canvas));
        }
    }

    fn pick_canvas_image(&mut self, targets: &[PanelTarget]) {
        let clips = targets
            .iter()
            .map(|t| (t.timeline, t.track_index, t.clip_id))
            .collect();
        self.spawn_file_dialog(project_io::DialogKind::CanvasImage(clips), |dlg| {
            dlg.add_filter(
                "images",
                &project_io::case_insensitive(vv_media::IMAGE_EXTENSIONS),
            )
            .pick_file()
        });
    }

    /// The picked picture becomes the canvas of the clips it was picked for.
    pub(crate) fn set_canvas_image(
        &mut self,
        clips: Vec<(TimelineId, usize, ClipId)>,
        path: PathBuf,
    ) {
        let preload = path.clone();
        std::thread::spawn(move || frame_provider::preload_canvas_image(&preload));
        let group = self.session.history.begin_group();
        for (timeline, track_index, clip_id) in clips {
            self.session.apply(Box::new(vv_core::set_clip_canvas(
                timeline,
                track_index,
                clip_id,
                Some(Canvas::Image(path.clone())),
            )));
        }
        self.session
            .history
            .end_group_as(group, vv_core::CommandLabel::Canvas);
    }
}

fn canvas_commands(targets: &[PanelTarget], canvas: &Option<Canvas>) -> Vec<BoxedCommand> {
    targets
        .iter()
        .map(|t| {
            Box::new(vv_core::set_clip_canvas(
                t.timeline,
                t.track_index,
                t.clip_id,
                canvas.clone(),
            )) as BoxedCommand
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/canvas_ui.rs"]
mod tests;
