//! Microphone recording during timeline playback.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vv_audio::recorder::{Recorder, Take};
use vv_core::{Clip, ClipSource, CommandLabel, FrameIdx, Rational, TimelineId, TrackKind};
use vv_media::audio_file::{AudioFileFormat, write_audio_file};

use crate::VenturiApp;
use crate::theme;
use crate::timeline_ui::{RECORDING_PEAKS_PER_SEC, RecordingView};
use crate::workspace::Block;

/// Folder of the takes, next to the project file.
pub(crate) const RECORDINGS_DIR: &str = "Recordings";

pub(crate) struct ActiveTake {
    recorder: Recorder,
    timeline: TimelineId,
    start: FrameIdx,
    target: TakeTarget,
    /// For the live waveform, see `extend_peaks`.
    peaks: Vec<f32>,
    peaked_frames: usize,
}

enum TakeTarget {
    Armed(Vec<usize>),
    NewTrack { end: FrameIdx },
}

#[derive(Clone, Copy)]
struct Countdown {
    timeline: TimelineId,
    start: FrameIdx,
    deadline: Instant,
}

pub(crate) struct Panel {
    open: bool,
    devices: Vec<String>,
    input: Option<String>,
    mute_preview: bool,
    countdown: Option<Countdown>,
    error: Option<String>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            open: false,
            devices: Vec::new(),
            input: None,
            mute_preview: true,
            countdown: None,
            error: None,
        }
    }
}

/// Adds to `peaks` one peak (over every channel) per full block of
/// `1 / RECORDING_PEAKS_PER_SEC` s of `recorded` past `peaked_frames`.
pub(crate) fn extend_peaks(peaks: &mut Vec<f32>, peaked_frames: &mut usize, recorded: &Take) {
    let ch = recorded.channels.max(1) as usize;
    let block = (recorded.sample_rate as f64 / RECORDING_PEAKS_PER_SEC).max(1.0) as usize;
    while *peaked_frames + block <= recorded.frames() {
        let samples = &recorded.samples[*peaked_frames * ch..(*peaked_frames + block) * ch];
        peaks.push(samples.iter().fold(0.0f32, |p, s| p.max(s.abs())));
        *peaked_frames += block;
    }
}

/// The first `Voiceover NNN` not in `dir` yet, in any format: a take
/// number is never reused when the format changes.
pub(crate) fn next_take_path(dir: &Path, format: AudioFileFormat) -> PathBuf {
    (1..)
        .map(|n| format!("Voiceover {n:03}"))
        .find(|name| {
            AudioFileFormat::ALL
                .iter()
                .all(|f| !dir.join(format!("{name}.{}", f.extension())).exists())
        })
        .map(|name| dir.join(format!("{name}.{}", format.extension())))
        .expect("an unbounded range")
}

/// Past this distance between the playhead and the recorded time the
/// playback jumped (a seek, a scrub): the take ends there.
const JUMP_SECS: f64 = 0.5;
const COUNTDOWN: Duration = Duration::from_secs(3);

impl VenturiApp {
    /// The armed tracks of the open timeline a take can land on.
    fn armed_tracks(&self) -> Vec<usize> {
        let Some(timeline_id) = self.timeline_id else {
            return Vec::new();
        };
        self.session.project.timelines[timeline_id]
            .tracks_of_kind(TrackKind::Audio)
            .filter(|(_, t)| t.armed && !t.locked)
            .map(|(i, _)| i)
            .collect()
    }

    /// The folder chosen in the settings, or `Recordings` next to the
    /// project file once it has one.
    fn recordings_dir(&self) -> Option<PathBuf> {
        if let Some(dir) = &self.settings.recording_dir {
            return Some(dir.clone());
        }
        Some(self.session.path()?.parent()?.join(RECORDINGS_DIR))
    }

    /// Before playback starts: `false` if it must not, because armed
    /// tracks are waiting for a project folder to record into.
    pub(crate) fn prepare_take(&mut self) -> bool {
        if self.armed_tracks().is_empty() {
            return true;
        }
        if self.recordings_dir().is_none() {
            self.timeline_state.record_needs_save = true;
            return false;
        }
        true
    }

    /// Just before playback starts at 1x from `start`.
    pub(crate) fn start_take(&mut self, start: FrameIdx) {
        let tracks = self.armed_tracks();
        let Some(timeline) = self.timeline_id.filter(|_| !tracks.is_empty()) else {
            return;
        };
        match Recorder::start(self.settings.input_device.as_deref()) {
            Ok(recorder) => {
                self.take = Some(ActiveTake {
                    recorder,
                    timeline,
                    start,
                    target: TakeTarget::Armed(tracks),
                    peaks: Vec::new(),
                    peaked_frames: 0,
                });
            }
            Err(e) => {
                self.project_error = Some(t!("voiceover.no_input", error = e).into_owned());
            }
        }
    }

    /// Every frame: collects the microphone and ends the take once the
    /// playback stopped, left 1x or jumped.
    pub(crate) fn drive_take(&mut self) {
        self.timeline_state.can_record = self.recordings_dir().is_some();
        let playing = self.is_timeline_playing();
        let Some(take) = &mut self.take else {
            self.timeline_state.recording = None;
            return;
        };
        let Some(timeline) = self.session.project.timelines.get(take.timeline) else {
            self.finish_take();
            return;
        };
        let fps = timeline.fps.as_f64();
        let recorded = take.recorder.poll();
        let expected = take.start as f64 + recorded.seconds() * fps;
        let jumped = (self.timeline_state.playhead as f64 - expected).abs() > JUMP_SECS * fps;
        let still = playing
            && self.playback_speed == 1.0
            && self.timeline_id == Some(take.timeline)
            && self.browsing.is_none()
            && !jumped;
        if !still {
            if matches!(take.target, TakeTarget::NewTrack { .. }) {
                self.stop_voiceover();
            } else {
                self.finish_take();
            }
        } else if let TakeTarget::Armed(tracks) = &take.target {
            extend_peaks(&mut take.peaks, &mut take.peaked_frames, recorded);
            self.timeline_state.recording = Some(RecordingView {
                start: take.start,
                tracks: tracks.clone(),
                peaks: take.peaks.clone(),
            });
        }
    }

    pub(crate) fn finish_take(&mut self) {
        self.timeline_state.recording = None;
        let Some(take) = self.take.take() else {
            return;
        };
        let mut recorded = take.recorder.finish();
        let manual = matches!(take.target, TakeTarget::NewTrack { .. });
        if manual && let Some(audio) = &self.timeline_audio {
            audio.set_preview_muted(false);
        }
        let Some(timeline) = self.session.project.timelines.get(take.timeline) else {
            return;
        };
        if let TakeTarget::NewTrack { end } = take.target {
            let seconds = (end - take.start) as f64 / timeline.fps.as_f64();
            let frames = (seconds * f64::from(recorded.sample_rate)).round() as usize;
            recorded
                .samples
                .truncate(frames.saturating_mul(recorded.channels as usize));
        }
        if recorded.frames() == 0 {
            if manual {
                self.voiceover.error = Some(t!("voiceover.empty_take").into_owned());
            }
            return;
        }
        if let Err(e) = self.place_take(take.timeline, take.start, &take.target, &recorded) {
            let message = t!("voiceover.save_failed", error = e).into_owned();
            if manual && self.voiceover.open {
                self.voiceover.error = Some(message);
            } else {
                self.project_error = Some(message);
            }
        }
    }

    /// Writes `recorded` into the project folder and puts it on `tracks`
    /// from `start`, overwriting what it meets, as one history step.
    fn place_take(
        &mut self,
        timeline: TimelineId,
        start: FrameIdx,
        target: &TakeTarget,
        recorded: &Take,
    ) -> Result<(), String> {
        let dir = self.recordings_dir().ok_or("the project has no folder")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let format = AudioFileFormat::resolve(self.settings.recording_format);
        let path = next_take_path(&dir, format);
        write_audio_file(
            &path,
            format,
            &recorded.samples,
            recorded.sample_rate,
            recorded.channels,
        )
        .map_err(|e| e.to_string())?;
        let meta = vv_media::probe_media(&path).map_err(|e| e.to_string())?;

        let group = self.session.history.begin_group();
        let media_id = self.session.add_media(path, meta.clone());
        let project = &mut self.session.project;
        let rate = Rational::conform_rate(project.timelines[timeline].fps, meta.fps);
        let (new_track, limit);
        let tracks = match target {
            TakeTarget::Armed(tracks) => {
                limit = FrameIdx::MAX;
                tracks.as_slice()
            }
            TakeTarget::NewTrack { end } => {
                limit = end - start;
                new_track = [vv_core::edit::add_track(
                    project,
                    &mut self.session.history,
                    timeline,
                    TrackKind::Audio,
                )];
                &new_track
            }
        };
        let clips = tracks
            .iter()
            .map(|&track| {
                let mut clip = Clip::from_source_range(
                    project.alloc_clip_id(),
                    ClipSource::Media(media_id),
                    0,
                    meta.duration_frames,
                    start,
                    rate,
                );
                if matches!(target, TakeTarget::NewTrack { .. }) {
                    clip.timeline_len = clip.timeline_len.clamp(1, limit);
                }
                (track, clip, None::<u64>)
            })
            .collect();
        vv_core::edit::insert_clips(
            project,
            &mut self.session.history,
            timeline,
            clips,
            CommandLabel::RecordVoiceover,
        );
        // One take per arming: the next playback must not record over it.
        if let TakeTarget::Armed(tracks) = target {
            for &track in tracks {
                self.session.history.do_command(
                    &mut self.session.project,
                    Box::new(vv_core::SetTrackFlag::new(
                        timeline,
                        track,
                        vv_core::TrackFlag::Armed,
                        false,
                    )),
                );
            }
        }
        self.session
            .history
            .end_group_as(group, CommandLabel::RecordVoiceover);
        self.enqueue_media_background_jobs(media_id);
        Ok(())
    }

    pub(crate) fn open_voiceover(&mut self) {
        if !self.voiceover.open {
            self.voiceover.open = true;
            self.voiceover.devices = vv_audio::recorder::input_device_names();
            self.voiceover.input = self.settings.input_device.clone();
            self.voiceover.error = None;
        }
        self.reveal_block(Block::Player);
    }

    fn voiceover_start(&self) -> Option<(TimelineId, FrameIdx)> {
        let timeline = self.timeline_id?;
        let start = self.timeline_state.playhead;
        let end = self.session.project.timelines.get(timeline)?.total_frames();
        (start >= 0 && start < end).then_some((timeline, start))
    }

    fn count_down_voiceover(&mut self) {
        if self.take.is_some() {
            return;
        }
        let Some((timeline, start)) = self.voiceover_start() else {
            return;
        };
        if self.recordings_dir().is_none() {
            self.timeline_state.record_needs_save = true;
            return;
        }
        self.stop_browsing();
        self.timeline_audio().pause();
        self.reset_playback_speed_to_normal();
        self.resume_after_scrub = false;
        self.reverse_shuttle = None;
        self.voiceover.error = None;
        self.voiceover.countdown = Some(Countdown {
            timeline,
            start,
            deadline: Instant::now() + COUNTDOWN,
        });
    }

    fn start_voiceover(&mut self, countdown: Countdown) {
        let Countdown {
            timeline, start, ..
        } = countdown;
        let sequence = &self.session.project.timelines[timeline];
        let (fps, end) = (sequence.fps.as_f64(), sequence.total_frames());
        self.cue_timeline_playback(start, fps);
        match Recorder::start(self.voiceover.input.as_deref()) {
            Ok(recorder) => {
                self.take = Some(ActiveTake {
                    recorder,
                    timeline,
                    start,
                    target: TakeTarget::NewTrack { end },
                    peaks: Vec::new(),
                    peaked_frames: 0,
                });
                let muted = self.voiceover.mute_preview;
                let audio = self.timeline_audio();
                audio.set_preview_muted(muted);
                audio.play();
            }
            Err(e) => {
                self.voiceover.error = Some(t!("voiceover.no_input", error = e).into_owned());
            }
        }
    }

    pub(crate) fn stop_voiceover(&mut self) {
        self.voiceover.countdown = None;
        if self.take.as_ref().is_some_and(ActiveTake::is_voiceover) {
            self.timeline_audio().pause();
            self.reset_playback_speed_to_normal();
            self.finish_take();
        }
    }

    pub(crate) fn show_voiceover(
        &mut self,
        ctx: &egui::Context,
        timeline_rect: Option<egui::Rect>,
    ) {
        if !self.voiceover.open {
            return;
        }

        let now = Instant::now();
        let start = self.voiceover_start();
        if self.voiceover.countdown.is_some_and(|countdown| {
            start != Some((countdown.timeline, countdown.start))
                || self.browsing.is_some()
                || self.is_timeline_playing()
                || self.take.is_some()
        }) {
            self.voiceover.countdown = None;
        }

        let recording = self.take.as_ref().filter(|take| take.is_voiceover());
        let ready = start.is_some() && self.take.is_none();
        let mut open = true;
        let mut record = false;
        let mut stop = false;
        let mut window = egui::Window::new(t!("voiceover.title"))
            .id(egui::Id::new("voiceover"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(270.0);
        if let Some(rect) = timeline_rect {
            window = window.default_pos(egui::pos2(
                (rect.right() - 302.0).max(rect.left()),
                rect.top() + 36.0,
            ));
        }
        window.show(ctx, |ui| {
            if let Some(take) = recording {
                let fps = self.session.project.timelines[take.timeline].fps.as_f64();
                let elapsed = (self.timeline_state.playhead - take.start).max(0) as f64 / fps;
                ui.horizontal(|ui| {
                    ui.colored_label(theme::ERROR, t!("voiceover.recording"));
                    ui.monospace(crate::timeline_ui::format_timecode(elapsed, fps));
                });
                stop = ui.button(t!("voiceover.stop")).clicked();
            } else if let Some(countdown) = self.voiceover.countdown {
                let remaining = countdown.deadline.saturating_duration_since(now);
                let seconds = (remaining.as_secs_f64().ceil() as u64).max(1);
                ui.label(t!("voiceover.countdown", seconds = seconds));
                stop = ui.button(t!("common.cancel")).clicked();
                ctx.request_repaint_after(remaining - Duration::from_secs(seconds - 1));
            } else {
                crate::settings_dialog::input_device_picker(
                    ui,
                    &mut self.voiceover.input,
                    &self.voiceover.devices,
                );
                ui.checkbox(
                    &mut self.voiceover.mute_preview,
                    t!("voiceover.mute_preview"),
                );
                if let Some(error) = &self.voiceover.error {
                    ui.colored_label(theme::ERROR, error);
                }
                record = ui
                    .add_enabled(ready, egui::Button::new(t!("voiceover.record")))
                    .on_disabled_hover_text(t!("voiceover.start_hint"))
                    .clicked();
            }
        });
        self.voiceover.open = open;
        if !open || stop {
            self.stop_voiceover();
        } else if record {
            self.count_down_voiceover();
            ctx.request_repaint();
        } else if let Some(countdown) = self.voiceover.countdown.filter(|c| now >= c.deadline) {
            self.voiceover.countdown = None;
            self.start_voiceover(countdown);
            ctx.request_repaint();
        }
    }

    /// Why recording cannot start yet, with a way out.
    pub(crate) fn show_record_needs_save(&mut self, ctx: &egui::Context) {
        if !self.timeline_state.record_needs_save {
            return;
        }
        let (mut save, mut close) = (false, false);
        egui::Window::new(t!("voiceover.needs_save_title"))
            .id(egui::Id::new("record_needs_save"))
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(t!("voiceover.needs_save", dir = RECORDINGS_DIR));
                ui.separator();
                ui.horizontal(|ui| {
                    save = ui.button(t!("voiceover.save_project")).clicked();
                    close = ui.button(t!("common.cancel")).clicked();
                });
            });
        if save {
            self.save_project();
        }
        if save || close {
            self.timeline_state.record_needs_save = false;
        }
    }
}

impl ActiveTake {
    fn is_voiceover(&self) -> bool {
        matches!(self.target, TakeTarget::NewTrack { .. })
    }
}

#[cfg(test)]
#[path = "tests/voiceover.rs"]
mod tests;
