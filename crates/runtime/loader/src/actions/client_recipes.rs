//! The crafting menu's highlighted recipe for the client snapshot, and the
//! recipe a client command asks the menu to highlight, both at the recipe
//! directory column, which draws the browser in the interface frame.
//!
//! The browser keeps the highlighted recipe's record and a recipe to bring
//! into focus; the tile row focuses that tile, which highlights it in the
//! same frame and crafts nothing (client 26430625, seen live). Until a mod
//! asks, the detour adds three atomic loads.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use enshwick_core::RecipeId;
use enshwick_core::abi::HookFlags;

use crate::actions::client_commands::{Handoff, Progress};
use crate::actions::client_control::{GROUPS, Recipes};
use crate::actions::client_snapshot::{self, RecipeRead};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;

/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "the crafting menu's recipe reads and highlights";
/// How many draws a recipe to focus may stay unfocused before the open
/// menu counts as not listing it: a tile out of view takes the list's
/// scroll first.
const UNLISTED_DRAWS: u32 = 30;
/// The read bit of [`Browser::highlighted`].
const READ: u64 = 1 << 32;

/// The stages of a handed recipe.
mod stage {
    /// The next draw writes it into the browser.
    pub(super) const WANTED: u8 = 1;
    /// Written; the tile row has not focused it yet.
    pub(super) const WRITTEN: u8 = 2;
    /// The tile row focused it.
    pub(super) const TAKEN: u8 = 3;
    /// No tile took it: the open menu does not list it.
    pub(super) const UNLISTED: u8 = 4;
}

/// What the detour keeps, written in the interface frame.
struct Browser {
    handoff: Handoff,
    /// The handed recipe's id.
    recipe: AtomicU32,
    /// The draws since it was written.
    draws: AtomicU32,
    /// A recipe the detour left in the browser's slot, 0 for none, cleared
    /// once a released command's tile no longer needs it.
    written: AtomicU32,
    /// The last draw's highlighted recipe, its id with [`READ`], 0 before
    /// one.
    highlighted: AtomicU64,
    /// Whether the browser drew since the last frame took its read.
    drawn: AtomicBool,
    linked: AtomicBool,
}

static BROWSER: Browser = Browser {
    handoff: Handoff::new(),
    recipe: AtomicU32::new(0),
    draws: AtomicU32::new(0),
    written: AtomicU32::new(0),
    highlighted: AtomicU64::new(0),
    drawn: AtomicBool::new(false),
    linked: AtomicBool::new(false),
};
static FUSE: Fuse = Fuse::new(FEATURE);

/// Links the directory column's detour on a client whose build has the
/// recipe browser.
pub(crate) fn link(loader: &Loader) {
    if GROUPS.recipes.get().is_none() {
        return;
    }
    let target = Target::Pinned(pinned::ui::RECIPE_DIRECTORY);
    let linked = loader.link_fused(&FUSE, target, &NEXT, recipe_directory, HookFlags::NONE);
    BROWSER.linked.store(linked, Ordering::Release);
}

/// Whether the build reads and highlights recipes: the browser and the
/// column's detour are linked and on.
pub(crate) fn runs() -> bool {
    GROUPS.recipes.get().is_some() && BROWSER.linked.load(Ordering::Acquire) && !FUSE.is_off()
}

/// The highlighted recipe of the browser's draw since the last call;
/// [`RecipeRead::Unread`] when it did not draw, as when no crafting menu
/// shows. On the interface thread, once a frame.
pub(crate) fn take_drawn() -> RecipeRead {
    if !runs() || !BROWSER.drawn.swap(false, Ordering::AcqRel) {
        return RecipeRead::Unread;
    }
    let word = BROWSER.highlighted.load(Ordering::Relaxed);
    let id = u32::try_from(word & 0xffff_ffff).unwrap_or(0);
    RecipeRead::Shown((id != 0).then_some(RecipeId(id)))
}

/// Hands command `id`'s recipe to the browser's next draw; whether it took
/// it, false while another runs.
pub(crate) fn hand(id: u64, recipe: RecipeId) -> bool {
    BROWSER.handoff.arm(id, stage::WANTED, || {
        BROWSER.recipe.store(recipe.0, Ordering::Relaxed);
        BROWSER.draws.store(0, Ordering::Relaxed);
    })
}

/// Where the browser stands with command `id`; `None` once it is no longer
/// handed.
pub(crate) fn progress(id: u64) -> Option<Progress> {
    BROWSER.handoff.stage(id).map(|handed| match handed {
        stage::TAKEN => Progress::Taken,
        stage::UNLISTED => Progress::Unavailable,
        _ => Progress::Waiting,
    })
}

/// Takes command `id` back; a recipe it left in the browser's slot is
/// cleared at the next draw.
pub(crate) fn release(id: u64) {
    let _had = BROWSER.handoff.release(id);
}

/// What a draw does with the browser's slot after the game's draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum After {
    /// A tile took the slot: the recipe is focused.
    Taken,
    /// Still unfocused after this many draws.
    Waits(u32),
    /// No tile took it in [`UNLISTED_DRAWS`] draws: clear the slot.
    Unlisted,
}

/// What the draw found for a written recipe `wanted`, the slot holding
/// `slot` after the game's draw, `draws` draws after it was written.
fn after(wanted: RecipeId, slot: Option<RecipeId>, draws: u32) -> After {
    if slot != Some(wanted) {
        After::Taken
    } else if draws + 1 >= UNLISTED_DRAWS {
        After::Unlisted
    } else {
        After::Waits(draws + 1)
    }
}

/// Writes a handed recipe into the browser before the game's draw, or
/// clears one a released command left there.
///
/// # Safety
///
/// - `browser` is the live recipe browser block the column draws, on the
///   interface thread, before the game's draw.
unsafe fn before(layout: &Recipes, browser: *mut u8, handed: Option<(u64, u8)>) {
    if let Some((id, stage::WANTED)) = handed {
        let recipe = RecipeId(BROWSER.recipe.load(Ordering::Relaxed));
        if BROWSER.handoff.advance(id, stage::WANTED, stage::WRITTEN) {
            // SAFETY: the caller's live block, on its thread.
            unsafe { layout.select(browser, Some(recipe)) };
            BROWSER.written.store(recipe.0, Ordering::Relaxed);
        }
        return;
    }
    let written = BROWSER.written.load(Ordering::Relaxed);
    if written != 0 && handed.is_none() {
        // SAFETY: the caller's live block, on its thread.
        if unsafe { layout.recipe_to_select(browser) } == Some(RecipeId(written)) {
            // SAFETY: the same block.
            unsafe { layout.select(browser, None) };
        }
        BROWSER.written.store(0, Ordering::Relaxed);
    }
}

/// Reads the highlighted recipe after the game's draw, and how the slot of
/// a written recipe fared.
///
/// # Safety
///
/// - `browser` is the live recipe browser block the column just drew, on
///   the interface thread.
unsafe fn after_draw(layout: &Recipes, browser: *mut u8, handed: Option<(u64, u8)>) {
    if client_snapshot::asked() || handed.is_some() {
        // SAFETY: the caller's live block, whose highlighted record pointer
        // the tile rows keep null or on a recipe record.
        let highlighted = unsafe { layout.highlighted_recipe(browser) };
        BROWSER.highlighted.store(
            READ | u64::from(highlighted.map_or(0, |r| r.0)),
            Ordering::Relaxed,
        );
        BROWSER.drawn.store(true, Ordering::Release);
    }
    let Some((id, stage::WRITTEN)) = handed else {
        return;
    };
    let wanted = RecipeId(BROWSER.recipe.load(Ordering::Relaxed));
    // SAFETY: the caller's live block.
    let slot = unsafe { layout.recipe_to_select(browser) };
    match after(wanted, slot, BROWSER.draws.load(Ordering::Relaxed)) {
        After::Taken => {
            BROWSER.written.store(0, Ordering::Relaxed);
            let _taken = BROWSER.handoff.advance(id, stage::WRITTEN, stage::TAKEN);
        }
        After::Waits(draws) => BROWSER.draws.store(draws, Ordering::Relaxed),
        After::Unlisted => {
            // SAFETY: the caller's live block.
            unsafe { layout.select(browser, None) };
            BROWSER.written.store(0, Ordering::Relaxed);
            let _unlisted = BROWSER.handoff.advance(id, stage::WRITTEN, stage::UNLISTED);
        }
    }
}

crate::engine::chains::chain_detour! {
    /// `doRecipeDirectoryColumn(node, book, browser, ctx)`, which draws the
    /// crafting menu's recipe directory from its browser block in the
    /// interface frame (client 26430625).
    fn recipe_directory(node: usize, book: usize, browser: *mut u8, ctx: usize) -> u64;
    next NEXT;
    fuse &FUSE, "the crafting menu's recipe browser";
    |onward| {
        let handed = BROWSER.handoff.current();
        let idle = handed.is_none()
            && BROWSER.written.load(Ordering::Relaxed) == 0
            && !client_snapshot::asked();
        let Some(layout) = GROUPS.recipes.get().filter(|_| !idle && !browser.is_null()) else {
            return onward.call();
        };
        // SAFETY: the column's own non-null browser block, at its entry on
        // the interface thread.
        unsafe { before(layout, browser, handed) };
        let drawn = onward.call();
        // SAFETY: the same block, right after the game's draw.
        unsafe { after_draw(layout, browser, BROWSER.handoff.current()) };
        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_recipe_is_taken_once_the_slot_moves_and_unlisted_when_it_stays() {
        let torch = RecipeId(0x7cd8_0a04);
        assert_eq!(after(torch, None, 0), After::Taken, "zeroed by its tile");
        assert_eq!(
            after(torch, Some(RecipeId(9)), 0),
            After::Taken,
            "the menu's rebuild"
        );
        assert_eq!(after(torch, Some(torch), 0), After::Waits(1));
        assert_eq!(
            after(torch, Some(torch), UNLISTED_DRAWS - 1),
            After::Unlisted,
            "no tile took it"
        );
    }

    #[test]
    fn a_handed_recipe_reports_its_stage_until_it_is_released() {
        assert!(hand(5, RecipeId(3)));
        assert_eq!(progress(5), Some(Progress::Waiting));
        assert!(!hand(6, RecipeId(4)), "one at a time");
        assert!(BROWSER.handoff.advance(5, stage::WANTED, stage::WRITTEN));
        assert!(BROWSER.handoff.advance(5, stage::WRITTEN, stage::UNLISTED));
        assert_eq!(progress(5), Some(Progress::Unavailable));
        release(5);
        assert_eq!(progress(5), None);
    }

    #[test]
    fn no_draw_reads_nothing() {
        assert_eq!(take_drawn(), RecipeRead::Unread);
    }
}
