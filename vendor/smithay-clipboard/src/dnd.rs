//! Venturi patch: forwards drag-and-drop and selection events of this
//! crate's data device. Some compositors (Hyprland) only talk to the first
//! data device of a client, and this crate creates it before the app can.

use std::sync::OnceLock;

use sctk::data_device_manager::data_offer::{DragOffer, SelectionOffer};
use sctk::reexports::client::Connection;

pub trait DndHooks: Send + Sync + 'static {
    fn enter(&self, conn: &Connection, offer: Option<DragOffer>, x: f64, y: f64);
    fn motion(&self, x: f64, y: f64);
    fn leave(&self);
    fn drop_performed(&self, conn: &Connection, offer: Option<DragOffer>);
    fn source_actions(&self, offer: &mut DragOffer);
    fn selection(&self, offer: Option<SelectionOffer>);
}

static HOOKS: OnceLock<Box<dyn DndHooks>> = OnceLock::new();

/// Installs the hooks; only the first call wins.
pub fn set_hooks(hooks: Box<dyn DndHooks>) -> bool {
    HOOKS.set(hooks).is_ok()
}

pub(crate) fn hooks() -> Option<&'static dyn DndHooks> {
    HOOKS.get().map(|h| h.as_ref())
}
