//! Dropping files from the file manager on Wayland, where winit 0.30 does not
//! handle it (X11 only), and reading images from the clipboard, which
//! egui-winit reads only as text. The events come from egui-winit's own
//! clipboard data device (see `vendor/smithay-clipboard/src/dnd.rs`): some
//! compositors talk only to the first data device of a client, and that one
//! is created before the app starts. The files reach egui in
//! `raw_input_hook`.

use std::ffi::OsString;
use std::io::Read;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use sctk::data_device_manager::ReadPipe;
use sctk::data_device_manager::data_offer::{DragOffer, SelectionOffer};
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::{Connection, backend::Backend};

use crate::paste_image::IMAGE_MIME_TYPES;

const URI_LIST: &str = "text/uri-list";

#[derive(Default)]
struct Shared {
    hovering: bool,
    /// Where the drag is, in surface coordinates: egui gets no pointer
    /// events during it, and the drop target depends on it.
    position: Option<(f64, f64)>,
    dropped: Vec<PathBuf>,
    selection: Option<SelectionOffer>,
}

pub struct WaylandDnd {
    shared: Arc<Mutex<Shared>>,
    conn: Connection,
}

impl WaylandDnd {
    /// `None` outside Wayland.
    pub fn start(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        let RawDisplayHandle::Wayland(handle) = cc.display_handle().ok()?.as_raw() else {
            return None;
        };
        // The display stays valid for the whole process lifetime: it is winit's.
        let backend = unsafe { Backend::from_foreign_display(handle.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        let shared = Arc::new(Mutex::new(Shared::default()));
        let hooks = Hooks {
            shared: shared.clone(),
            ctx: cc.egui_ctx.clone(),
        };
        if !smithay_clipboard::dnd::set_hooks(Box::new(hooks)) {
            return None;
        }
        Some(Self { shared, conn })
    }

    /// The pipe the clipboard's image arrives from, and its extension;
    /// `None` if the clipboard holds no image.
    pub fn clipboard_image(&self) -> Option<(ReadPipe, &'static str)> {
        let offer = self.shared.lock().unwrap().selection.clone()?;
        let (mime, extension) = offer.with_mime_types(|mimes| {
            IMAGE_MIME_TYPES
                .into_iter()
                .find(|(mime, _)| mimes.iter().any(|m| m == mime))
        })?;
        let pipe = offer.receive(mime.to_string()).ok()?;
        let _ = self.conn.flush();
        Some((pipe, extension))
    }

    /// `zoom`: egui's zoom factor, between surface coordinates and points.
    pub fn feed(&self, raw_input: &mut egui::RawInput, zoom: f32) {
        let mut shared = self.shared.lock().unwrap();
        if shared.hovering {
            raw_input.hovered_files.push(egui::HoveredFile::default());
        }
        if let Some((x, y)) = shared.position.take() {
            raw_input.events.push(egui::Event::PointerMoved(
                egui::pos2(x as f32, y as f32) / zoom,
            ));
        }
        raw_input.dropped_files.extend(
            shared.dropped.drain(..).map(|path| {
                Arc::new(DroppedPath(path)) as Arc<dyn egui::DroppedFile + Send + Sync>
            }),
        );
    }
}

#[derive(Debug)]
struct DroppedPath(PathBuf);

impl egui::DroppedFile for DroppedPath {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

struct Hooks {
    shared: Arc<Mutex<Shared>>,
    ctx: egui::Context,
}

impl Hooks {
    fn set_hovering(&self, hovering: bool) {
        self.shared.lock().unwrap().hovering = hovering;
        self.ctx.request_repaint();
    }
}

impl smithay_clipboard::dnd::DndHooks for Hooks {
    fn enter(&self, conn: &Connection, offer: Option<DragOffer>, x: f64, y: f64) {
        self.shared.lock().unwrap().position = Some((x, y));
        let Some(offer) = offer else {
            return;
        };
        let has_files = offer.with_mime_types(|mimes| mimes.iter().any(|m| m == URI_LIST));
        offer.accept_mime_type(offer.serial, has_files.then(|| URI_LIST.to_string()));
        if has_files {
            offer.set_actions(DndAction::Copy, DndAction::Copy);
        }
        let _ = conn.flush();
        self.set_hovering(has_files);
    }

    fn motion(&self, x: f64, y: f64) {
        self.shared.lock().unwrap().position = Some((x, y));
        self.ctx.request_repaint();
    }

    fn leave(&self) {
        self.set_hovering(false);
    }

    fn drop_performed(&self, conn: &Connection, offer: Option<DragOffer>) {
        self.set_hovering(false);
        let Some(offer) = offer else {
            return;
        };
        let Ok(mut pipe) = offer.receive(URI_LIST.to_string()) else {
            offer.destroy();
            return;
        };
        let _ = conn.flush();
        // The read waits for the source client: it must not block the queue.
        let conn = conn.clone();
        let shared = self.shared.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let mut uri_list = Vec::new();
            let _ = pipe.read_to_end(&mut uri_list);
            offer.finish();
            offer.destroy();
            let _ = conn.flush();
            shared
                .lock()
                .unwrap()
                .dropped
                .extend(parse_uri_list(&uri_list));
            ctx.request_repaint();
        });
    }

    fn source_actions(&self, offer: &mut DragOffer) {
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn selection(&self, offer: Option<SelectionOffer>) {
        self.shared.lock().unwrap().selection = offer;
    }
}

/// Local paths from a `text/uri-list` (RFC 2483): comments and non-`file:`
/// URIs are discarded.
fn parse_uri_list(bytes: &[u8]) -> Vec<PathBuf> {
    bytes
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.starts_with(b"#"))
        .filter_map(|line| line.strip_prefix(b"file://"))
        .filter_map(|rest| {
            // After `file://` comes the host, usually empty or `localhost`.
            let path = &rest[rest.iter().position(|&b| b == b'/')?..];
            Some(PathBuf::from(OsString::from_vec(percent_decode(path))))
        })
        .collect()
}

fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/wayland_dnd.rs"]
mod tests;
