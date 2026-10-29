//! Vetoes on crafting and building, asked of the mods' filters before the
//! game acts.
//!
//! A craft request is a `CraftRecipeEvent` the server's `player_crafting`
//! system raises from the player's input; the crafting system reads it in
//! the next step. The loader screens the requests in the event streams on
//! the game thread, before the engine moves them, and empties a refused one:
//! no crafter and no amount, so the crafting system has nothing to make and
//! the ingredients stay where they are.
//!
//! A building placement passes the game's cursor check, which the server
//! runs on the cursor each player's client sends and the client runs for its
//! own prediction, on worker threads. The loader runs the filters after the
//! check and adds the game's own "no permission" flag to a refused
//! placement, which the building systems refuse to place and the client's
//! cursor shows as not allowed.

use core::ffi::c_void;
use std::sync::OnceLock;

use enshwick::abi::{
    BuildCheck, BuildCheckFn, CraftCheck, CraftCheckFn, Header, HookFlags, Status,
};
use enshwick::raw::{read_at, read_ptr_at, write_at};

use crate::chains::Next;
use crate::host::{Loader, Registered, Target};
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::building::CURSOR_CHECK.id(),
    pinned::building::CURSOR_ITEM_INFO.id(),
    pinned::building::CURSOR_NO_PERMISSION.id(),
];

static CRAFT_FILTERS: Registry<Registered<CraftCheckFn>> = Registry::new();
static BUILD_FILTERS: Registry<Registered<BuildCheckFn>> = Registry::new();
static CRAFT: OnceLock<CraftLayout> = OnceLock::new();
static BUILD: OnceLock<BuildLayout> = OnceLock::new();
// SAFETY: `CursorFn` is the signature of the table's
// `building.cursor_check`, and this slot is the `next` of the one link `link`
// makes on it, whose detour `cursor` calls `get`.
static CURSOR_NEXT: Next<CursorFn> = unsafe { Next::new() };

type CursorFn = unsafe extern "system" fn(
    flags: *mut u32,
    cursor: *mut u8,
    placement: *mut u8,
    transform: *const i64,
    a: u8,
    b: u8,
    range: f32,
);

/// `CraftRecipeEvent`'s hash and fields, from reflection.
#[derive(Debug, Clone, Copy)]
struct CraftLayout {
    hash: u32,
    size: usize,
    player: usize,
    station: usize,
    recipe: usize,
    amount: usize,
}

/// The cursor check's placement and flags, from the build table.
#[derive(Debug, Clone, Copy)]
struct BuildLayout {
    item_info: usize,
    no_permission: u32,
}

/// Registers mod `owner`'s craft filter.
pub(crate) fn register_craft(
    loader: &Loader,
    owner: ModIndex,
    callback: CraftCheckFn,
    user: *mut c_void,
) -> Status {
    if CRAFT_FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    if craft_layout(loader).is_none() {
        return Status::MISSING;
    }
    CRAFT_FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Registers mod `owner`'s build filter.
pub(crate) fn register_build(
    loader: &Loader,
    owner: ModIndex,
    callback: BuildCheckFn,
    user: *mut c_void,
) -> Status {
    if BUILD_FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    if build_layout(loader).is_none()
        || loader.address(pinned::building::CURSOR_CHECK).is_none()
        || crate::ecs::engine(loader).is_none()
    {
        return Status::MISSING;
    }
    BUILD_FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

fn craft_layout(loader: &Loader) -> Option<CraftLayout> {
    let name = "keen::ecs::CraftRecipeEvent";
    let reflection = &loader.reflection;
    let info = reflection.type_info(name).ok()?;
    let field = |field: &str| {
        reflection
            .field_offset(name, field)
            .ok()
            .and_then(|v| usize::try_from(v).ok())
    };
    Some(CraftLayout {
        hash: crate::world::type_hash(info.va())?,
        size: usize::try_from(info.size()).ok()?,
        player: field("craftingOperatorId")?,
        station: field("craftingStationId")?,
        recipe: field("recipeId")?,
        amount: field("recipeAmount")?,
    })
}

fn build_layout(loader: &Loader) -> Option<BuildLayout> {
    Some(BuildLayout {
        item_info: loader.offset(pinned::building::CURSOR_ITEM_INFO)?,
        no_permission: u32::try_from(
            loader
                .build
                .constant(pinned::building::CURSOR_NO_PERMISSION.id())?,
        )
        .ok()?,
    })
}

/// Whether craft requests are screened, for mods' filters and disabled
/// recipes alike, for the event link.
pub(crate) fn crafting() -> bool {
    CRAFT.get().is_some()
}

/// Seals the filters and links what they need. Craft requests are screened
/// whenever the build reads them, as any recipe may be disabled later.
pub(crate) fn link(loader: &'static Loader) {
    let crafts = CRAFT_FILTERS.seal();
    if let Some(layout) = craft_layout(loader) {
        let _first = CRAFT.set(layout);
        if !crafts.is_empty() {
            crate::log!(INFO, "craft filters on: {}", crafts.len());
        }
    } else {
        crate::log!(
            WARN,
            "craft requests are not screened: this build's CraftRecipeEvent is not reflected"
        );
    }
    let builds = BUILD_FILTERS.seal();
    let wanted = !builds.is_empty();
    if !wanted {
        return;
    }
    let Some(layout) = build_layout(loader) else {
        return;
    };
    let _first = BUILD.set(layout);
    loader.link_feature(
        "build filters",
        Target::Pinned(pinned::building::CURSOR_CHECK),
        &CURSOR_NEXT,
        cursor as CursorFn,
        HookFlags::NONE,
    );
}

/// Screens one event record of the server's streams on the game thread:
/// empties a craft request a filter refuses.
///
/// # Safety
///
/// - `payload` is a live event of `type_hash`, readable and writable for
///   `size` bytes.
/// - Called on the game thread before the engine moves the streams, so
///   nothing else reads or writes the event during the call.
pub(crate) unsafe fn screen(type_hash: u32, payload: *mut u8, size: usize) {
    let (Some(layout), Some(filters)) = (CRAFT.get(), CRAFT_FILTERS.sealed()) else {
        return;
    };
    if type_hash != layout.hash || size < layout.size {
        return;
    }
    // SAFETY: the event is a `CraftRecipeEvent` of at least its reflected size
    // (`size < layout.size` returns first), `size` readable bytes (the caller's
    // contract), and reflection places its `u32` `craftingOperatorId` at
    // `player`.
    let player: u32 = unsafe { read_at(payload, layout.player) };
    // SAFETY: the event is a `CraftRecipeEvent` of at least its reflected size
    // (`size < layout.size` returns first), `size` readable bytes (the caller's
    // contract), and reflection places its `u32` `craftingStationId` at
    // `station`.
    let station: u32 = unsafe { read_at(payload, layout.station) };
    // SAFETY: the event is a `CraftRecipeEvent` of at least its reflected size
    // (`size < layout.size` returns first), `size` readable bytes (the caller's
    // contract), and reflection places its `u32` `recipeId` at `recipe`.
    let recipe: u32 = unsafe { read_at(payload, layout.recipe) };
    // SAFETY: the event is a `CraftRecipeEvent` of at least its reflected size
    // (`size < layout.size` returns first), `size` readable bytes (the caller's
    // contract), and reflection places its `u16` `recipeAmount` at `amount`.
    let amount: u16 = unsafe { read_at(payload, layout.amount) };
    let call = CraftCheck {
        header: Header::of::<CraftCheck>(),
        crafting_operator_id: enshwick::Entity(player),
        crafting_station_id: enshwick::Entity(station),
        recipe_id: enshwick::RecipeId::new(recipe),
        recipe_amount: amount,
        reserved: 0,
    };
    if call.crafting_operator_id == enshwick::Entity(0) {
        return;
    }
    let disabled = crate::recipes::is_disabled(call.recipe_id.get());
    let allowed = !disabled
        && filters.iter().filter(|f| f.live()).all(|f| {
            // SAFETY: `callback` is a `CraftCheckFn` the mod registered, in
            // its DLL, which stays loaded (`Registered`); `user` is the
            // pointer it registered with it, and `call` lives on this stack
            // for the call.
            crate::profile::call(f.owner, crate::profile::Feature::Vetoes, || unsafe {
                (f.callback)(&raw const call, f.user()) != 0
            })
        });
    if !allowed {
        // SAFETY: the event is a `CraftRecipeEvent` of at least its reflected
        // size (`size < layout.size` returns first), `size` writable bytes
        // nothing else touches until the engine moves the streams (the caller's
        // contract), with its `u32` `craftingOperatorId` at `player`.
        unsafe { write_at(payload, layout.player, 0u32) };
        // SAFETY: the event is a `CraftRecipeEvent` of at least its reflected
        // size (`size < layout.size` returns first), `size` writable bytes
        // nothing else touches until the engine moves the streams (the caller's
        // contract), with its `u16` `recipeAmount` at `amount`.
        unsafe { write_at(payload, layout.amount, 0u16) };
        crate::log!(
            DEBUG,
            "craft of recipe {:#x} by player {} refused{}",
            call.recipe_id,
            call.crafting_operator_id.0,
            if disabled {
                ": the recipe is disabled"
            } else {
                ""
            }
        );
    }
}

/// The switch of the crafting and building vetoes.
static VETOES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("crafting and building vetoes");

unsafe extern "system" fn cursor(
    flags: *mut u32,
    cursor_context: *mut u8,
    placement: *mut u8,
    transform: *const i64,
    a: u8,
    b: u8,
    range: f32,
) {
    // SAFETY: the game's arguments to `building.cursor_check`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    unsafe { CURSOR_NEXT.get()(flags, cursor_context, placement, transform, a, b, range) };
    let _caught = VETOES_FUSE.run("the cursor", || {
        // SAFETY: the game's arguments to `building.cursor_check`, after its
        // own check returned, on the thread that ran it.
        unsafe { refuse(flags, cursor_context, placement, transform) };
    });
}

/// Adds the "no permission" flag to a placement a filter refuses.
///
/// # Safety
///
/// - `flags`, `cursor`, `placement` and `transform` are the arguments the
///   game passed to `building.cursor_check`, each null or live.
/// - Called on the thread that ran the check, right after it returned, so
///   nothing else writes them during the call.
unsafe fn refuse(flags: *mut u32, cursor: *mut u8, placement: *mut u8, transform: *const i64) {
    let (Some(layout), Some(filters), Some(engine)) =
        (BUILD.get(), BUILD_FILTERS.sealed(), crate::ecs::resolved())
    else {
        return;
    };
    if flags.is_null() || cursor.is_null() || placement.is_null() || transform.is_null() {
        return;
    }
    // SAFETY: `flags` is non-null (the null checks return first) and the
    // check's live `u32` flags word (the caller's contract).
    let current: u32 = unsafe { read_at(flags.cast(), 0) };
    if current & layout.no_permission != 0 {
        return;
    }
    // SAFETY: `cursor` is non-null (the null checks return first) and the
    // check's live cursor context (the caller's contract), which starts with
    // its system context pointer.
    let system = unsafe { read_ptr_at::<u64>(cursor, 0) };
    // SAFETY: `system` is the cursor's system context, null or live, on the
    // thread running that system (the caller's contract).
    let player = unsafe { engine.entity(system) };
    // SAFETY: `placement` is non-null (the null checks return first) and the
    // check's live placement (the caller's contract); the table's
    // `building.cursor_item_info` offset holds the placed item's static data
    // pointer, or null.
    let info = unsafe { read_ptr_at::<u8>(placement, layout.item_info) }.cast_const();
    let item = if info.is_null() {
        0
    } else {
        // SAFETY: `info` is non-null (the `is_null` branch gives 0) and an
        // item's static data, which starts with its `u32` id and stays loaded.
        unsafe { read_at::<u32>(info, 0) }
    };
    let call = BuildCheck {
        header: Header::of::<BuildCheck>(),
        player: enshwick::Entity(player),
        item: enshwick::ItemId::new(item),
        // SAFETY: `transform` is non-null (the null checks return first) and
        // the check's live cursor transform (the caller's contract), which
        // starts with its 32.32 position, three `i64`.
        position: unsafe { read_at(transform.cast(), 0) },
    };
    let allowed = filters.iter().filter(|f| f.live()).all(|f| {
        // SAFETY: `callback` is a `BuildCheckFn` the mod registered, in its
        // DLL, which stays loaded (`Registered`); `user` is the pointer it
        // registered with it, and `call` lives on this stack for the call.
        crate::profile::call(f.owner, crate::profile::Feature::Vetoes, || unsafe {
            (f.callback)(&raw const call, f.user()) != 0
        })
    });
    if !allowed {
        // SAFETY: `flags` is non-null (the null checks return first) and the
        // check's live `u32` flags word, which nothing else writes during the
        // call (the caller's contract) and the building systems read after it.
        unsafe { write_at(flags.cast(), 0, current | layout.no_permission) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: CraftLayout = CraftLayout {
        hash: 0x1234,
        size: 16,
        player: 0,
        station: 4,
        recipe: 8,
        amount: 12,
    };

    fn request(player: u32, recipe: u32, amount: u16) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&player.to_le_bytes());
        bytes[8..12].copy_from_slice(&recipe.to_le_bytes());
        bytes[12..14].copy_from_slice(&amount.to_le_bytes());
        bytes
    }

    #[test]
    fn a_request_for_a_disabled_recipe_reaches_the_game_empty() {
        let _first = CRAFT.set(LAYOUT);
        let _sealed = CRAFT_FILTERS.seal();
        crate::recipes::disable_for_test(&[7]);
        let mut refused = request(5, 7, 3);
        let mut allowed = request(5, 8, 3);
        for bytes in [&mut refused, &mut allowed] {
            // SAFETY: `bytes` is a request of the layout's 16 bytes on this
            // stack, readable and writable, which only this thread touches.
            unsafe { screen(LAYOUT.hash, bytes.as_mut_ptr(), bytes.len()) };
        }
        assert_eq!(refused, request(0, 7, 0));
        assert_eq!(allowed, request(5, 8, 3));
    }
}
