//! MCP in the editor window (attach mode). The agent's calls run on a host
//! thread of their own, on the same `Session` the user edits: between two
//! frames the session sits in a shared slot, and the UI checks it out for
//! the length of each frame. So calls are answered even while the window is
//! not drawn: hidden or on another workspace, Wayland sends it no frames.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use vv_mcp::{
    Dispatch, McpRequest, Pending, PendingCalls, Reply, ToolCall, ToolError, ToolOutput, ToolResult,
};
use vv_session::{JobId, Session, SessionEvent};

use super::*;

/// How long `screenshot_ui` waits for the window to be drawn.
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct McpHost {
    shared: Arc<Shared>,
    /// `None` in the tests, which submit calls straight to the channel.
    server: Option<vv_mcp::SocketServer>,
    thread: Option<std::thread::JoinHandle<()>>,
}

struct Shared {
    ctx: egui::Context,
    slot: Mutex<Slot>,
    /// Signalled when the UI puts the session back.
    returned: Condvar,
    ui: Mutex<UiSide>,
    stop: AtomicBool,
    wake_host: vv_session::Waker,
}

struct Slot {
    /// The real session while the UI is between frames, a placeholder while
    /// the UI has it.
    session: Session,
    in_ui: bool,
}

/// What the UI and the host thread tell each other.
#[derive(Default)]
struct UiSide {
    /// Published by the UI at the end of each frame.
    mid_gesture: bool,
    modal_open: bool,
    state: vv_mcp::GuiState,
    /// Events the host ticked, for the UI's next frame.
    events_for_ui: Vec<SessionEvent>,
    /// Events the UI ticked, for the calls waiting on them.
    events_for_host: Vec<SessionEvent>,
    open_timeline: Option<TimelineId>,
    set_playhead: Option<i64>,
    set_playback: Option<bool>,
    start_ai_clips: Option<(vv_core::MediaId, String, usize)>,
    insert_ai_clips: Option<bool>,
    select_ai_clips: Option<Vec<usize>>,
    ai_clips_status: serde_json::Value,
    insert_sticker: Option<String>,
    sticker_status: serde_json::Value,
    screenshots: Vec<(Reply, Instant)>,
    /// Jobs the agent started: their outcome does not move the user's view.
    agent_jobs: HashSet<JobId>,
    last_tool: Option<&'static str>,
}

impl Shared {
    fn ui(&self) -> MutexGuard<'_, UiSide> {
        self.ui.lock().unwrap()
    }

    /// The session, once the UI is done with its frame. `None` when stopping.
    fn take_slot(&self) -> Option<MutexGuard<'_, Slot>> {
        let mut slot = self.slot.lock().unwrap();
        while slot.in_ui {
            if self.stop.load(Ordering::Relaxed) {
                return None;
            }
            slot = self
                .returned
                .wait_timeout(slot, Duration::from_millis(100))
                .unwrap()
                .0;
        }
        Some(slot)
    }
}

impl McpHost {
    /// Starts the host thread. The UI has the session: `session` is the real
    /// one, whose waker is extended to wake the host too.
    fn start(
        server: Option<vv_mcp::SocketServer>,
        inbox: vv_mcp::McpInbox,
        session: &mut Session,
        ctx: &egui::Context,
    ) -> Self {
        let wake_host = inbox.waker();
        let repaint = ctx.clone();
        session.set_waker(vv_session::Waker::new({
            let wake_host = wake_host.clone();
            move || {
                repaint.request_repaint();
                wake_host.wake();
            }
        }));
        let shared = Arc::new(Shared {
            ctx: ctx.clone(),
            slot: Mutex::new(Slot {
                session: Session::default(),
                in_ui: true,
            }),
            returned: Condvar::new(),
            ui: Mutex::new(UiSide::default()),
            stop: AtomicBool::new(false),
            wake_host,
        });
        let thread = std::thread::spawn({
            let (shared, ctx) = (shared.clone(), ctx.clone());
            move || host_loop(&shared, &inbox, &ctx)
        });
        Self {
            shared,
            server,
            thread: Some(thread),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        inbox: vv_mcp::McpInbox,
        session: &mut Session,
        ctx: &egui::Context,
    ) -> Self {
        Self::start(None, inbox, session, ctx)
    }

    pub(crate) fn clients(&self) -> usize {
        self.server.as_ref().map_or(0, |s| s.clients())
    }

    pub(crate) fn last_tool(&self) -> Option<&'static str> {
        self.shared.ui().last_tool
    }

    /// Stops the host thread and hands the real session back to `session`.
    fn shut_down(mut self, session: &mut Session) {
        self.stop_thread();
        let mut slot = self.shared.slot.lock().unwrap();
        if !slot.in_ui {
            std::mem::swap(session, &mut slot.session);
            slot.in_ui = true;
        }
    }

    fn stop_thread(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        // Closing the socket drops the transport's handles: the inbox
        // disconnects and the host loop ends.
        self.server = None;
        self.shared.wake_host.wake();
        self.shared.returned.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for McpHost {
    fn drop(&mut self) {
        self.stop_thread();
    }
}

fn reply(request: McpRequest, result: ToolResult) {
    let _ = request.reply.send(result);
}

fn host_loop(shared: &Shared, inbox: &vv_mcp::McpInbox, ctx: &egui::Context) {
    let mut pending = PendingCalls::default();
    let mut held: VecDeque<McpRequest> = VecDeque::new();
    loop {
        let idle = pending.is_empty() && held.is_empty();
        let mut requests = VecDeque::new();
        match inbox.recv_timeout(Duration::from_millis(if idle { 500 } else { 50 })) {
            Ok(Some(request)) => requests.push_back(request),
            Ok(None) => {}
            Err(vv_mcp::Disconnected) => return,
        }
        while let Some(request) = inbox.try_recv() {
            requests.push_back(request);
        }
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        expire_screenshots(shared);
        pending.poll();
        let events = std::mem::take(&mut shared.ui().events_for_host);
        if requests.is_empty() && held.is_empty() && pending.is_empty() && events.is_empty() {
            continue;
        }
        {
            let Some(mut slot) = shared.take_slot() else {
                return;
            };
            let session = &mut slot.session;
            for event in &events {
                pending.resolve(session, event);
            }
            // Only for the calls waiting on jobs: otherwise the UI ticks.
            // Not mid-gesture: the imports it applies would land in the
            // gesture's undo group.
            if !pending.is_empty() && !shared.ui().mid_gesture {
                let ticked = session.tick();
                for event in &ticked {
                    pending.resolve(session, event);
                }
                shared.ui().events_for_ui.extend(ticked);
            }
        }
        let mid_gesture = shared.ui().mid_gesture;
        if !mid_gesture {
            while let Some(request) = held.pop_back() {
                requests.push_front(request);
            }
        }
        for request in requests {
            if mid_gesture && request.call.mutates() {
                held.push_back(request);
            } else {
                handle_request(shared, ctx, &mut pending, request);
            }
        }
        ctx.request_repaint();
    }
}

fn expire_screenshots(shared: &Shared) {
    let mut ui = shared.ui();
    let (expired, waiting) = std::mem::take(&mut ui.screenshots)
        .into_iter()
        .partition(|(_, since)| since.elapsed() > SCREENSHOT_TIMEOUT);
    ui.screenshots = waiting;
    for (reply, _) in expired {
        let _ = reply.send(Err(ToolError(
            "the Tandem window is not being drawn (hidden or on another workspace?): \
             bring it to the front and retry"
                .into(),
        )));
    }
}

fn handle_request(
    shared: &Shared,
    ctx: &egui::Context,
    pending: &mut PendingCalls,
    request: McpRequest,
) {
    let modal_open = {
        let mut ui = shared.ui();
        ui.last_tool = Some(request.call.name());
        ui.modal_open
    };
    if request.call.mutates() && modal_open {
        return reply(
            request,
            Err(ToolError(
                "the user has a dialog open in Tandem: retry once it is closed".into(),
            )),
        );
    }
    match &request.call {
        ToolCall::SaveProject(_) => return save_project(shared, request),
        ToolCall::OpenProject(_) => return open_project(shared, request),
        ToolCall::ScreenshotUi => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            shared
                .ui()
                .screenshots
                .push((request.reply, Instant::now()));
            return;
        }
        _ => {}
    }
    let Some(mut slot) = shared.take_slot() else {
        return;
    };
    let session = &mut slot.session;
    match &request.call {
        ToolCall::GetState => {
            let ui = shared.ui();
            let mut state = vv_mcp::state_json(session, &ui.state);
            state["sticker"] = ui.sticker_status.clone();
            return reply(request, Ok(ToolOutput::json(state)));
        }
        ToolCall::SetActiveTimeline(args) => {
            let result = vv_mcp::parse_timeline_id(&session.project, &args.timeline_id).map(|id| {
                shared.ui().open_timeline = Some(id);
                ToolOutput::json(serde_json::json!({ "active_timeline": args.timeline_id }))
            });
            return reply(request, result);
        }
        ToolCall::SetPlayhead(args) => {
            let frame = args.frame;
            if frame < 0 {
                return reply(request, Err(ToolError("frame must not be negative".into())));
            }
            if shared.ui().state.active_timeline.is_none() {
                return reply(request, Err(ToolError("no active timeline".into())));
            }
            shared.ui().set_playhead = Some(frame);
            ctx.request_repaint();
            return reply(
                request,
                Ok(ToolOutput::json(serde_json::json!({ "frame": frame }))),
            );
        }
        ToolCall::SetPlayback(args) => {
            let playing = args.playing;
            if shared.ui().state.active_timeline.is_none() {
                return reply(request, Err(ToolError("no active timeline".into())));
            }
            shared.ui().set_playback = Some(playing);
            ctx.request_repaint();
            return reply(
                request,
                Ok(ToolOutput::json(serde_json::json!({ "playing": playing }))),
            );
        }
        ToolCall::StartAiClips(args) => {
            if args.duration > 2 {
                return reply(request, Err(ToolError("duration must be 0, 1 or 2".into())));
            }
            let media = match vv_mcp::parse_media_id(&session.project, &args.media_id) {
                Ok(id) => id,
                Err(error) => return reply(request, Err(error)),
            };
            let item = &session.project.media_pool[media];
            if !item.meta.has_video || !item.meta.has_audio || item.compound.is_some() {
                return reply(
                    request,
                    Err(ToolError("AI Clips needs a video with audio".into())),
                );
            }
            shared.ui().start_ai_clips = Some((media, args.prompt.clone(), args.duration));
            ctx.request_repaint();
            return reply(
                request,
                Ok(ToolOutput::json(
                    serde_json::json!({ "status": "scheduled" }),
                )),
            );
        }
        ToolCall::GetAiClips => {
            return reply(
                request,
                Ok(ToolOutput::json(shared.ui().ai_clips_status.clone())),
            );
        }
        ToolCall::SelectAiClips(args) => {
            let indices = args.indices.clone();
            let count = shared.ui().ai_clips_status["suggestions"]
                .as_array()
                .map_or(0, Vec::len);
            if indices.iter().any(|&index| index >= count) {
                return reply(
                    request,
                    Err(ToolError(format!("suggestion index must be below {count}"))),
                );
            }
            shared.ui().select_ai_clips = Some(indices.clone());
            ctx.request_repaint();
            return reply(
                request,
                Ok(ToolOutput::json(serde_json::json!({ "selected": indices }))),
            );
        }
        ToolCall::InsertAiClips(args) => {
            if shared.ui().state.active_timeline.is_none() {
                return reply(request, Err(ToolError("no active timeline".into())));
            }
            let status = shared.ui().ai_clips_status.clone();
            if status["running"] == true
                || status["suggestions"].as_array().is_none_or(Vec::is_empty)
            {
                return reply(
                    request,
                    Err(ToolError("AI Clips suggestions are not ready".into())),
                );
            }
            shared.ui().insert_ai_clips = Some(args.replace_previous);
            ctx.request_repaint();
            return reply(
                request,
                Ok(ToolOutput::json(
                    serde_json::json!({ "status": "scheduled" }),
                )),
            );
        }
        ToolCall::ListStickers(args) => {
            let query = args.query.as_deref().unwrap_or_default().to_lowercase();
            let matches: Vec<_> = crate::stickers::all()
                .iter()
                .filter(|sticker| {
                    args.category
                        .as_deref()
                        .is_none_or(|category| sticker.category.id() == category)
                })
                .filter(|sticker| {
                    query.is_empty()
                        || sticker.name.to_lowercase().contains(&query)
                        || sticker.id.contains(&query)
                })
                .collect();
            let total = matches.len();
            let stickers: Vec<_> = matches
                .into_iter()
                .skip(args.offset)
                .take(args.limit.unwrap_or(100).min(500))
                .map(|sticker| {
                    serde_json::json!({
                        "id": sticker.id,
                        "name": sticker.name,
                        "category": sticker.category.id(),
                        "animated": sticker.animated,
                    })
                })
                .collect();
            return reply(
                request,
                Ok(ToolOutput::json(
                    serde_json::json!({ "stickers": stickers, "total": total }),
                )),
            );
        }
        ToolCall::InsertSticker(args) => {
            if crate::stickers::find(&args.sticker_id).is_none() {
                return reply(request, Err(ToolError("unknown sticker id".into())));
            }
            if shared.ui().state.active_timeline.is_none() {
                return reply(request, Err(ToolError("no active timeline".into())));
            }
            shared.ui().insert_sticker = Some(args.sticker_id.clone());
            shared.ui().sticker_status =
                serde_json::json!({ "id": args.sticker_id, "status": "pending" });
            ctx.request_repaint();
            return reply(
                request,
                Ok(ToolOutput::json(
                    serde_json::json!({ "status": "scheduled" }),
                )),
            );
        }
        ToolCall::NewProject if session.has_unsaved_changes() => {
            return reply(request, Err(unsaved_changes()));
        }
        _ => {}
    }
    let McpRequest { call, reply: to } = request;
    match vv_mcp::dispatch(session, call) {
        Dispatch::Handled(result) => {
            let _ = to.send(result);
        }
        Dispatch::Deferred(pending_call) => {
            if let Pending::Import { job, .. } | Pending::Otio { job, .. } = &pending_call {
                shared.ui().agent_jobs.insert(*job);
            }
            pending.push(pending_call, to);
        }
    }
}

fn unsaved_changes() -> ToolError {
    ToolError(
        "the project open in Tandem has unsaved changes: save it first (save_project) \
         or ask the user"
            .into(),
    )
}

/// Writes a copy, so the UI is not held while the file is written.
fn save_project(shared: &Shared, request: McpRequest) {
    let ToolCall::SaveProject(args) = &request.call else {
        return;
    };
    let (project, path, mark) = {
        let Some(slot) = shared.take_slot() else {
            return;
        };
        let path = match (&args.path, slot.session.path()) {
            (Some(path), _) => PathBuf::from(path),
            (None, Some(current)) => current.to_path_buf(),
            (None, None) => {
                return reply(
                    request,
                    Err(ToolError("the project has no file yet: pass `path`".into())),
                );
            }
        };
        (
            slot.session.project.clone(),
            path,
            slot.session.change_mark(),
        )
    };
    let result = vv_core::save_project(&project, &path)
        .map_err(|e| ToolError(format!("cannot save to {}: {e}", path.display())));
    if result.is_ok()
        && let Some(mut slot) = shared.take_slot()
    {
        slot.session.finish_save(path.clone(), mark);
    }
    reply(
        request,
        result.map(|()| ToolOutput::json(serde_json::json!({ "path": path }))),
    );
}

/// Reads the file (and the media it lists) before taking the session.
fn open_project(shared: &Shared, request: McpRequest) {
    let ToolCall::OpenProject(args) = &request.call else {
        return;
    };
    let unsaved = shared
        .take_slot()
        .is_some_and(|slot| slot.session.has_unsaved_changes());
    if unsaved {
        return reply(request, Err(unsaved_changes()));
    }
    let path = PathBuf::from(&args.path);
    let loaded = vv_core::load_project(&path).map(|mut project| {
        vv_session::complete_legacy_media(&mut project);
        project
    });
    let result = match loaded {
        Err(e) => Err(ToolError(format!("cannot open {}: {e}", args.path))),
        Ok(project) => match shared.take_slot() {
            None => return,
            // The user may have edited meanwhile.
            Some(slot) if slot.session.has_unsaved_changes() => Err(unsaved_changes()),
            Some(mut slot) => {
                slot.session.install_project(project, Some(path));
                Ok(ToolOutput::json(vv_mcp::project_json(&slot.session)))
            }
        },
    };
    reply(request, result);
}

impl VenturiApp {
    /// Starts or stops serving MCP to follow the setting (or `--mcp`). The UI
    /// must have the session: called during a frame, or before the first.
    pub(crate) fn apply_mcp_setting(&mut self, ctx: &egui::Context) {
        let wanted = self.settings.mcp_enabled || self.mcp_forced;
        match (wanted, self.mcp.is_some()) {
            (true, false) => {
                let repaint = ctx.clone();
                let (handle, inbox) = vv_mcp::channel(move || repaint.request_repaint());
                match vv_mcp::SocketServer::start(handle) {
                    Ok(server) => {
                        self.mcp =
                            Some(McpHost::start(Some(server), inbox, &mut self.session, ctx));
                        self.seen_epoch = self.session.epoch();
                    }
                    Err(e) => {
                        self.project_error =
                            Some(t!("settings.mcp_failed", error = e.to_string()).into_owned())
                    }
                }
            }
            (false, true) => {
                if let Some(mcp) = self.mcp.take() {
                    mcp.shut_down(&mut self.session);
                }
                let repaint = ctx.clone();
                self.session
                    .set_waker(vv_session::Waker::new(move || repaint.request_repaint()));
            }
            _ => {}
        }
    }

    /// At the start of a frame: takes the session from the slot (waiting for
    /// a call in progress to finish) and catches up with what the agent did
    /// meanwhile.
    pub(crate) fn mcp_checkout(&mut self) {
        let Some(mcp) = &self.mcp else {
            return;
        };
        let shared = mcp.shared.clone();
        {
            let mut slot = shared.slot.lock().unwrap();
            if !slot.in_ui {
                std::mem::swap(&mut self.session, &mut slot.session);
                slot.in_ui = true;
            }
        }
        if self.session.epoch() != self.seen_epoch {
            self.reset_for_replaced_project();
        }
        let (
            events,
            open_timeline,
            set_playhead,
            set_playback,
            start_ai_clips,
            select_ai_clips,
            insert_ai_clips,
            insert_sticker,
        ) = {
            let mut ui = shared.ui();
            (
                std::mem::take(&mut ui.events_for_ui),
                ui.open_timeline.take(),
                ui.set_playhead.take(),
                ui.set_playback.take(),
                ui.start_ai_clips.take(),
                ui.select_ai_clips.take(),
                ui.insert_ai_clips.take(),
                ui.insert_sticker.take(),
            )
        };
        for event in events {
            self.handle_session_event(event);
        }
        if let Some(id) =
            open_timeline.filter(|&id| self.session.project.timelines.contains_key(id))
        {
            self.stop_browsing();
            self.open_timeline(id);
            self.spawn_render_ahead_if_needed(id);
        }
        if let Some(frame) = set_playhead {
            if let Some(id) = self.timeline_id {
                let end = self.session.project.timelines[id].total_frames();
                self.timeline_state.playhead = frame.clamp(0, (end - 1).max(0));
                self.timeline_state.reveal_playhead = true;
            }
        }
        if let Some(playing) = set_playback {
            if self.timeline_id.is_some() && self.is_timeline_playing() != playing {
                self.toggle_playback();
            }
        }
        if let Some((media, prompt, duration)) = start_ai_clips {
            self.mcp_start_ai_clips(media, prompt, duration);
        }
        if let Some(indices) = select_ai_clips {
            self.mcp_select_ai_clips(&indices);
        }
        if let Some(replace) = insert_ai_clips {
            self.mcp_insert_ai_clips(replace);
        }
        if let Some(id) = insert_sticker {
            if self.stickers.store.is_none()
                && let Some(dir) = crate::stickers::default_dir()
            {
                self.stickers.store = Some(crate::stickers::Store::new(dir, &shared.ctx));
            }
            if let Some(store) = &mut self.stickers.store {
                store.poll();
            }
            match self.sticker_asset_id(&id) {
                Some(asset) => {
                    self.insert_assets_at_playhead(&[asset], true);
                    shared.ui().sticker_status =
                        serde_json::json!({ "id": id, "status": "inserted" });
                }
                None if self
                    .stickers
                    .store
                    .as_ref()
                    .is_some_and(|store| store.failed(&id)) =>
                {
                    self.import_warnings
                        .push(format!("sticker {id} could not be downloaded"));
                    shared.ui().sticker_status =
                        serde_json::json!({ "id": id, "status": "failed" });
                }
                None => {
                    shared.ui().insert_sticker = Some(id);
                    shared.ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
        }
        // The progress window shows the agent's export like the user's.
        if self.export.is_none()
            && let Some(job) = self.session.running_export()
            && let Some(progress) = self.session.export_progress(job)
        {
            self.pause_proxies_for_export();
            self.export = Some(ExportUiState { job, progress });
        }
    }

    /// Takes the session for work outside the frame (`App::logic`), without
    /// the catch-up `mcp_checkout` does. `None` when the agent replaced the
    /// project and the next frame must reconcile first; otherwise whether
    /// the session had to be taken from the slot (give it back with
    /// `mcp_return_session`).
    pub(crate) fn mcp_borrow_session(&mut self) -> Option<bool> {
        let Some(mcp) = &self.mcp else {
            return Some(false);
        };
        let shared = mcp.shared.clone();
        let mut slot = shared.slot.lock().unwrap();
        // Between frames `self.session` is only a placeholder.
        if slot.in_ui {
            return (self.session.epoch() == self.seen_epoch).then_some(false);
        }
        if slot.session.epoch() != self.seen_epoch {
            return None;
        }
        std::mem::swap(&mut self.session, &mut slot.session);
        slot.in_ui = true;
        Some(true)
    }

    pub(crate) fn mcp_return_session(&mut self) {
        let Some(mcp) = &self.mcp else {
            return;
        };
        let shared = mcp.shared.clone();
        let mut slot = shared.slot.lock().unwrap();
        if slot.in_ui {
            std::mem::swap(&mut self.session, &mut slot.session);
            slot.in_ui = false;
        }
        drop(slot);
        shared.returned.notify_all();
    }

    /// At the end of a frame: publishes what the host needs to know and puts
    /// the session back.
    pub(crate) fn mcp_checkin(&mut self, ctx: &egui::Context) {
        let Some(mcp) = &self.mcp else {
            return;
        };
        let shared = mcp.shared.clone();
        {
            let mut ui = shared.ui();
            ui.mid_gesture = self.user_mid_gesture(ctx);
            ui.modal_open = self.modal_open();
            ui.state = self.gui_state();
            ui.ai_clips_status = self.mcp_ai_clips_status();
        }
        let mut slot = shared.slot.lock().unwrap();
        if slot.in_ui {
            std::mem::swap(&mut self.session, &mut slot.session);
            slot.in_ui = false;
        }
        drop(slot);
        shared.returned.notify_all();
    }

    /// Events the UI ticked also go to the agent's calls waiting on them.
    pub(crate) fn mcp_session_event(&mut self, event: &SessionEvent) {
        if let Some(mcp) = &self.mcp {
            mcp.shared.ui().events_for_host.push(event.clone());
            mcp.shared.wake_host.wake();
        }
    }

    pub(crate) fn is_agent_job(&self, job: JobId) -> bool {
        self.mcp
            .as_ref()
            .is_some_and(|mcp| mcp.shared.ui().agent_jobs.contains(&job))
    }

    /// The user is in the middle of a pointer gesture: a drag on the
    /// timeline, a slider, the viewer's handles.
    fn user_mid_gesture(&self, ctx: &egui::Context) -> bool {
        self.edit_drag_group.is_some()
            || self.timeline_state.gesture_active()
            || ctx.input(|i| i.pointer.any_down())
    }

    /// A dialog is waiting for the user's answer about the project.
    fn modal_open(&self) -> bool {
        self.pending_project_switch.is_some()
            || self.session.otio_awaiting_decision().is_some()
            || self.forced_relink.is_some()
    }

    fn gui_state(&self) -> vv_mcp::GuiState {
        vv_mcp::GuiState {
            active_timeline: self.timeline_id,
            timeline_stack: self.timeline_stack.clone(),
            playhead: self.timeline_state.playhead,
            playing: self.is_timeline_playing(),
            selected_clips: self
                .timeline_state
                .selected
                .iter()
                .map(|&(_, id)| id)
                .collect(),
            selected_media: self.media_pool_state.selected.iter().copied().collect(),
            previewed_media: match &self.browsing {
                Some(Browsed::Media(id)) => Some(*id),
                _ => None,
            },
        }
    }

    /// eframe hands a capture over at the start of the frame after it: an
    /// idle UI must draw that frame too.
    pub(crate) fn mcp_awaits_screenshot(&self) -> bool {
        self.mcp
            .as_ref()
            .is_some_and(|mcp| !mcp.shared.ui().screenshots.is_empty())
    }

    /// Answers the `screenshot_ui` calls with the frame egui just captured.
    pub(crate) fn mcp_screenshot(&mut self, image: &egui::ColorImage) {
        let Some(mcp) = &self.mcp else {
            return;
        };
        let waiting = std::mem::take(&mut mcp.shared.ui().screenshots);
        if waiting.is_empty() {
            return;
        }
        let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
        let size = (image.size[0] as u32, image.size[1] as u32);
        let result = vv_mcp::encode_png(&rgba, size).map(|png| ToolOutput {
            value: serde_json::json!({ "width": size.0, "height": size.1 }),
            image_png: Some(png),
        });
        for (reply, _) in waiting {
            let _ = reply.send(result.clone());
        }
    }
}

#[cfg(test)]
#[path = "tests/mcp_host.rs"]
mod tests;
