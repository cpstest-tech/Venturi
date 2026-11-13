//! The open project saves itself: ~2 s after the last confirmed edit, at
//! least every 30 s while edits are pending, and when it is left.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;

const DEBOUNCE: Duration = Duration::from_secs(2);
const MAX_INTERVAL: Duration = Duration::from_secs(30);
const RETRY_AFTER: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SaveStatus {
    Saved,
    Saving,
    Failed(String),
}

struct InFlight {
    path: PathBuf,
    epoch: u64,
    mark: vv_session::ChangeMark,
    rx: mpsc::Receiver<Result<(), String>>,
}

#[derive(Default)]
pub(crate) struct Autosave {
    seen: Option<vv_session::ChangeMark>,
    last_change: Option<Instant>,
    /// When the oldest edit not yet on disk was seen.
    pending_since: Option<Instant>,
    in_flight: Option<InFlight>,
    error: Option<String>,
    retry_at: Option<Instant>,
}

impl Autosave {
    pub(crate) fn observe(&mut self, mark: vv_session::ChangeMark, unsaved: bool, now: Instant) {
        if self.seen != Some(mark) {
            self.seen = Some(mark);
            if unsaved {
                self.last_change = Some(now);
            }
        }
        if unsaved {
            self.pending_since.get_or_insert(now);
        } else {
            self.pending_since = None;
        }
    }

    pub(crate) fn due(&self, now: Instant) -> bool {
        let Some(since) = self.pending_since else {
            return false;
        };
        if self.in_flight.is_some() || self.retry_at.is_some_and(|at| now < at) {
            return false;
        }
        let quiet = self
            .last_change
            .is_none_or(|at| now.duration_since(at) >= DEBOUNCE);
        quiet || now.duration_since(since) >= MAX_INTERVAL
    }

    /// How long until `due` may turn true, for the repaint timer.
    pub(crate) fn next_check(&self, now: Instant) -> Option<Duration> {
        let since = self.pending_since?;
        let mut at = since + MAX_INTERVAL;
        if let Some(last) = self.last_change {
            at = at.min(last + DEBOUNCE);
        }
        if let Some(retry) = self.retry_at {
            at = at.max(retry);
        }
        Some(at.saturating_duration_since(now))
    }

    fn failed(&mut self, error: String, now: Instant) {
        self.error = Some(error);
        self.retry_at = Some(now + RETRY_AFTER);
    }

    fn succeeded(&mut self) {
        self.error = None;
        self.retry_at = None;
    }
}

impl VenturiApp {
    /// Once per frame in the editor.
    pub(crate) fn tick_autosave(&mut self, ctx: &egui::Context) {
        self.poll_autosave();
        let Some(path) = self.session.path().map(Path::to_path_buf) else {
            return;
        };
        let now = Instant::now();
        let unsaved = self.has_unsaved_changes();
        self.autosave
            .observe(self.session.change_mark(), unsaved, now);
        if self.autosave.in_flight.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        // A gesture is one edit: it is saved once released.
        if self.undo_group_open() {
            return;
        }
        if self.autosave.due(now) {
            self.start_background_save(path);
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if let Some(wait) = self.autosave.next_check(now) {
            ctx.request_repaint_after(wait);
        }
    }

    /// The copy is serialized and written on another thread, so a large
    /// project does not stall the editor.
    fn start_background_save(&mut self, path: PathBuf) {
        self.store_view();
        self.copy_library_media_into_project(&path);
        let project = self.session.project.clone();
        let (tx, rx) = mpsc::channel();
        let target = path.clone();
        std::thread::spawn(move || {
            let result = vv_core::save_project(&project, &target).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        self.autosave.in_flight = Some(InFlight {
            path,
            epoch: self.session.epoch(),
            mark: self.session.change_mark(),
            rx,
        });
    }

    fn poll_autosave(&mut self) {
        let Some(flight) = &self.autosave.in_flight else {
            return;
        };
        let result = match flight.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(t!("project.autosave_lost").into_owned()),
        };
        if let Some(flight) = self.autosave.in_flight.take() {
            self.finish_autosave(flight, result);
        }
    }

    fn finish_autosave(&mut self, flight: InFlight, result: Result<(), String>) {
        match result {
            // The project may have been replaced while the file was written.
            Ok(()) if flight.epoch == self.session.epoch() => {
                self.session.finish_save(flight.path, flight.mark);
                self.autosave.succeeded();
            }
            Ok(()) => self.autosave.succeeded(),
            Err(e) => self.autosave.failed(e, Instant::now()),
        }
    }

    /// Before leaving the project: waits for a save in flight and writes
    /// what is left. `false` if something could not be saved.
    pub(crate) fn flush_autosave(&mut self) -> bool {
        if let Some(flight) = self.autosave.in_flight.take() {
            let result = flight
                .rx
                .recv()
                .unwrap_or_else(|_| Err(t!("project.autosave_lost").into_owned()));
            self.finish_autosave(flight, result);
        }
        let Some(path) = self.session.path().map(Path::to_path_buf) else {
            return !self.has_unsaved_changes();
        };
        if self.has_unsaved_changes() {
            self.save_project_to(&path);
            if self.has_unsaved_changes() {
                let error = self.project_error.clone().unwrap_or_default();
                self.autosave.failed(error, Instant::now());
                return false;
            }
            self.autosave.succeeded();
        } else {
            self.save_view();
        }
        true
    }

    pub(crate) fn save_status(&self) -> SaveStatus {
        if let Some(error) = &self.autosave.error {
            SaveStatus::Failed(error.clone())
        } else if self.autosave.in_flight.is_some() || self.has_unsaved_changes() {
            SaveStatus::Saving
        } else {
            SaveStatus::Saved
        }
    }

    pub(crate) fn reset_autosave(&mut self) {
        self.autosave = Autosave::default();
    }
}

#[cfg(test)]
#[path = "tests/autosave.rs"]
mod tests;
