//! Background-removal jobs: one media at a time on its own thread (the
//! matting itself spreads over several), keyed by `content_hash`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use vv_core::FrameIdx;

pub(crate) struct MatteRequest {
    pub content_hash: u64,
    pub path: PathBuf,
    pub frames: FrameIdx,
    pub fps: f64,
}

pub(crate) enum MatteState {
    Queued,
    Running(Arc<vv_matting::Progress>),
    Failed(String),
}

struct Running {
    content_hash: u64,
    progress: Arc<vv_matting::Progress>,
    cancel: Arc<AtomicBool>,
    done: mpsc::Receiver<Result<(), vv_matting::Error>>,
}

#[derive(Default)]
pub(crate) struct MatteWorker {
    queue: VecDeque<MatteRequest>,
    running: Option<Running>,
    /// Empty: stopped by the user.
    failed: HashMap<u64, String>,
    complete: HashSet<u64>,
}

impl MatteWorker {
    /// Queues `request` unless its mattes are ready, on the way, or failed
    /// or stopped (until `retry`).
    pub(crate) fn request(&mut self, request: MatteRequest) {
        let hash = request.content_hash;
        if self.complete.contains(&hash)
            || self.failed.contains_key(&hash)
            || self
                .running
                .as_ref()
                .is_some_and(|r| r.content_hash == hash)
            || self.queue.iter().any(|q| q.content_hash == hash)
        {
            return;
        }
        if vv_media::matte::mattes_complete(hash) {
            self.complete.insert(hash);
            return;
        }
        self.queue.push_back(request);
    }

    pub(crate) fn state(&self, content_hash: u64) -> Option<MatteState> {
        if let Some(running) = self
            .running
            .as_ref()
            .filter(|r| r.content_hash == content_hash)
        {
            return Some(MatteState::Running(running.progress.clone()));
        }
        if self.queue.iter().any(|q| q.content_hash == content_hash) {
            return Some(MatteState::Queued);
        }
        self.failed
            .get(&content_hash)
            .map(|e| MatteState::Failed(e.clone()))
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.running.is_some() || !self.queue.is_empty()
    }

    /// Starts the next job and collects the finished one. Returns `true`
    /// when new mattes reached the disk, so the preview redraws.
    pub(crate) fn poll(&mut self) -> bool {
        let mut progressed = false;
        if let Some(running) = &self.running {
            progressed = running.progress.stage() == vv_matting::Stage::Matting;
            match running.done.try_recv() {
                Ok(result) => {
                    let hash = running.content_hash;
                    self.running = None;
                    match result {
                        Ok(()) => {
                            self.complete.insert(hash);
                        }
                        Err(vv_matting::Error::Cancelled) => {
                            self.failed.insert(hash, String::new());
                        }
                        Err(e) => {
                            self.failed.insert(hash, e.to_string());
                        }
                    }
                    progressed = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.running = None,
            }
        }
        if self.running.is_none()
            && let Some(request) = self.queue.pop_front()
        {
            self.running = Some(spawn(request));
        }
        progressed
    }

    /// Forgets a failure: the next `request` tries again.
    pub(crate) fn retry(&mut self, content_hash: u64) {
        self.failed.remove(&content_hash);
    }

    pub(crate) fn cancel(&mut self, content_hash: u64) {
        if self.queue.iter().any(|q| q.content_hash == content_hash) {
            self.failed.insert(content_hash, String::new());
        }
        self.queue.retain(|q| q.content_hash != content_hash);
        if let Some(running) = self
            .running
            .as_ref()
            .filter(|r| r.content_hash == content_hash)
        {
            running.cancel.store(true, Ordering::Relaxed);
        }
    }
}

fn spawn(request: MatteRequest) -> Running {
    let progress = Arc::new(vv_matting::Progress::default());
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, done) = mpsc::channel();
    let (job_progress, job_cancel) = (progress.clone(), cancel.clone());
    let hash = request.content_hash;
    std::thread::Builder::new()
        .name("matting".into())
        .spawn(move || {
            let result = match crate::subtitles::models_dir() {
                Some(dir) => vv_matting::matte_media(
                    &request.path,
                    request.content_hash,
                    request.frames,
                    request.fps,
                    &dir,
                    &job_progress,
                    &job_cancel,
                ),
                None => Err(vv_matting::Error::Download("no models folder".into())),
            };
            let _ = tx.send(result);
        })
        .expect("spawning the matting thread");
    Running {
        content_hash: hash,
        progress,
        cancel,
        done,
    }
}
