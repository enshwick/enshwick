use core::num::NonZeroU64;

use super::{Engine, Missing, address, field};
use crate::engine::loader::Loader;
use crate::engine::pinned::{self, Symbol};
use crate::ui::window_node::{NAME_FITS, NAME_MAX, root_id};

/// The ids the root node of the game's popup window can have, so a dialog
/// can bind Back to the popup it draws, and where `ui.window_begin` reads
/// the view slot that picks one.
#[derive(Debug, Clone, Copy)]
pub(in crate::ui::interface) struct PopupRoot {
    /// The address of the global `ui.window_begin` reads its view from.
    windows: usize,
    view: usize,
    slot: usize,
    /// The id while the slot is zero, when the name has no suffix.
    plain: NonZeroU64,
    /// The id for each value of the slot's low four bits otherwise.
    suffixed: [NonZeroU64; 16],
}

impl PopupRoot {
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        let lookup = |id: &str| loader.build.constant(id);
        let symbol = |name: Symbol| address(loader, name).ok_or(Missing::Entry(name.id()));
        let at = symbol(pinned::ui::POPUP_WINDOW)?;
        let len = field(&lookup, pinned::ui::POPUP_WINDOW_LEN.id())?;
        if len > NAME_FITS {
            return Err(Missing::Room(pinned::ui::POPUP_WINDOW_LEN.id()));
        }
        let mut bytes = [0u8; NAME_MAX];
        let name = &mut bytes[..len];
        // SAFETY: the route of `ui.popup_window` takes the name `ui.popup`
        // hands `ui.window_begin`, inside the executable's `.rdata` or
        // `.data`, mapped for the process and never written; `len` is the
        // length `ui.popup` passes with it, so the game reads these same
        // bytes each time it draws the popup.
        unsafe { crate::engine::memory::read_bytes(at, 0, name) };
        if name.is_empty() || !name.iter().all(u8::is_ascii_graphic) {
            return Err(Missing::Bytes(pinned::ui::POPUP_WINDOW.id()));
        }
        let id =
            |slot: i32| root_id(name, slot).ok_or(Missing::Bytes(pinned::ui::POPUP_WINDOW.id()));
        let mut suffixed = [NonZeroU64::MIN; 16];
        for (n, id_n) in (0..).zip(&mut suffixed) {
            // Low four bits `n` with a bit above them, so the slot is non-zero.
            *id_n = id(0x10 | n)?;
        }
        Ok(Self {
            windows: symbol(pinned::ui::WINDOWS)?,
            view: field(&lookup, pinned::ui::WINDOWS_VIEW.id())?,
            slot: field(&lookup, pinned::ui::VIEW_SLOT.id())?,
            plain: id(0)?,
            suffixed,
        })
    }

    /// The root node's id while the view's slot is `slot`.
    fn id(&self, slot: i32) -> NonZeroU64 {
        if slot == 0 {
            self.plain
        } else {
            self.suffixed[usize::from(slot.to_le_bytes()[0] & 0xf)]
        }
    }
}

impl Engine {
    /// The id of the root node of the game's popup window as
    /// `ui.window_begin` names it in this frame; `None` when the build gave
    /// no way to make it or the view is not set.
    pub(in crate::ui::interface) fn popup_node(&self) -> Option<NonZeroU64> {
        let root = self.popup_root.as_ref()?;
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
        Some(root.id(slot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_id_follows_the_slot_as_the_game_names_the_window() {
        let name = b"errorMessageWindow";
        let root = PopupRoot {
            windows: 0,
            view: 0,
            slot: 0,
            plain: root_id(name, 0).unwrap(),
            suffixed: core::array::from_fn(|n| {
                root_id(name, 0x10 | i32::try_from(n).unwrap()).unwrap()
            }),
        };
        for slot in [0, 0x10, 0x20, 0x1b, -1, 0x7fff_fff5] {
            assert_eq!(Some(root.id(slot)), root_id(name, slot), "slot {slot:#x}");
        }
    }
}
