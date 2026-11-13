//! The main menu's line under the game's version: the loader's version, how
//! many mods load and what is off.

use core::cell::RefCell;

use enshwick_core::abi::HookFlags;

use crate::engine::diag::KeptLine;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;

use super::engine::{Engine, Missing};
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
    let site = pinned::ui::LEGAL_LINE_SITE;
    let missing = missing(
        engine.legal_line_site != 0,
        loader.address(site).is_some(),
        engine.has_legal_look(),
    );
    if let Some(why) = missing {
        loader.fuse_off(&FUSE, why.id(), &why.reason());
        return;
    }
    loader.link_mid(
        &FUSE,
        Target::Found(site.id(), Some(engine.legal_line_site)),
        version_line,
        HookFlags::NONE,
    );
}

/// What the line lacks on this build: its site, when the engine found none
/// (`site`) in the table or at all (`in_table`), else the version line's
/// look (`look`).
fn missing(site: bool, in_table: bool, look: bool) -> Option<Missing> {
    let id = pinned::ui::LEGAL_LINE_SITE.id();
    if !site {
        return Some(if in_table {
            Missing::Bytes(id)
        } else {
            Missing::Entry(id)
        });
    }
    (!look).then_some(Missing::Looks)
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
                    !loader.build.lacking(loader.parts.table_ids()).is_empty(),
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

crate::engine::mid::mid_handler! {
    /// The hook between the main menu's version line and its legal line:
    /// draws the loader's line under the version.
    fn version_line;
    fuse FUSE, "the main menu's frame";
    |_registers| {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_names_what_this_build_lacks_its_site_before_its_look() {
        let id = pinned::ui::LEGAL_LINE_SITE.id();
        assert_eq!(missing(true, true, true), None);
        assert_eq!(missing(false, false, false), Some(Missing::Entry(id)));
        assert_eq!(missing(false, true, true), Some(Missing::Bytes(id)));
        assert_eq!(missing(true, true, false), Some(Missing::Looks));
    }
}
