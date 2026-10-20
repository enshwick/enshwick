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
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{
    BuildCheck, BuildCheckFn, CraftCheck, CraftCheckFn, Header, HookFlags, Status,
};

use crate::chains::Next;
use crate::host::{Loader, Registered, take};

static CRAFT_FILTERS: Mutex<Vec<Registered<CraftCheckFn>>> = Mutex::new(Vec::new());
static BUILD_FILTERS: Mutex<Vec<Registered<BuildCheckFn>>> = Mutex::new(Vec::new());
static CRAFT_SEALED: OnceLock<Box<[Registered<CraftCheckFn>]>> = OnceLock::new();
static BUILD_SEALED: OnceLock<Box<[Registered<BuildCheckFn>]>> = OnceLock::new();
static CRAFT: OnceLock<CraftLayout> = OnceLock::new();
static BUILD: OnceLock<BuildLayout> = OnceLock::new();
// SAFETY: the slot joins the chain of the cursor check, typed `CursorFn`.
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
    owner: usize,
    callback: CraftCheckFn,
    user: *mut c_void,
) -> Status {
    if CRAFT_SEALED.get().is_some() {
        return Status::TOO_LATE;
    }
    if craft_layout(loader).is_none() {
        return Status::MISSING;
    }
    CRAFT_FILTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
    Status::OK
}

/// Registers mod `owner`'s build filter.
pub(crate) fn register_build(
    loader: &Loader,
    owner: usize,
    callback: BuildCheckFn,
    user: *mut c_void,
) -> Status {
    if BUILD_SEALED.get().is_some() {
        return Status::TOO_LATE;
    }
    if build_layout(loader).is_none()
        || loader.address("building.cursor_check").is_none()
        || crate::ecs::engine(loader).is_none()
    {
        return Status::MISSING;
    }
    BUILD_FILTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
    Status::OK
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
        hash: crate::world::type_hash(loader, info.va())?,
        size: usize::try_from(info.size()).ok()?,
        player: field("craftingOperatorId")?,
        station: field("craftingStationId")?,
        recipe: field("recipeId")?,
        amount: field("recipeAmount")?,
    })
}

fn build_layout(loader: &Loader) -> Option<BuildLayout> {
    Some(BuildLayout {
        item_info: loader.offset("building.cursor_item_info")?,
        no_permission: u32::try_from(loader.build.constant("building.cursor_no_permission")?)
            .ok()?,
    })
}

/// Whether a mod asked to screen craft requests, for the event link.
pub(crate) fn crafting() -> bool {
    CRAFT_SEALED
        .get()
        .is_some_and(|filters| !filters.is_empty())
}

/// Seals the filters and links what they need.
pub(crate) fn link(loader: &'static Loader) {
    let crafts = take(&CRAFT_FILTERS);
    if !crafts.is_empty()
        && let Some(layout) = craft_layout(loader)
    {
        let _first = CRAFT.set(layout);
        crate::log!(INFO, "craft filters on: {}", crafts.len());
    }
    let _first = CRAFT_SEALED.set(crafts.into_boxed_slice());
    let builds = take(&BUILD_FILTERS);
    let wanted = !builds.is_empty();
    let _first = BUILD_SEALED.set(builds.into_boxed_slice());
    if !wanted {
        return;
    }
    let Some(layout) = build_layout(loader) else {
        return;
    };
    let _first = BUILD.set(layout);
    loader.link_feature(
        "build filters",
        loader.address("building.cursor_check"),
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
/// `payload` is a live event of `type_hash`, `size` bytes, on the game
/// thread before the engine moves the streams.
pub(crate) unsafe fn screen(type_hash: u32, payload: *mut u8, size: usize) {
    let (Some(layout), Some(filters)) = (CRAFT.get(), CRAFT_SEALED.get()) else {
        return;
    };
    if type_hash != layout.hash || size < layout.size {
        return;
    }
    let read = |at: usize| -> u32 {
        // SAFETY: the field lies inside the event, as reflection says.
        unsafe { payload.wrapping_add(at).cast::<u32>().read_unaligned() }
    };
    // SAFETY: as above; the amount is a u16.
    let amount = unsafe {
        payload
            .wrapping_add(layout.amount)
            .cast::<u16>()
            .read_unaligned()
    };
    let call = CraftCheck {
        header: Header::of::<CraftCheck>(),
        player: read(layout.player),
        station: read(layout.station),
        recipe: read(layout.recipe),
        amount: u32::from(amount),
    };
    if call.player == 0 {
        return;
    }
    let allowed = filters.iter().filter(|f| f.live()).all(|f| {
        // SAFETY: the callback lives in the mod's DLL, which stays loaded,
        // and takes the pointer it registered; the call is valid for it.
        unsafe { (f.callback)(&raw const call, f.user()) != 0 }
    });
    if !allowed {
        // SAFETY: as for the reads; the streams are the loader's to change
        // until the engine moves them.
        unsafe {
            payload
                .wrapping_add(layout.player)
                .cast::<u32>()
                .write_unaligned(0);
        };
        // SAFETY: as above.
        unsafe {
            payload
                .wrapping_add(layout.amount)
                .cast::<u16>()
                .write_unaligned(0);
        };
        crate::log!(DEBUG, "craft of recipe {:#x} refused", call.recipe);
    }
}

unsafe extern "system" fn cursor(
    flags: *mut u32,
    cursor_context: *mut u8,
    placement: *mut u8,
    transform: *const i64,
    a: u8,
    b: u8,
    range: f32,
) {
    // SAFETY: forwarded unchanged.
    unsafe { CURSOR_NEXT.get()(flags, cursor_context, placement, transform, a, b, range) };
    let _caught = std::panic::catch_unwind(|| {
        // SAFETY: the game's arguments, after its own check.
        unsafe { refuse(flags, cursor_context, placement, transform) };
    });
}

/// Adds the "no permission" flag to a placement a filter refuses.
///
/// # Safety
///
/// The cursor check's arguments, on the thread that ran it.
unsafe fn refuse(flags: *mut u32, cursor: *mut u8, placement: *mut u8, transform: *const i64) {
    let (Some(layout), Some(filters), Some(engine)) =
        (BUILD.get(), BUILD_SEALED.get(), crate::ecs::resolved())
    else {
        return;
    };
    if flags.is_null() || cursor.is_null() || placement.is_null() || transform.is_null() {
        return;
    }
    // SAFETY: the game's flags word.
    let current = unsafe { flags.read_unaligned() };
    if current & layout.no_permission != 0 {
        return;
    }
    // SAFETY: the cursor context starts with its system context.
    let system: *mut u64 = unsafe { cursor.cast::<*mut u64>().read_unaligned() };
    // SAFETY: a live system context or null.
    let player = unsafe { engine.entity(system) };
    // SAFETY: the placement holds the placed item's static data, or null.
    let info: *const u8 = unsafe {
        placement
            .wrapping_add(layout.item_info)
            .cast::<*const u8>()
            .read_unaligned()
    };
    let item = if info.is_null() {
        0
    } else {
        // SAFETY: an item's static data starts with its id.
        unsafe { info.cast::<u32>().read_unaligned() }
    };
    let call = BuildCheck {
        header: Header::of::<BuildCheck>(),
        player,
        item,
        // SAFETY: the cursor transform starts with its 32.32 position.
        position: unsafe { transform.cast::<[i64; 3]>().read_unaligned() },
    };
    let allowed = filters.iter().filter(|f| f.live()).all(|f| {
        // SAFETY: the callback lives in the mod's DLL, which stays loaded,
        // and takes the pointer it registered; the call is valid for it.
        unsafe { (f.callback)(&raw const call, f.user()) != 0 }
    });
    if !allowed {
        // SAFETY: the game's flags word, which its building systems read.
        unsafe { flags.write_unaligned(current | layout.no_permission) };
    }
}
