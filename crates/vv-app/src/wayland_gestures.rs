//! Touchpad pinch on Wayland, which winit 0.30 does not report (macOS/iOS
//! only). A pinch gesture bound to a pointer of our own, on a separate event
//! queue, reaches egui as `Event::Zoom` in `raw_input_hook`.

use std::sync::{Arc, Mutex};

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use sctk::reexports::client::globals::{GlobalListContents, registry_queue_init};
use sctk::reexports::client::protocol::{wl_pointer, wl_registry, wl_seat};
use sctk::reexports::client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum, backend::Backend, delegate_noop,
};
use sctk::reexports::protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
};

pub struct WaylandGestures {
    /// Zoom factor accumulated since the last frame.
    pending: Arc<Mutex<f32>>,
}

impl WaylandGestures {
    /// `None` outside Wayland or without the pointer-gestures protocol.
    pub fn start(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        let RawDisplayHandle::Wayland(handle) = cc.display_handle().ok()?.as_raw() else {
            return None;
        };
        // The display stays valid for the whole process lifetime: it is winit's.
        let backend = unsafe { Backend::from_foreign_display(handle.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
        let qh = queue.handle();
        let gestures: ZwpPointerGesturesV1 = globals.bind(&qh, 1..=3, ()).ok()?;
        let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=5, ()).ok()?;
        let pending = Arc::new(Mutex::new(1.0));
        let mut state = State {
            gestures,
            seat,
            pinch: None,
            last_scale: 1.0,
            pending: pending.clone(),
            ctx: cc.egui_ctx.clone(),
        };
        std::thread::Builder::new()
            .name("wayland-gestures".into())
            .spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {})
            .ok()?;
        Some(Self { pending })
    }

    pub fn feed(&self, raw_input: &mut egui::RawInput) {
        let factor = std::mem::replace(&mut *self.pending.lock().unwrap(), 1.0);
        if factor != 1.0 {
            raw_input.events.push(egui::Event::Zoom(factor));
        }
    }
}

struct State {
    gestures: ZwpPointerGesturesV1,
    seat: wl_seat::WlSeat,
    pinch: Option<(wl_pointer::WlPointer, ZwpPointerGesturePinchV1)>,
    /// `scale` of the last update: the protocol reports it since the begin.
    last_scale: f64,
    pending: Arc<Mutex<f32>>,
    ctx: egui::Context,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        else {
            return;
        };
        let has_pointer = caps.contains(wl_seat::Capability::Pointer);
        if has_pointer && state.pinch.is_none() {
            let pointer = state.seat.get_pointer(qh, ());
            let pinch = state.gestures.get_pinch_gesture(&pointer, qh, ());
            state.pinch = Some((pointer, pinch));
        } else if !has_pointer && let Some((pointer, pinch)) = state.pinch.take() {
            pinch.destroy();
            if pointer.version() >= 3 {
                pointer.release();
            }
        }
    }
}

impl Dispatch<ZwpPointerGesturePinchV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { .. } => state.last_scale = 1.0,
            zwp_pointer_gesture_pinch_v1::Event::Update { scale, .. } if scale > 0.0 => {
                let factor = (scale / state.last_scale) as f32;
                state.last_scale = scale;
                *state.pending.lock().unwrap() *= factor;
                state.ctx.request_repaint();
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_pointer::WlPointer);
delegate_noop!(State: ZwpPointerGesturesV1);
