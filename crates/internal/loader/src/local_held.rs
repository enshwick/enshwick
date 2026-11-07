//! The game controls the local player holds, read on the client at the
//! entry of the interface's own player system, `player_ui`, which runs on
//! the local player: its `ClientPlayerInput`'s digital mask, kept for any
//! thread to read.
//!
//! `player_ui` takes its row's entity first thing (client 26430625), so the
//! context reads the same at its entry; the mask is the last step's, since
//! the system writes the new one as it runs.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use enshwick::abi::{HookFlags, Side, Status};
use enshwick_gamedata::catalogue;

use crate::chains::Next;
use crate::ecs::Components;
use crate::fuse::Fuse;
use crate::host::{Loader, Target};
use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::COMPONENT_READ_RAW.id(),
    pinned::ecs::ROW_ENTITY.id(),
];

/// The system whose entry reads the mask.
const SYSTEM: &str = "player_ui";
/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "the local player's held controls";
/// How long a read stays the answer: past it, no step ran, as in a menu.
const FRESH_MS: u64 = 1000;

type SystemFn = unsafe extern "system" fn(*mut u64) -> u64;
type RowEntity = unsafe extern "system" fn(ctx: *mut u64, out: *mut u32) -> *const u32;

/// Where the mask is read on this build.
#[derive(Debug)]
struct Layout {
    components: Components,
    row_entity: RowEntity,
    input: u32,
    digital: usize,
}

/// The reads' state: the layout once linked, whether a mod asked yet, the
/// last mask and when it was read, in milliseconds from `start` plus one,
/// so 0 is never read.
struct State {
    layout: OnceLock<Layout>,
    asked: AtomicBool,
    start: OnceLock<Instant>,
    bits: AtomicU64,
    read_ms: AtomicU64,
}

static STATE: State = State {
    layout: OnceLock::new(),
    asked: AtomicBool::new(false),
    start: OnceLock::new(),
    bits: AtomicU64::new(0),
    read_ms: AtomicU64::new(0),
};
static FUSE: Fuse = Fuse::new(FEATURE);
// SAFETY: `SystemFn` is the signature of a system entry, here the reflected
// `player_ui` system's, and `link` uses this slot only as that function's
// next.
static NEXT: Next<SystemFn> = unsafe { Next::new() };

fn now_ms() -> u64 {
    let start = STATE.start.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX - 1) + 1
}

fn layout(loader: &Loader) -> Result<Layout, &'static str> {
    let row = loader
        .address(pinned::ecs::ROW_ENTITY)
        .and_then(|at| usize::try_from(at).ok())
        .ok_or(pinned::ecs::ROW_ENTITY.id())?;
    let digital = catalogue::constant(
        catalogue::ids::CLIENT_PLAYER_INPUT_DIGITAL,
        &loader.reflection,
    )
    .and_then(|at| usize::try_from(at).ok())
    .ok_or(catalogue::ids::CLIENT_PLAYER_INPUT_DIGITAL)?;
    let size = loader
        .reflection
        .type_info(catalogue::type_name(catalogue::ids::CLIENT_PLAYER_INPUT))
        .map_err(|_| catalogue::ids::CLIENT_PLAYER_INPUT)?
        .size();
    if digital + size_of::<u64>() > usize::try_from(size).unwrap_or(0) {
        return Err(catalogue::ids::CLIENT_PLAYER_INPUT_DIGITAL);
    }
    Ok(Layout {
        components: Components::resolve(loader).ok_or(pinned::ecs::COMPONENT_READ_RAW.id())?,
        // SAFETY: the build table pins `ecs.row_entity` to this signature.
        row_entity: unsafe { core::mem::transmute::<usize, RowEntity>(row) },
        input: catalogue::type_hash(catalogue::ids::CLIENT_PLAYER_INPUT)
            .ok_or(catalogue::ids::CLIENT_PLAYER_INPUT)?,
        digital,
    })
}

pub(crate) fn link(loader: &Loader) {
    if loader.side != Side::CLIENT {
        return;
    }
    let layout = match layout(loader) {
        Ok(layout) => layout,
        Err(id) => {
            let _off = loader.feature_off(FEATURE, id, "this build lacks it");
            return;
        }
    };
    let _first = STATE.layout.set(layout);
    let entry = loader
        .reflection
        .system(SYSTEM)
        .ok()
        .map(enshwick_gamedata::reflect::System::entry);
    let _linked = loader.link_feature(
        FEATURE,
        Target::Found(SYSTEM, entry),
        &NEXT,
        player_ui as SystemFn,
        HookFlags::NONE,
    );
}

/// Reads the local player's mask from `player_ui`'s context.
///
/// # Safety
///
/// - `ctx` is the `player_ui` system's live, non-null context, at its entry
///   on its thread.
unsafe fn read(ctx: *mut u64, layout: &Layout) {
    let mut entity = 0u32;
    // SAFETY: the game's `ecs.row_entity` on the system's live context, at
    // its entry on its thread, as `player_ui` calls it itself first thing
    // (client 26430625), with our own out word.
    let found = unsafe { (layout.row_entity)(ctx, &raw mut entity) };
    if found.is_null() {
        return;
    }
    // SAFETY: `ecs.row_entity` returned a non-null pointer to the row's u32
    // entity id, live for the call.
    let player: u32 = unsafe { enshwick::raw::read_at(found.cast(), 0) };
    // SAFETY: the caller's live context at the system's entry on its thread.
    let Some(input) = (unsafe { layout.components.component(ctx, player, layout.input) }) else {
        return;
    };
    // SAFETY: `input` is the player's live `ClientPlayerInput`, unwritten
    // until the system runs; `layout` checked that the u64 at `digital` lies
    // inside the type.
    let bits: u64 = unsafe { enshwick::raw::read_at(input, layout.digital) };
    STATE.bits.store(bits, Ordering::Relaxed);
    STATE.read_ms.store(now_ms(), Ordering::Release);
}

unsafe extern "system" fn player_ui(ctx: *mut u64) -> u64 {
    // Until a mod asks, the system runs as the game's alone.
    if STATE.asked.load(Ordering::Relaxed)
        && let Some(layout) = STATE.layout.get()
        && !ctx.is_null()
    {
        // SAFETY: `ctx` is non-null by the check before it and is the
        // system's own context, at its entry on its thread.
        let _read = FUSE.run("the local player's held controls", || unsafe {
            read(ctx, layout);
        });
    }
    // SAFETY: this detour runs only as the `player_ui` system's link, so the
    // slot holds the next function of that signature; the game's context
    // passes unchanged.
    unsafe { NEXT.get()(ctx) }
}

/// The local player's held mask, or why not: [`Status::MISSING`] while no
/// step read it for [`FRESH_MS`], on a server or on a build without it.
/// The first ask starts the reads, so it answers [`Status::MISSING`].
pub(crate) fn held() -> Result<u64, Status> {
    if !STATE.asked.load(Ordering::Relaxed) {
        STATE.asked.store(true, Ordering::Relaxed);
    }
    let read = STATE.read_ms.load(Ordering::Acquire);
    if read == 0 || FUSE.is_off() || now_ms().saturating_sub(read) > FRESH_MS {
        return Err(Status::MISSING);
    }
    Ok(STATE.bits.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_ask_starts_the_reads_and_answers_missing() {
        assert!(
            !STATE.asked.load(Ordering::Relaxed),
            "the detour reads nothing"
        );
        assert_eq!(held(), Err(Status::MISSING));
        assert!(STATE.asked.load(Ordering::Relaxed));
    }
}
