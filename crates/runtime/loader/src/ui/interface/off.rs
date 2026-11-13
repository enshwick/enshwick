//! What turns off with the interface: the parts' features and what the
//! interface alone serves.

use crate::engine::fuse::{Fuse, Off};
use crate::engine::interface_parts::Part;

use super::{DIALOGS_FUSE, HUD_FUSE, INTERFACE_FUSE, MODS_SCREEN_FUSE, dialog, mods};

/// The feature of each of the interface's optional parts, in the order of
/// [`Part::ALL`]: its links and its work run under it.
pub(super) static PART_FUSES: [&Fuse; Part::ALL.len()] =
    [&mods::MAIN_MENU_FUSE, &mods::PAUSE_MENU_FUSE, &HUD_FUSE];

/// Why a part the interface serves is off when the interface is.
const WITH_INTERFACE: &str = "the interface it runs in is off";

/// Turns off with the interface the parts only it serves: the menus' Mods
/// entries, which open a screen it draws, the Mods screen and the mod
/// control keys, which it reads each frame.
#[cold]
#[inline(never)]
pub(super) fn parts_off_with_interface() {
    for fuse in [
        &mods::MAIN_MENU_FUSE,
        &mods::PAUSE_MENU_FUSE,
        &MODS_SCREEN_FUSE,
    ] {
        fuse.turn_off(&Off::fault(fuse.feature(), WITH_INTERFACE));
    }
    crate::actions::controls::keys_off(WITH_INTERFACE);
}

/// After a frame in which the interface or its dialogs turned off: the
/// dialogs show as system message boxes, and with the interface off, the
/// parts only it serves turn off too.
#[cold]
#[inline(never)]
pub(super) fn drawn_no_more() {
    dialog::fall_back();
    if INTERFACE_FUSE.is_off() {
        parts_off_with_interface();
    }
}

/// Whether the dialogs draw in game: the interface and its dialogs are on.
pub(super) fn dialogs_drawn() -> bool {
    both_on(&INTERFACE_FUSE, &DIALOGS_FUSE)
}

/// Whether `outer` and `inner`, a part drawn inside it, are both on.
pub(super) fn both_on(outer: &Fuse, inner: &Fuse) -> bool {
    !outer.is_off() && !inner.is_off()
}
