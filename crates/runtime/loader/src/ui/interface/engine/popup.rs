use core::num::NonZeroU64;

use super::{Engine, Missing, address, field};
use crate::engine::loader::Loader;
use crate::engine::pinned::{self, Symbol};
use crate::ui::window_node::{NAME_MAX, root_id};

/// What the root node id of the game's popup window is made of, so a
/// dialog can bind Back to the popup it draws: the window name `ui.popup`
/// opens, and where `ui.window_begin` reads the view slot that suffixes
/// every window name.
#[derive(Debug, Clone, Copy)]
pub(in crate::ui::interface) struct PopupRoot {
    name: &'static [u8],
    /// The address of the global `ui.window_begin` reads its view from.
    windows: usize,
    view: usize,
    slot: usize,
}

impl PopupRoot {
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        let lookup = |id: &str| loader.build.constant(id);
        let symbol = |name: Symbol| address(loader, name).ok_or(Missing::Entry(name.id()));
        let at = symbol(pinned::ui::POPUP_WINDOW)?;
        let len = field(&lookup, pinned::ui::POPUP_WINDOW_LEN.id())?;
        if len > NAME_MAX {
            return Err(Missing::Room(pinned::ui::POPUP_WINDOW_LEN.id()));
        }
        // SAFETY: the route of `ui.popup_window` takes the name `ui.popup`
        // hands `ui.window_begin` and counts it only inside the executable's
        // `.rdata` or `.data`, mapped for the process and never freed; `len`
        // is the length `ui.popup` passes with it, at most `NAME_MAX`.
        let name = unsafe { core::slice::from_raw_parts(at as *const u8, len) };
        if name.is_empty() || !name.iter().all(u8::is_ascii_graphic) {
            return Err(Missing::Bytes(pinned::ui::POPUP_WINDOW.id()));
        }
        Ok(Self {
            name,
            windows: symbol(pinned::ui::WINDOWS)?,
            view: field(&lookup, pinned::ui::WINDOWS_VIEW.id())?,
            slot: field(&lookup, pinned::ui::VIEW_SLOT.id())?,
        })
    }
}

impl Engine {
    /// The id of the root node of the game's popup window as
    /// `ui.window_begin` names it in this frame; `None` when the build gave
    /// no way to make it or the view is not set.
    pub(in crate::ui::interface) fn popup_node(&self) -> Option<NonZeroU64> {
        let root = self.popup_root?;
        // SAFETY: `root.windows` is the address of the global
        // `ui.window_begin` loads, from its route, eight bytes of the
        // executable's data mapped for the process.
        let windows = unsafe { crate::engine::memory::read::<usize>(root.windows, 0) };
        if windows == 0 {
            return None;
        }
        // SAFETY: `windows` is non-zero, checked by the guard before it, and
        // points at the object `ui.window_begin` reads its view pointer from
        // at the table's `ui.windows_view`.
        let view = unsafe { crate::engine::memory::read::<usize>(windows, root.view) };
        if view == 0 {
            return None;
        }
        // SAFETY: `view` is non-zero, checked by the guard before it, and
        // points at the view whose `i32` slot sits at the table's
        // `ui.view_slot`, as `ui.window_begin` reads it.
        let slot = unsafe { crate::engine::memory::read::<i32>(view, root.slot) };
        root_id(root.name, slot)
    }
}
