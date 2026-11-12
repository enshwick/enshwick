//! The main menu's line under the game's version: the loader's version, how
//! many mods load and what is off.

use core::cell::RefCell;

use enshwick_core::abi::Registers;

use crate::engine::diag::KeptLine;
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use enshwick_base::Relock;

use super::engine::Engine;
use super::{ENGINE, copy};

thread_local! {
    /// The line, made again only when the count of mods off changes.
    static LINE: RefCell<KeptLine> = const { RefCell::new(KeptLine::new()) };
}

/// Turns the line off, alone, at its first panic.
static FUSE: Fuse = Fuse::new("the main menu's Enshwick line");

/// Links the hook between the version line and the legal line, on a build
/// whose site holds and that has the version line's look.
pub(super) fn link(loader: &Loader, engine: &Engine) {
    let build = &loader.build.name;
    let off = crate::engine::diag::menu_line_off(
        engine.legal_line_site != 0,
        loader.address(pinned::ui::LEGAL_LINE_SITE).is_some(),
        engine.has_legal_look(),
    );
    if let Some(why) = off {
        crate::log!(
            WARN,
            "interface: the main menu's Enshwick line off on {build}: {why}"
        );
        return;
    }
    let link = crate::engine::mid::MidLink {
        owner: ModIndex::LOADER,
        handler: version_line,
        user: 0,
        priority: i32::MIN,
        flags: enshwick_core::abi::HookFlags::NONE,
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
    let off = crate::base::off_line::mods_off_count(loader);
    LINE.with_borrow_mut(|kept| {
        let line = kept.get(off, || {
            use crate::engine::diag::MenuNote;
            let notes = [
                (
                    !loader.build.lacking(crate::TABLE_IDS).is_empty(),
                    MenuNote::Lacking,
                ),
                (!loader.settings_problems.is_empty(), MenuNote::BadSettings),
                (loader.developer_tools, MenuNote::Developer),
            ];
            let notes: Vec<MenuNote> = notes
                .into_iter()
                .filter_map(|(shown, note)| shown.then_some(note))
                .collect();
            let total = crate::base::off_line::mods_counted(loader);
            crate::engine::diag::menu_line(env!("CARGO_PKG_VERSION"), total, off, &notes)
        });
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
        let (Some(engine), Some(loader)) = (ENGINE.get(), crate::engine::loader::loader()) else {
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
