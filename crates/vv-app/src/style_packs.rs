//! Style packs in the app: the built-in ones, the user's library (one JSON
//! file each in `style-packs/` next to the settings) and applying a pack to
//! the open edit as one undo step, previewed before it is confirmed.
//!
//! Applying never overwrites what the user made: clips that already have an
//! animation, a transition or an animated volume keep it, and auto zooms
//! skip the spans holding hand-made keyframes.

use std::collections::BTreeSet;
use std::path::PathBuf;

use vv_core::style_pack::{
    self, Animations, Bumper, Component, Music, PackError, SafeZone, StylePack, SubtitleStyle,
    TransitionStyle,
};
use vv_core::{ClipId, ClipSource, CommandLabel, FrameIdx, TimelineId, TrackKind};
use vv_render::assets::AssetKind;

use crate::VenturiApp;
use crate::timeline_ui::MediaDropTarget;

/// What applying one component did.
pub(crate) enum Outcome {
    Applied(String),
    Skipped(String),
}

pub(crate) struct Entry {
    pub pack: StylePack,
    /// `None` for a built-in pack: it cannot be renamed, edited or deleted.
    pub path: Option<PathBuf>,
}

pub(crate) struct Preview {
    pub pack: String,
    pub generation: u64,
    /// The history moved: Annulla has a step to undo.
    applied: bool,
    pub report: Vec<(Component, Outcome)>,
    safe_zone_before: (bool, String),
    subtitle_template_before: Option<String>,
}

#[derive(Default)]
pub(crate) struct State {
    pub entries: Vec<Entry>,
    loaded: bool,
    pub load_errors: Vec<String>,
    pub selected: Option<String>,
    /// Components ticked for the selected pack.
    pub choice: BTreeSet<Component>,
    pub preview: Option<Preview>,
    /// The checkboxes changed during a preview: applied again on release.
    pub dirty: bool,
    pub rename: Option<(String, String)>,
    pub confirm_delete: Option<String>,
    /// A copy of the user pack being edited.
    pub editor: Option<StylePack>,
    /// `(is_error, text)` under the toolbar.
    pub message: Option<(bool, String)>,
}

/// The packs that ship with the app. Each is plain data: copy one, change
/// the ids (templates and assets listed in `docs/ASSETS.md` and in
/// `assets/text-templates`) and add it here to extend the list.
pub(crate) fn builtin_packs() -> Vec<StylePack> {
    use vv_core::auto_zoom::{Config as Zoom, Frequency, Speed};
    vec![
        // «Pulito»: interviews and tutorials. Classic line captions on a
        // dark box, gentle rare zooms, plain dissolves, music well below
        // the voice. Nothing moves that does not need to.
        StylePack {
            description: t!("style_pack.builtin_clean_desc").into_owned(),
            subtitles: Some(SubtitleStyle {
                template: "line_classic".into(),
            }),
            auto_zoom: Some(Zoom {
                intensity: 1.08,
                speed: Speed::Slow,
                frequency: Frequency::Low,
                min_gap: Frequency::Low.default_gap(),
                alternate: false,
                shift: false,
            }),
            animations: Some(Animations {
                text_in: Some("fade".into()),
                text_out: Some("dissolve".into()),
                ..Animations::default()
            }),
            music: Some(Music {
                file: None,
                volume_db: -8.0,
                ducking_db: -14.0,
            }),
            transition: Some(TransitionStyle {
                asset: "transitions/dissolve".into(),
                seconds: 0.4,
            }),
            ..StylePack::new("builtin.clean", t!("style_pack.builtin_clean"))
        },
        // «Dinamico»: TikTok/Reels energy. One big word at a time, strong
        // and frequent punch-ins that also move the centre, flashes in and
        // out of every clip, a zoom transition, an opening title and the
        // TikTok safe zones on.
        StylePack {
            description: t!("style_pack.builtin_dynamic_desc").into_owned(),
            subtitles: Some(SubtitleStyle {
                template: "word_pop".into(),
            }),
            auto_zoom: Some(Zoom {
                intensity: 1.2,
                speed: Speed::Fast,
                frequency: Frequency::High,
                min_gap: Frequency::High.default_gap(),
                alternate: true,
                shift: true,
            }),
            animations: Some(Animations {
                clip_in: Some("animations/flash_in".into()),
                clip_in_secs: 0.3,
                clip_out: Some("animations/flash_out".into()),
                clip_out_secs: 0.25,
                text_in: Some("pop".into()),
                text_out: Some("fall".into()),
            }),
            music: Some(Music {
                file: None,
                volume_db: -6.0,
                ducking_db: -10.0,
            }),
            transition: Some(TransitionStyle {
                asset: "transitions/shake_zoom".into(),
                seconds: 0.5,
            }),
            intro: Some(Bumper {
                template: "hype_impact".into(),
                text: String::new(),
            }),
            safe_zone: Some(SafeZone {
                platform: "tiktok".into(),
            }),
            ..StylePack::new("builtin.dynamic", t!("style_pack.builtin_dynamic"))
        },
        // «Tech»: keynote-like product videos. Captions that build word by
        // word in mixed case, medium zooms without drift, soft slides and
        // fades, dips to black, an elegant title at both ends.
        StylePack {
            description: t!("style_pack.builtin_tech_desc").into_owned(),
            subtitles: Some(SubtitleStyle {
                template: "word_build".into(),
            }),
            auto_zoom: Some(Zoom {
                intensity: 1.1,
                speed: Speed::Normal,
                frequency: Frequency::Medium,
                min_gap: Frequency::Medium.default_gap(),
                alternate: true,
                shift: false,
            }),
            animations: Some(Animations {
                clip_in: Some("animations/motion_slide".into()),
                clip_in_secs: 0.57,
                clip_out: Some("animations/fade_out".into()),
                clip_out_secs: 0.5,
                text_in: Some("blur_in".into()),
                text_out: Some("dissolve".into()),
            }),
            music: Some(Music {
                file: None,
                volume_db: -10.0,
                ducking_db: -14.0,
            }),
            transition: Some(TransitionStyle {
                asset: "transitions/dip_black".into(),
                seconds: 0.8,
            }),
            intro: Some(Bumper {
                template: "elegant_serif".into(),
                text: String::new(),
            }),
            outro: Some(Bumper {
                template: "elegant_serif".into(),
                text: String::new(),
            }),
            safe_zone: Some(SafeZone {
                platform: "reels".into(),
            }),
            ..StylePack::new("builtin.tech", t!("style_pack.builtin_tech"))
        },
        // «Vlog»: warm and casual. Yellow line captions, medium zooms,
        // side slides between shots, music a little louder, Shorts zones.
        StylePack {
            description: t!("style_pack.builtin_vlog_desc").into_owned(),
            subtitles: Some(SubtitleStyle {
                template: "line_yellow".into(),
            }),
            auto_zoom: Some(Zoom::default()),
            animations: Some(Animations {
                clip_in: Some("animations/side_slide".into()),
                clip_in_secs: 0.6,
                text_in: Some("slide".into()),
                text_out: Some("slide".into()),
                ..Animations::default()
            }),
            music: Some(Music {
                file: None,
                volume_db: -5.0,
                ducking_db: -9.0,
            }),
            transition: Some(TransitionStyle {
                asset: "transitions/slide".into(),
                seconds: 0.5,
            }),
            safe_zone: Some(SafeZone {
                platform: "shorts".into(),
            }),
            ..StylePack::new("builtin.vlog", t!("style_pack.builtin_vlog"))
        },
    ]
}

pub(crate) fn component_label(component: Component) -> std::borrow::Cow<'static, str> {
    match component {
        Component::Subtitles => t!("style_pack.c_subtitles"),
        Component::AutoZoom => t!("style_pack.c_auto_zoom"),
        Component::Animations => t!("style_pack.c_animations"),
        Component::Music => t!("style_pack.c_music"),
        Component::Transition => t!("style_pack.c_transition"),
        Component::Intro => t!("style_pack.c_intro"),
        Component::Outro => t!("style_pack.c_outro"),
        Component::SafeZone => t!("style_pack.c_safe_zone"),
    }
}

pub(crate) fn pack_error_text(error: &PackError) -> String {
    match error {
        PackError::NotAPack => t!("style_pack.error_not_pack").into_owned(),
        PackError::Corrupt(detail) => t!("style_pack.error_corrupt", detail = detail).into_owned(),
        PackError::Newer { found, supported } => t!(
            "style_pack.error_newer",
            found = found,
            supported = supported
        )
        .into_owned(),
    }
}

fn animation_asset(id: &str, category: &str) -> bool {
    vv_render::assets::library()
        .get(id)
        .is_some_and(|a| a.kind == AssetKind::Animation && a.category == category)
}

fn timeline_end(timeline: &vv_core::Timeline) -> FrameIdx {
    let video: Vec<_> = timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video && !t.subtitles)
        .collect();
    video
        .iter()
        .flat_map(|t| &t.clips)
        .map(|c| c.timeline_end())
        .max()
        .or_else(|| {
            timeline
                .tracks
                .iter()
                .flat_map(|t| &t.clips)
                .map(|c| c.timeline_end())
                .max()
        })
        .unwrap_or(0)
}

impl VenturiApp {
    fn style_packs_dir(&self) -> Option<PathBuf> {
        Some(self.settings_path.as_ref()?.parent()?.join("style-packs"))
    }

    pub(crate) fn ensure_style_packs(&mut self) {
        if !self.style_packs.loaded {
            self.reload_style_packs();
        }
    }

    pub(crate) fn reload_style_packs(&mut self) {
        let state = &mut self.style_packs;
        state.loaded = true;
        state.load_errors.clear();
        state.entries = builtin_packs()
            .into_iter()
            .map(|pack| Entry { pack, path: None })
            .collect();
        let Some(dir) = self.style_packs_dir() else {
            return;
        };
        let Ok(read) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut files: Vec<PathBuf> = read
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        let state = &mut self.style_packs;
        for path in files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            match std::fs::read(&path)
                .map_err(|e| PackError::Corrupt(e.to_string()))
                .and_then(|bytes| style_pack::parse(&bytes))
            {
                Ok(pack) if state.entries.iter().any(|e| e.pack.id == pack.id) => {
                    state
                        .load_errors
                        .push(format!("{name}: {}", t!("style_pack.error_duplicate")));
                }
                Ok(pack) => state.entries.push(Entry {
                    pack,
                    path: Some(path),
                }),
                Err(e) => state
                    .load_errors
                    .push(format!("{name}: {}", pack_error_text(&e))),
            }
        }
    }

    pub(crate) fn style_pack(&self, id: &str) -> Option<&Entry> {
        self.style_packs.entries.iter().find(|e| e.pack.id == id)
    }

    fn new_pack_id() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        format!("user-{nanos:x}")
    }

    /// Writes `pack` into the library (new or replacing the one with its id).
    pub(crate) fn save_style_pack(&mut self, pack: StylePack) -> Result<(), String> {
        let dir = self
            .style_packs_dir()
            .ok_or_else(|| t!("style_pack.error_no_dir").into_owned())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = self
            .style_pack(&pack.id)
            .and_then(|e| e.path.clone())
            .unwrap_or_else(|| dir.join(format!("{}.json", pack.id)));
        vv_core::write_atomic(&path, pack.to_json().as_bytes()).map_err(|e| e.to_string())?;
        let entries = &mut self.style_packs.entries;
        match entries.iter_mut().find(|e| e.pack.id == pack.id) {
            Some(entry) => entry.pack = pack,
            None => entries.push(Entry {
                pack,
                path: Some(path),
            }),
        }
        Ok(())
    }

    pub(crate) fn report_pack_result(&mut self, result: Result<(), String>, done: String) {
        self.style_packs.message = Some(match result {
            Ok(()) => (false, done),
            Err(e) => (true, t!("style_pack.error_save", detail = e).into_owned()),
        });
    }

    pub(crate) fn duplicate_style_pack(&mut self, id: &str) {
        let Some(entry) = self.style_pack(id) else {
            return;
        };
        let mut pack = entry.pack.clone();
        pack.id = Self::new_pack_id();
        pack.name = t!("style_pack.copy_name", name = pack.name).into_owned();
        let new_id = pack.id.clone();
        let result = self.save_style_pack(pack);
        if result.is_ok() {
            self.style_packs.selected = Some(new_id);
        }
        self.report_pack_result(result, t!("style_pack.duplicated").into_owned());
    }

    pub(crate) fn rename_style_pack(&mut self, id: &str, name: &str) {
        let name = name.trim();
        let Some(entry) = self.style_pack(id).filter(|e| e.path.is_some()) else {
            return;
        };
        if name.is_empty() || name == entry.pack.name {
            return;
        }
        let mut pack = entry.pack.clone();
        pack.name = name.to_owned();
        let result = self.save_style_pack(pack);
        self.report_pack_result(result, t!("style_pack.renamed").into_owned());
    }

    pub(crate) fn delete_style_pack(&mut self, id: &str) {
        let Some(path) = self.style_pack(id).and_then(|e| e.path.clone()) else {
            return;
        };
        let result = std::fs::remove_file(&path).map_err(|e| e.to_string());
        if result.is_ok() {
            self.style_packs.entries.retain(|e| e.pack.id != id);
            if self.style_packs.selected.as_deref() == Some(id) {
                self.style_packs.selected = None;
            }
        }
        self.report_pack_result(result, t!("style_pack.deleted").into_owned());
    }

    /// A pack file chosen by the user goes into the library under a new id.
    pub(crate) fn import_style_pack(&mut self, path: &std::path::Path) {
        let parsed = std::fs::read(path)
            .map_err(|e| PackError::Corrupt(e.to_string()))
            .and_then(|bytes| style_pack::parse(&bytes));
        match parsed {
            Ok(mut pack) => {
                pack.id = Self::new_pack_id();
                let id = pack.id.clone();
                let result = self.save_style_pack(pack);
                if result.is_ok() {
                    self.style_packs.selected = Some(id);
                }
                self.report_pack_result(result, t!("style_pack.imported").into_owned());
            }
            Err(e) => {
                self.style_packs.message = Some((true, pack_error_text(&e)));
            }
        }
    }

    pub(crate) fn export_style_pack(&mut self, id: &str, path: &std::path::Path) {
        let Some(entry) = self.style_pack(id) else {
            return;
        };
        let result =
            vv_core::write_atomic(path, entry.pack.to_json().as_bytes()).map_err(|e| e.to_string());
        self.report_pack_result(result, t!("style_pack.exported").into_owned());
    }

    /// The parts of `pack` this installation cannot apply, with the reason.
    pub(crate) fn missing_pack_parts(&self, pack: &StylePack) -> Vec<(Component, String)> {
        let mut missing = Vec::new();
        if let Some(s) = &pack.subtitles
            && !self.subtitle_template_exists(&s.template)
        {
            missing.push((
                Component::Subtitles,
                t!("style_pack.missing_template", id = &s.template).into_owned(),
            ));
        }
        if let Some(a) = &pack.animations {
            let bad = [
                a.clip_in.as_deref().filter(|id| !animation_asset(id, "in")),
                a.clip_out
                    .as_deref()
                    .filter(|id| !animation_asset(id, "out")),
                a.text_in
                    .as_deref()
                    .filter(|id| vv_core::text_anim_presets::in_preset(id).is_none()),
                a.text_out
                    .as_deref()
                    .filter(|id| vv_core::text_anim_presets::out_preset(id).is_none()),
            ];
            if let Some(id) = bad.into_iter().flatten().next() {
                missing.push((
                    Component::Animations,
                    t!("style_pack.missing_animation", id = id).into_owned(),
                ));
            }
        }
        if let Some(t) = &pack.transition
            && !vv_render::assets::library()
                .get(&t.asset)
                .is_some_and(|a| a.kind == AssetKind::Transition)
        {
            missing.push((
                Component::Transition,
                t!("style_pack.missing_transition", id = &t.asset).into_owned(),
            ));
        }
        for (component, bumper) in [
            (Component::Intro, &pack.intro),
            (Component::Outro, &pack.outro),
        ] {
            if let Some(b) = bumper
                && !self.text_template_exists(&b.template)
            {
                missing.push((
                    component,
                    t!("style_pack.missing_template", id = &b.template).into_owned(),
                ));
            }
        }
        if let Some(z) = &pack.safe_zone
            && !self
                .safe_zone_platforms
                .0
                .iter()
                .any(|p| p.id == z.platform)
        {
            missing.push((
                Component::SafeZone,
                t!("style_pack.missing_platform", id = &z.platform).into_owned(),
            ));
        }
        missing
    }

    fn subtitle_template_exists(&self, id: &str) -> bool {
        vv_core::SubtitleTemplate::find(id).is_some()
            || id
                .strip_prefix(crate::text_templates::SUBTITLE_PREFIX)
                .is_some_and(|t| self.text_template_exists(t))
    }

    fn text_template_exists(&self, id: &str) -> bool {
        crate::text_templates::find(id).is_some()
            || self.settings.text_templates.iter().any(|t| t.id == id)
    }

    /// Selects `id` and ticks every part it can apply.
    pub(crate) fn select_style_pack(&mut self, id: &str) {
        let Some(entry) = self.style_pack(id) else {
            return;
        };
        let missing: BTreeSet<Component> = self
            .missing_pack_parts(&entry.pack)
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        self.style_packs.choice = entry
            .pack
            .components()
            .into_iter()
            .filter(|c| !missing.contains(c))
            .collect();
        self.style_packs.selected = Some(id.to_owned());
    }

    /// The preview of a pack is a real history step: an edit made meanwhile
    /// makes it final, like the auto zoom preview.
    pub(crate) fn settle_style_pack_preview(&mut self) {
        if self
            .style_packs
            .preview
            .as_ref()
            .is_some_and(|p| p.generation != self.session.history.generation())
        {
            self.style_packs.preview = None;
        }
    }

    /// Applies the ticked parts of the selected pack, replacing the preview
    /// running for it.
    pub(crate) fn preview_style_pack(&mut self) {
        self.settle_style_pack_preview();
        let Some(id) = self.style_packs.selected.clone() else {
            return;
        };
        let Some(pack) = self.style_pack(&id).map(|e| e.pack.clone()) else {
            return;
        };
        let restore = self.style_packs.preview.take().map(|p| {
            if p.applied {
                self.session.history.undo(&mut self.session.project);
            }
            (p.safe_zone_before, p.subtitle_template_before)
        });
        if self.is_timeline_playing() {
            self.toggle_playback();
        }
        let (safe_zone_before, subtitle_template_before) = restore.unwrap_or_else(|| {
            (
                (
                    self.settings.shorts.safe_zones,
                    self.settings.shorts.safe_zone_platform.clone(),
                ),
                self.subtitles.selected.clone(),
            )
        });
        (
            self.settings.shorts.safe_zones,
            self.settings.shorts.safe_zone_platform,
        ) = safe_zone_before.clone();
        self.subtitles.selected = subtitle_template_before.clone();
        let choice = self.style_packs.choice.clone();
        let before = self.session.history.position();
        let report = self.apply_style_pack(&pack, &choice);
        let applied = self.session.history.position() != before;
        self.style_packs.preview = Some(Preview {
            pack: id,
            generation: self.session.history.generation(),
            applied,
            report,
            safe_zone_before,
            subtitle_template_before,
        });
        self.ensure_active_clip_matches_playhead(true);
    }

    pub(crate) fn confirm_style_pack(&mut self) {
        let Some(preview) = self.style_packs.preview.take() else {
            return;
        };
        let zoom = self
            .style_pack(&preview.pack)
            .filter(|_| self.style_packs.choice.contains(&Component::AutoZoom))
            .and_then(|e| e.pack.auto_zoom.clone());
        if let Some(config) = zoom {
            self.settings.shorts.auto_zoom = config;
        }
        self.persist_settings();
        self.style_packs.message = Some((false, t!("style_pack.applied").into_owned()));
    }

    pub(crate) fn cancel_style_pack(&mut self) {
        self.settle_style_pack_preview();
        let Some(preview) = self.style_packs.preview.take() else {
            return;
        };
        if preview.applied {
            self.session.history.undo(&mut self.session.project);
        }
        (
            self.settings.shorts.safe_zones,
            self.settings.shorts.safe_zone_platform,
        ) = preview.safe_zone_before;
        self.subtitles.selected = preview.subtitle_template_before;
        self.ensure_active_clip_matches_playhead(true);
    }

    /// Applies the `choice` parts of `pack` to the open timeline in one
    /// undo step.
    pub(crate) fn apply_style_pack(
        &mut self,
        pack: &StylePack,
        choice: &BTreeSet<Component>,
    ) -> Vec<(Component, Outcome)> {
        let Some(timeline) = self.timeline_id else {
            return Vec::new();
        };
        let missing = self.missing_pack_parts(pack);
        let group = self.session.history.begin_group();
        let mut report = Vec::new();
        for component in pack.components() {
            if !choice.contains(&component) {
                continue;
            }
            if let Some((_, why)) = missing.iter().find(|(c, _)| *c == component) {
                report.push((component, Outcome::Skipped(why.clone())));
                continue;
            }
            let outcome = match component {
                Component::Transition => self.pack_transitions(timeline, pack),
                Component::Animations => self.pack_animations(timeline, pack),
                Component::Subtitles => self.pack_subtitles(timeline, pack),
                Component::AutoZoom => self.pack_auto_zoom(timeline, pack),
                Component::Music => self.pack_music(timeline, pack),
                Component::Intro | Component::Outro => self.pack_bumper(timeline, pack, component),
                Component::SafeZone => self.pack_safe_zone(pack),
            };
            report.push((component, outcome));
        }
        self.session
            .history
            .end_group_as(group, CommandLabel::ApplyStylePack);
        report
    }

    fn pack_transitions(&mut self, timeline: TimelineId, pack: &StylePack) -> Outcome {
        let style = pack.transition.as_ref().expect("checked by the caller");
        let library = vv_render::assets::library();
        let Some(asset) = library.get(&style.asset) else {
            return Outcome::Skipped(String::new());
        };
        let tl = &self.session.project.timelines[timeline];
        let Some(main) = tl.main_track_index() else {
            return Outcome::Skipped(t!("style_pack.no_main_track").into_owned());
        };
        let track = &tl.tracks[main];
        let free = |l: ClipId, r: ClipId| {
            !track
                .crossings
                .iter()
                .any(|c| c.left_clip == l || c.right_clip == r)
                && track
                    .clip(l)
                    .is_some_and(|c| c.effects.transition_out.is_none())
                && track
                    .clip(r)
                    .is_some_and(|c| c.effects.transition_in.is_none())
        };
        let pairs: Vec<(ClipId, ClipId)> = vv_core::fx::adjacent_pairs(tl)
            .into_iter()
            .filter(|&(t, l, r)| t == main && free(l, r))
            .map(|(_, l, r)| (l, r))
            .collect();
        let mut count = 0;
        for pair in pairs {
            if vv_core::fx::apply_asset_transition(
                &mut self.session.project,
                &mut self.session.history,
                timeline,
                main,
                pair,
                vv_core::AssetFx::new(asset.id.clone(), asset.default_params()),
                style.seconds,
            ) {
                count += 1;
            }
        }
        if count == 0 {
            Outcome::Skipped(t!("style_pack.no_cuts").into_owned())
        } else {
            Outcome::Applied(t!("style_pack.transitions_done", count = count).into_owned())
        }
    }

    fn pack_animations(&mut self, timeline: TimelineId, pack: &StylePack) -> Outcome {
        let style = pack.animations.as_ref().expect("checked by the caller");
        let tl = &self.session.project.timelines[timeline];
        let mut clips = Vec::new();
        let mut titles = Vec::new();
        for (index, track) in tl.tracks.iter().enumerate() {
            if track.kind != TrackKind::Video || track.subtitles || track.effects || track.locked {
                continue;
            }
            for clip in &track.clips {
                match &clip.source {
                    ClipSource::Text => {
                        let Some(title) = &clip.effects.title else {
                            continue;
                        };
                        let mut new = title.clone();
                        if new.anim.enter.is_none() {
                            new.anim.enter = style
                                .text_in
                                .as_deref()
                                .and_then(vv_core::text_anim_presets::in_preset);
                        }
                        if new.anim.exit.is_none() {
                            new.anim.exit = style
                                .text_out
                                .as_deref()
                                .and_then(vv_core::text_anim_presets::out_preset);
                        }
                        if new != *title {
                            titles.push((index, clip.id, new));
                        }
                    }
                    ClipSource::Media(media)
                        if self
                            .session
                            .project
                            .media_pool
                            .get(*media)
                            .is_some_and(|m| m.meta.has_video) =>
                    {
                        let mut animations = clip.effects.animations.clone();
                        if animations.in_asset.is_none()
                            && animations.entrance.is_none()
                            && let Some(id) = &style.clip_in
                        {
                            animations.in_asset = Some(id.clone());
                            animations.in_seconds = style.clip_in_secs;
                        }
                        if animations.out_asset.is_none()
                            && animations.exit.is_none()
                            && let Some(id) = &style.clip_out
                        {
                            animations.out_asset = Some(id.clone());
                            animations.out_seconds = style.clip_out_secs;
                        }
                        if animations != clip.effects.animations {
                            clips.push((index, clip.id, animations));
                        }
                    }
                    _ => {}
                }
            }
        }
        let count = clips.len() + titles.len();
        for (track, id, animations) in clips {
            self.session.history.do_command(
                &mut self.session.project,
                Box::new(vv_core::SetClipValue::new(
                    timeline,
                    track,
                    id,
                    CommandLabel::Transform,
                    animations,
                    |c| &mut c.effects.animations,
                )),
            );
        }
        for (track, id, title) in titles {
            self.session.history.do_command(
                &mut self.session.project,
                Box::new(vv_core::set_clip_title(timeline, track, id, title)),
            );
        }
        if count == 0 {
            Outcome::Skipped(t!("style_pack.animations_none").into_owned())
        } else {
            Outcome::Applied(t!("style_pack.animations_done", count = count).into_owned())
        }
    }

    fn pack_subtitles(&mut self, timeline: TimelineId, pack: &StylePack) -> Outcome {
        let style = pack.subtitles.as_ref().expect("checked by the caller");
        if !self.session.project.captions.contains_key(&timeline) {
            // Kept as the template of the next «Genera sottotitoli».
            self.subtitles.selected = Some(style.template.clone());
            return Outcome::Skipped(t!("style_pack.subtitles_later").into_owned());
        }
        self.choose_subtitle_template(&style.template);
        Outcome::Applied(t!("style_pack.subtitles_done").into_owned())
    }

    fn pack_auto_zoom(&mut self, timeline: TimelineId, pack: &StylePack) -> Outcome {
        let config = pack.auto_zoom.clone().expect("checked by the caller");
        let tl = &self.session.project.timelines[timeline];
        let Some(main) = tl.main_track_index() else {
            return Outcome::Skipped(t!("style_pack.no_main_track").into_owned());
        };
        if tl.tracks[main].locked {
            return Outcome::Skipped(t!("auto_cut.error_locked").into_owned());
        }
        let fps = tl.fps.as_f64();
        let resolution = tl.resolution;
        let clips: Vec<vv_core::Clip> = tl.tracks[main]
            .clips
            .iter()
            .filter(|c| match c.source {
                ClipSource::Media(m) => self
                    .session
                    .project
                    .media_pool
                    .get(m)
                    .is_some_and(|m| m.meta.has_video && !m.meta.is_image()),
                _ => false,
            })
            .cloned()
            .collect();
        let (mut zoomed, mut untranscribed) = (0, 0);
        for clip in clips {
            let Some(spec) = self
                .speech_source(timeline, &clip)
                .filter(|s| s.error.is_none())
            else {
                continue;
            };
            let Some(source) = self.cached_speech(&spec) else {
                untranscribed += 1;
                continue;
            };
            let duration = clip.timeline_len as f64 / fps;
            let (words, energy) =
                crate::auto_cut::clip_local(&source, spec.media_start, spec.media_speed, duration);
            let zooms = vv_core::auto_zoom::plan(&words, &energy, duration, &config);
            if zooms.is_empty() {
                continue;
            }
            let (transform, auto) =
                vv_core::auto_zoom::keyframes(&clip, &zooms, &config, fps, resolution);
            self.session.history.do_command(
                &mut self.session.project,
                Box::new(vv_core::auto_zoom::SetAutoZoom::new(
                    timeline, main, clip.id, transform, auto,
                )),
            );
            zoomed += 1;
        }
        let mut text = t!("style_pack.zoom_done", count = zoomed).into_owned();
        if untranscribed > 0 {
            text.push_str(&t!("style_pack.zoom_untranscribed", count = untranscribed));
        }
        if zoomed == 0 {
            Outcome::Skipped(text)
        } else {
            Outcome::Applied(text)
        }
    }

    /// Clips on audio tracks that are not someone speaking: not linked to a
    /// picture and not a transcribed voice the transcript follows.
    fn music_clips(&self, timeline: TimelineId) -> Vec<(usize, ClipId)> {
        let tl = &self.session.project.timelines[timeline];
        let speech: BTreeSet<ClipId> = self
            .transcript_candidates(timeline)
            .into_iter()
            .filter(|(_, c)| {
                self.speech_source(timeline, c)
                    .is_some_and(|spec| self.cached_speech(&spec).is_some())
            })
            .map(|(_, c)| c.id)
            .collect();
        let video_groups: std::collections::HashSet<_> = tl
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video)
            .flat_map(|t| &t.clips)
            .filter_map(|c| c.linked_group)
            .collect();
        tl.tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == TrackKind::Audio && !t.locked)
            .flat_map(|(i, t)| t.clips.iter().map(move |c| (i, c)))
            .filter(|(_, c)| {
                matches!(c.source, ClipSource::Media(_))
                    && !c.disabled
                    && !speech.contains(&c.id)
                    && c.linked_group.is_none_or(|g| !video_groups.contains(&g))
            })
            .map(|(i, c)| (i, c.id))
            .collect()
    }

    /// Puts `file` on a new audio track from the start, as long as the edit.
    fn insert_music(
        &mut self,
        timeline: TimelineId,
        file: &std::path::Path,
    ) -> Result<Vec<(usize, ClipId)>, String> {
        let media = match self.session.media_with_path(file) {
            Some(media) => media,
            None => self.add_media_to_pool(file.to_path_buf())?,
        };
        let meta = self.session.project.media_pool[media].meta.clone();
        if !meta.has_audio {
            return Err(t!("auto_cut.error_no_audio").into_owned());
        }
        let tl = &self.session.project.timelines[timeline];
        let end = timeline_end(tl);
        if end <= 0 {
            return Err(t!("style_pack.empty_timeline").into_owned());
        }
        let rate = vv_core::Rational::conform_rate(tl.fps, meta.fps);
        let full = vv_core::Clip::from_source_range(
            ClipId(0),
            ClipSource::Media(media),
            0,
            meta.duration_frames,
            0,
            rate,
        )
        .timeline_len
        .max(1);
        let source_out = if full > end {
            ((meta.duration_frames as f64 * end as f64 / full as f64).floor() as FrameIdx).max(1)
        } else {
            meta.duration_frames
        };
        let track = vv_core::edit::add_track(
            &mut self.session.project,
            &mut self.session.history,
            timeline,
            TrackKind::Audio,
        );
        let inserted = vv_core::edit::insert_media(
            &mut self.session.project,
            &mut self.session.history,
            timeline,
            vv_core::edit::MediaInsert {
                media_id: media,
                source_in: 0,
                source_out,
                video: false,
                audio: true,
            },
            0,
            vv_core::edit::TargetTracks {
                video: None,
                extra_audio: Some(track),
            },
        );
        inserted
            .filter(|clips| !clips.is_empty())
            .ok_or_else(|| t!("style_pack.music_failed").into_owned())
    }

    fn pack_music(&mut self, timeline: TimelineId, pack: &StylePack) -> Outcome {
        let music = pack.music.clone().expect("checked by the caller");
        let mut notes = Vec::new();
        let mut inserted = Vec::new();
        if let Some(file) = &music.file {
            if file.is_file() {
                match self.insert_music(timeline, file) {
                    Ok(clips) => inserted = clips,
                    Err(e) => notes.push(e),
                }
            } else {
                notes.push(
                    t!(
                        "style_pack.music_missing",
                        file = file.display().to_string()
                    )
                    .into_owned(),
                );
            }
        }
        let targets = if inserted.is_empty() {
            self.music_clips(timeline)
        } else {
            inserted.clone()
        };
        if targets.is_empty() {
            notes.push(t!("style_pack.no_music").into_owned());
            return Outcome::Skipped(notes.join(" "));
        }
        let regions = vv_core::style_pack::speech_regions(self.speech_spans(timeline));
        let fps = self.session.project.timelines[timeline].fps.as_f64();
        let (mut ducked, mut manual) = (0, 0);
        for (track, id) in targets {
            let Some(clip) = self.session.project.timelines[timeline].clip(track, id) else {
                continue;
            };
            if !inserted.contains(&(track, id)) && !clip.effects.gain_db.is_constant() {
                manual += 1;
                continue;
            }
            let gain = vv_core::style_pack::ducking_gain(
                clip,
                fps,
                &regions,
                music.volume_db,
                music.ducking_db,
            );
            self.session.history.do_command(
                &mut self.session.project,
                Box::new(vv_core::SetClipValue::new(
                    timeline,
                    track,
                    id,
                    CommandLabel::Gain,
                    gain,
                    |c| &mut c.effects.gain_db,
                )),
            );
            ducked += 1;
        }
        if !inserted.is_empty() {
            notes.insert(0, t!("style_pack.music_added").into_owned());
        }
        if ducked > 0 {
            notes.push(t!("style_pack.music_done", count = ducked).into_owned());
        }
        if manual > 0 {
            notes.push(t!("style_pack.music_manual", count = manual).into_owned());
        }
        if ducked == 0 {
            Outcome::Skipped(notes.join(" "))
        } else {
            Outcome::Applied(notes.join(" "))
        }
    }

    fn pack_bumper(&mut self, timeline: TimelineId, pack: &StylePack, which: Component) -> Outcome {
        let bumper = if which == Component::Intro {
            pack.intro.clone()
        } else {
            pack.outro.clone()
        }
        .expect("checked by the caller");
        let tl = &self.session.project.timelines[timeline];
        let end = timeline_end(tl);
        if end <= 0 {
            return Outcome::Skipped(t!("style_pack.empty_timeline").into_owned());
        }
        let fps = tl.fps.as_f64();
        let secs = self.text_template_secs(&bumper.template, tl.resolution);
        let length = ((f64::from(secs) * fps).round() as FrameIdx).max(1);
        let start = if which == Component::Intro {
            0
        } else {
            (end - length).max(0)
        };
        self.insert_text_template_with(
            Some(&bumper.template),
            start,
            MediaDropTarget::NewVideoTrack,
            Some(&bumper.text),
        );
        Outcome::Applied(
            if which == Component::Intro {
                t!("style_pack.intro_done")
            } else {
                t!("style_pack.outro_done")
            }
            .into_owned(),
        )
    }

    fn pack_safe_zone(&mut self, pack: &StylePack) -> Outcome {
        let zone = pack.safe_zone.as_ref().expect("checked by the caller");
        self.settings.shorts.safe_zones = true;
        self.settings.shorts.safe_zone_platform = zone.platform.clone();
        self.safe_zone_platforms = crate::safe_zones::load();
        Outcome::Applied(t!("style_pack.safe_zone_done").into_owned())
    }

    /// The look of the open edit as a new pack of the user's.
    pub(crate) fn capture_style_pack(&mut self) -> StylePack {
        let number = self
            .style_packs
            .entries
            .iter()
            .filter(|e| e.path.is_some())
            .count()
            + 1;
        let mut pack = StylePack::new(
            Self::new_pack_id(),
            t!("style_pack.my_style", number = number),
        );
        let Some(timeline) = self.timeline_id else {
            return pack;
        };
        let tl = &self.session.project.timelines[timeline];
        let fps = tl.fps.as_f64();
        pack.subtitles = self
            .session
            .project
            .captions
            .get(&timeline)
            .map(|c| c.template.clone())
            .or_else(|| self.subtitles.selected.clone())
            .map(|template| SubtitleStyle { template });
        let clips = || {
            tl.tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.kind == TrackKind::Video && !t.subtitles)
                .flat_map(|(i, t)| t.clips.iter().map(move |c| (i, c)))
        };
        if clips().any(|(_, c)| !c.auto_keyframes.is_empty()) {
            pack.auto_zoom = Some(self.settings.shorts.auto_zoom.clone());
        }
        let mut animations = Animations::default();
        if let Some((_, c)) = clips().find(|(_, c)| c.effects.animations.in_asset.is_some()) {
            animations.clip_in = c.effects.animations.in_asset.clone();
            animations.clip_in_secs = c.effects.animations.in_seconds;
        }
        if let Some((_, c)) = clips().find(|(_, c)| c.effects.animations.out_asset.is_some()) {
            animations.clip_out = c.effects.animations.out_asset.clone();
            animations.clip_out_secs = c.effects.animations.out_seconds;
        }
        let titles = || clips().filter_map(|(_, c)| c.effects.title.as_ref());
        animations.text_in = titles().find_map(|t| t.anim.enter.as_ref().map(|a| a.id.clone()));
        animations.text_out = titles().find_map(|t| t.anim.exit.as_ref().map(|a| a.id.clone()));
        if animations != Animations::default() {
            pack.animations = Some(animations);
        }
        if let Some(main) = tl.main_track_index()
            && let Some((asset, frames)) = tl.tracks[main]
                .crossings
                .iter()
                .find_map(|c| Some((c.transition.asset.as_ref()?, c.transition.duration)))
        {
            pack.transition = Some(TransitionStyle {
                asset: asset.id.clone(),
                seconds: frames as f64 / fps,
            });
        }
        if let Some(&(track, id)) = self.music_clips(timeline).first()
            && let Some(clip) = tl.clip(track, id)
        {
            let gain = &clip.effects.gain_db;
            let lowest = gain
                .keyframes()
                .iter()
                .map(|(_, v, _)| *v)
                .fold(gain.default, f32::min);
            let level = gain
                .keyframes()
                .iter()
                .map(|(_, v, _)| *v)
                .fold(gain.default, f32::max);
            pack.music = Some(Music {
                file: match clip.source {
                    ClipSource::Media(m) => self
                        .session
                        .project
                        .media_pool
                        .get(m)
                        .map(|m| m.path.clone()),
                    _ => None,
                },
                volume_db: level,
                ducking_db: if gain.is_constant() {
                    Music::default().ducking_db
                } else {
                    lowest - level
                },
            });
        }
        if self.settings.shorts.safe_zones {
            pack.safe_zone = Some(SafeZone {
                platform: self.settings.shorts.safe_zone_platform.clone(),
            });
        }
        pack
    }
}

#[cfg(test)]
#[path = "tests/style_packs.rs"]
mod tests;
