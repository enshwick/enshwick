//! The main menu's line under the game's version: the loader's version, how
//! many mods load and what is off.

use core::cell::RefCell;
use std::sync::atomic::Ordering;

use enshwick::abi::Registers;

use crate::fuse::Fuse;
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;

use super::engine::Engine;
use super::{ENGINE, copy};

thread_local! {
    /// The line, with the count of mods off it was made for, so a frame
    /// only builds it again when that count changes.
    static LINE: RefCell<Option<(usize, String)>> = const { RefCell::new(None) };
}

/// Turns the line off, alone, at its first panic.
static FUSE: Fuse = Fuse::new("the main menu's Enshwick line");

/// Links the hook between the version line and the legal line, on a build
/// whose site holds and that has the version line's look.
pub(super) fn link(loader: &Loader, engine: &Engine) {
    let build = &loader.build.name;
    if engine.legal_line_site == 0 {
        let why = if loader.address(pinned::ui::LEGAL_LINE_SITE).is_none() {
            "its table has no ui.legal_line_site"
        } else {
            "the code at ui.legal_line_site does not match its pattern or bytes"
        };
        crate::log!(
            WARN,
            "interface: the main menu's Enshwick line off on {build}: {why}"
        );
        return;
    }
    if !engine.has_legal_look() {
        crate::log!(
            WARN,
            "interface: the main menu's Enshwick line off on {build}: reflection lacks the version line's look (legalText, fontSizes.xsmall)"
        );
        return;
    }
    let link = crate::mid::MidLink {
        owner: ModIndex::LOADER,
        handler: version_line,
        user: 0,
        priority: i32::MIN,
        flags: enshwick::abi::HookFlags::NONE,
    };
    let status = loader.mids.relock().add(engine.legal_line_site, link);
    if !status.is_ok() {
        crate::log!(
            WARN,
            "interface: the main menu's Enshwick line off: its hook was refused ({})",
            status.0
        );
    }
}

/// The line for this frame in the frame's text arena, built again only when
/// the count of mods off changed.
fn line(loader: &Loader) -> super::engine::UiText {
    let off = loader
        .mods
        .iter()
        .filter(|m| m.off.load(Ordering::Acquire))
        .count();
    LINE.with_borrow_mut(|kept| {
        if kept.as_ref().is_none_or(|(was, _)| *was != off) {
            let lacking = !loader.build.lacking(crate::TABLE_IDS).is_empty();
            let line = crate::diag::menu_line(
                env!("CARGO_PKG_VERSION"),
                loader.mods.len(),
                off,
                lacking,
                !loader.settings_problems.is_empty(),
            );
            *kept = Some((off, line));
        }
        let line = kept.as_ref().map_or("", |(_, line)| line.as_str());
        copy(&[line.as_bytes()])
    })
}

/// The hook between the main menu's version line and its legal line: draws
/// the loader's line under the version.
///
/// # Safety
///
/// - Called only by the mid-hook stub, on the hooked thread, with null or
///   the registers it saved, valid and unaliased for the call.
unsafe extern "system" fn version_line(_registers: *mut Registers, _user: *mut core::ffi::c_void) {
    let _drawn = FUSE.run("the main menu's frame", || {
        let (Some(engine), Some(loader)) = (ENGINE.get(), crate::host::loader()) else {
            return;
        };
        let context = engine.context();
        if context == 0 {
            return;
        }
        let text = line(loader);
        // SAFETY: the hook at the table's `ui.legal_line_site` runs between
        // the main menu's version line and its legal line, on the frame's
        // thread, with the column of the `LegalLine` window open (client
        // 426); `context` is this frame's non-zero context, and the text
        // lives in the frame's arena.
        let _drawn = unsafe { engine.legal_line(context, &text) };
    });
}
