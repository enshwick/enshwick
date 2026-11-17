use core::cell::Cell;
use core::num::NonZeroU64;

use super::{Engine, Missing, address, field, game_fn};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::pinned::{self, Symbol};
use crate::ui::window_node::{NAME_FITS, NAME_MAX, root_id};

/// `ui.node_find`: the entry the node map `nodes` holds under `id`, null for
/// an id it does not hold (client 26430625, `0x1411af900`).
type NodeFindFn = unsafe extern "system" fn(nodes: usize, id: u64) -> usize;

/// Frames in a row a shown dialog's popup root may be missing from the
/// game's node map before Back on dialogs turns off: a second at 60 frames,
/// so a root the game names another way is reported, not one odd frame.
const ROOT_MISSES_OFF: u32 = 60;

/// The game's node map, so Back binds only to a node the game holds:
/// `ui.action_query` reads through its lookup without a null check.
#[derive(Debug, Clone, Copy)]
pub(in crate::ui::interface) struct NodeMap {
    /// The lookup `ui.action_query` finds a bound node with.
    find: NodeFindFn,
    /// The interface context's field that holds the node map.
    context_nodes: usize,
}

impl NodeMap {
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        let lookup = |id: &str| loader.build.constant(id);
        Ok(Self {
            // SAFETY: `NodeFindFn` is the signature of `ui.node_find`, from
            // `ui.action_query`'s own call of it on client 26430625: the node
            // map in `rcx`, the id in `rdx`, the entry or null in `rax`.
            find: unsafe { game_fn::<NodeFindFn>(loader, pinned::ui::NODE_FIND) }?,
            context_nodes: field(&lookup, pinned::ui::CONTEXT_NODES.id())?,
        })
    }

    /// Whether the node map at `nodes` holds a node of id `id`; false for a
    /// null map.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    /// - `nodes` is null or the node map of this frame's context.
    unsafe fn holds(&self, nodes: usize, id: NonZeroU64) -> bool {
        // SAFETY: `self.find` is the game's `ui.node_find`, checked by
        // `game_fn`, called on the frame's thread with the non-zero node map
        // of this frame's context, by this function's `# Safety`, as
        // `ui.action_query` calls it; it only reads the map and gives null
        // for an id it does not hold.
        nodes != 0 && unsafe { (self.find)(nodes, id.get()) } != 0
    }
}

/// The ids the root node of the game's popup window can have, so a dialog
/// can bind Back to the popup it draws, and where `ui.window_begin` reads the
/// view slot that picks one.
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

/// Counts the frames in a row a shown dialog's popup root was missing from
/// the game's node map, `held` saying whether it was there in this one, and
/// turns `feature` off once the root stayed missing [`ROOT_MISSES_OFF`]
/// frames: the game then names its popup window otherwise than the loader
/// computes, and Back on dialogs would stay dead with no reason given.
pub(in crate::ui::interface) fn note_root(held: bool, misses: &Cell<u32>, feature: &Fuse) {
    if held {
        misses.set(0);
        return;
    }
    let count = misses.get().saturating_add(1);
    misses.set(count);
    if count == ROOT_MISSES_OFF {
        feature.report_off_with(
            "the game's node map holds no popup root of the id the loader computes",
            "loader dialogs close by their buttons",
        );
    }
}

impl Engine {
    /// The id of the root node of the game's popup window as
    /// `ui.window_begin` names it in this frame; `None` when the build gave
    /// no way to make it, Back on dialogs is off, or the view is not set.
    pub(in crate::ui::interface) fn popup_node(&self) -> Option<NonZeroU64> {
        let root = self.popup_root.as_ref()?;
        if super::optional::DIALOG_BACK_FUSE.is_off() {
            return None;
        }
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

    /// Whether the game's node map holds a node of id `id` in this frame;
    /// false outside a frame or where the build gave no lookup.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    pub(super) unsafe fn holds_node(&self, id: NonZeroU64) -> bool {
        let Some(map) = self.nodes else {
            return false;
        };
        let context = self.context();
        if context == 0 {
            return false;
        }
        // SAFETY: `context` is non-zero, checked by the guard before it, and
        // is the interface context of the frame being drawn, live until the
        // frame ends; its node map pointer sits at the table's
        // `ui.context_nodes`, as `ui.action_query` reads it.
        let nodes = unsafe { crate::engine::memory::read::<usize>(context, map.context_nodes) };
        // SAFETY: in a frame on its thread, by this function's `# Safety`,
        // with the node map of this frame's context.
        unsafe { map.holds(nodes, id) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn no_node(_: usize, _: u64) -> usize {
        0
    }

    unsafe extern "system" fn only_seven(_: usize, id: u64) -> usize {
        usize::from(id == 7)
    }

    fn popup_root() -> PopupRoot {
        let name = b"errorMessageWindow";
        PopupRoot {
            windows: 0,
            view: 0,
            slot: 0,
            plain: root_id(name, 0).unwrap(),
            suffixed: core::array::from_fn(|n| {
                root_id(name, 0x10 | i32::try_from(n).unwrap()).unwrap()
            }),
        }
    }

    #[test]
    fn the_root_id_follows_the_slot_as_the_game_names_the_window() {
        let root = popup_root();
        for slot in [0, 0x10, 0x20, 0x1b, -1, 0x7fff_fff5] {
            assert_eq!(
                Some(root.id(slot)),
                root_id(b"errorMessageWindow", slot),
                "slot {slot:#x}"
            );
        }
    }

    #[test]
    fn a_node_the_map_lacks_or_a_null_map_is_not_held() {
        let seven = NonZeroU64::new(7).unwrap();
        let eight = NonZeroU64::new(8).unwrap();
        let map = NodeMap {
            find: only_seven,
            context_nodes: 0x18,
        };
        let empty = NodeMap {
            find: no_node,
            ..map
        };
        // SAFETY: the fake lookups read no memory.
        let holds = |map: &NodeMap, nodes, id| unsafe { map.holds(nodes, id) };
        assert!(holds(&map, 1, seven));
        assert!(!holds(&map, 1, eight));
        assert!(!holds(&map, 0, seven));
        assert!(!holds(&empty, 1, seven));
    }

    #[test]
    fn a_popup_root_missing_for_a_second_of_frames_turns_back_on_dialogs_off() {
        let fuse = Fuse::new("test dialog back");
        let misses = Cell::new(0);
        for _ in 1..ROOT_MISSES_OFF {
            note_root(false, &misses, &fuse);
        }
        note_root(true, &misses, &fuse);
        assert_eq!(misses.get(), 0);
        for _ in 1..ROOT_MISSES_OFF {
            note_root(false, &misses, &fuse);
        }
        assert!(!fuse.is_off());
        note_root(false, &misses, &fuse);
        assert!(fuse.is_off());
    }
}
