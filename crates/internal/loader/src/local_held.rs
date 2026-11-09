//! The game controls the local player holds, read on the client at the
//! entry of the interface's own player system, `player_ui`, which runs on
//! the local player: its `ClientPlayerInput`'s digital mask, kept for any
//! thread to read.
//!
//! `player_ui` takes its row's entity first thing (client 26430625), so the
//! context reads the same at its entry; the mask is the last step's, since
//! the system writes the new one as it runs.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, fence};
use std::time::Instant;

use enshwick::Entity;
use enshwick::abi::{HookFlags, Side, Status};
use enshwick_gamedata::catalogue;

use crate::chains::Next;
use crate::ecs::Components;
use crate::fuse::Fuse;
use crate::host::{Loader, Target};
use crate::pinned;
use crate::world::TemplateWalk;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::COMPONENT_READ_RAW.id(),
    pinned::ecs::ROW_ENTITY.id(),
    pinned::ecs::ENTITY_SLOT.id(),
];

/// The system whose entry reads the mask.
const SYSTEM: &str = "player_ui";
/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "the local player's held controls";
/// The host reads' own name in the log and on the Mods screen.
const HOST_FEATURE: &str = "the open screen's host template and position";
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

/// Where the open screen host's template and position are read.
#[derive(Debug)]
struct HostLayout {
    components: Components,
    walk: TemplateWalk,
    /// `CurrentTransform`'s type hash and the offset of its position, three
    /// 32.32 fixed-point `i64`.
    transform: u32,
    position: usize,
}

/// The open screen host's last read: its entity, then its template's two
/// words and its fixed-point position, behind a sequence count that is odd
/// while the game thread writes, so the interface thread never takes half
/// of one read without a lock.
struct HostRead {
    seq: AtomicU32,
    entity: AtomicU32,
    words: [AtomicU64; 5],
}

impl HostRead {
    const fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            entity: AtomicU32::new(0),
            words: [const { AtomicU64::new(0) }; 5],
        }
    }

    /// Stores a read; only the system's thread writes.
    fn write(&self, entity: u32, template: [u8; 16], position: [i64; 3]) {
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        self.entity.store(entity, Ordering::Relaxed);
        let [lo, hi] = bytemuck::cast::<[u8; 16], [u64; 2]>(template);
        let [x, y, z] = position.map(i64::cast_unsigned);
        for (word, value) in self.words.iter().zip([lo, hi, x, y, z]) {
            word.store(value, Ordering::Relaxed);
        }
        self.seq.store(seq.wrapping_add(2), Ordering::Release);
    }

    /// The last whole read, or `None` while the writer stays mid-write.
    fn snapshot(&self) -> Option<(u32, [u8; 16], [i64; 3])> {
        for _ in 0..4 {
            let before = self.seq.load(Ordering::Acquire);
            if before % 2 == 1 {
                core::hint::spin_loop();
                continue;
            }
            let entity = self.entity.load(Ordering::Relaxed);
            let [lo, hi, x, y, z] = self.words.each_ref().map(|w| w.load(Ordering::Relaxed));
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == before {
                let template = bytemuck::cast::<[u64; 2], [u8; 16]>([lo, hi]);
                return Some((entity, template, [x, y, z].map(u64::cast_signed)));
            }
        }
        None
    }
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
    /// Where the host reads run, once linked on a build that has them.
    host_layout: OnceLock<HostLayout>,
    /// The open screen's host the interface asks about, 0 for none.
    host_wanted: AtomicU32,
    host: HostRead,
    /// When a step last wrote `host`, as `read_ms` counts.
    host_ms: AtomicU64,
}

static STATE: State = State {
    layout: OnceLock::new(),
    asked: AtomicBool::new(false),
    start: OnceLock::new(),
    bits: AtomicU64::new(0),
    read_ms: AtomicU64::new(0),
    host_layout: OnceLock::new(),
    host_wanted: AtomicU32::new(0),
    host: HostRead::new(),
    host_ms: AtomicU64::new(0),
};
static FUSE: Fuse = Fuse::new(FEATURE);
static HOST_FUSE: Fuse = Fuse::new(HOST_FEATURE);
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

/// Where the host reads run on this build, or the id of what it lacks.
fn host_layout(loader: &Loader) -> Result<HostLayout, &'static str> {
    Ok(HostLayout {
        components: Components::resolve(loader).ok_or(pinned::ecs::COMPONENT_READ_RAW.id())?,
        walk: TemplateWalk::resolve(loader).ok_or(pinned::ecs::ENTITY_SLOT.id())?,
        transform: catalogue::type_hash(catalogue::ids::CURRENT_TRANSFORM)
            .ok_or(catalogue::ids::CURRENT_TRANSFORM)?,
        position: catalogue::constant(
            catalogue::ids::CURRENT_TRANSFORM_TRANSFORM,
            &loader.reflection,
        )
        .and_then(|at| usize::try_from(at).ok())
        .ok_or(catalogue::ids::CURRENT_TRANSFORM_TRANSFORM)?,
    })
}

pub(crate) fn link(loader: &Loader) {
    if loader.side != Side::CLIENT {
        return;
    }
    let mut any = false;
    match layout(loader) {
        Ok(layout) => any |= STATE.layout.set(layout).is_ok(),
        Err(id) => {
            let _off = loader.feature_off(FEATURE, id, "this build lacks it");
        }
    }
    match host_layout(loader) {
        Ok(at) => any |= STATE.host_layout.set(at).is_ok(),
        Err(id) => {
            let _off = loader.feature_off(HOST_FEATURE, id, "this build lacks it");
        }
    }
    if !any {
        return;
    }
    let entry = loader
        .reflection
        .system(SYSTEM)
        .ok()
        .map(enshwick_gamedata::reflect::System::entry);
    let linked = loader.link_feature(
        FEATURE,
        Target::Found(SYSTEM, entry),
        &NEXT,
        player_ui as SystemFn,
        HookFlags::NONE,
    );
    if !linked && STATE.host_layout.get().is_some() {
        let _off = loader.feature_off(HOST_FEATURE, SYSTEM, "its system did not link");
    }
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

/// Reads the template and position of the open screen's host, `host`, in
/// the client's world, the one `player_ui` runs in (client 26430625).
///
/// # Safety
///
/// - `ctx` is the `player_ui` system's live, non-null context, at its entry
///   on its thread.
unsafe fn read_host(ctx: *mut u64, at: &HostLayout, host: u32) {
    // SAFETY: the caller's live context, on its thread.
    let world = unsafe { at.components.world(ctx) };
    // SAFETY: `world` is the running system's live entity world, read on
    // its thread during the system's entry, before it changes any entity.
    let template = world.and_then(|world| unsafe { at.walk.template(world, Entity(host)) });
    // SAFETY: the caller's live context, on its thread.
    let transform = unsafe { at.components.component(ctx, host, at.transform) };
    // SAFETY: `transform` is the host's live `CurrentTransform`, whose
    // position, three fixed-point `i64`, sits at the reflected offset
    // `at.position`.
    let position =
        transform.map(|data| unsafe { enshwick::raw::read_at::<[i64; 3]>(data, at.position) });
    // A template without a position would name the world's origin.
    let (template, position) = template.zip(position).unwrap_or_default();
    STATE.host.write(host, template, position);
    STATE.host_ms.store(now_ms(), Ordering::Release);
}

/// The template and fixed-point position of the open screen's host `host`
/// as the client's last step read them, and asks the next step to read it;
/// `None` for no host, before a step read this one, when no step read for
/// [`FRESH_MS`], and on a build without the reads.
pub(crate) fn screen_host(host: u32) -> Option<([u8; 16], [i64; 3])> {
    STATE.host_wanted.store(host, Ordering::Relaxed);
    let live = !HOST_FUSE.is_off() && fresh(STATE.host_ms.load(Ordering::Acquire), now_ms());
    answer(host, live, STATE.host.snapshot())
}

/// The answer for `host` from `read`, the last whole read, while the reads
/// are `live`: on and renewed by a step.
fn answer(
    host: u32,
    live: bool,
    read: Option<(u32, [u8; 16], [i64; 3])>,
) -> Option<([u8; 16], [i64; 3])> {
    let (read, template, position) = read.filter(|_| live)?;
    (host != 0 && read == host && template != [0; 16]).then_some((template, position))
}

/// Whether a read at `read` milliseconds is still the answer at `now`; 0 is
/// never read.
fn fresh(read: u64, now: u64) -> bool {
    read != 0 && now.saturating_sub(read) <= FRESH_MS
}

unsafe extern "system" fn player_ui(ctx: *mut u64) -> u64 {
    // Until a mod asks, the system runs as the game's alone.
    let host = STATE.host_wanted.load(Ordering::Relaxed);
    if host != 0
        && let Some(at) = STATE.host_layout.get()
        && !ctx.is_null()
    {
        // SAFETY: `ctx` is non-null by the check before it and is the
        // system's own context, at its entry on its thread.
        let _read = HOST_FUSE.run(HOST_FEATURE, || unsafe { read_host(ctx, at, host) });
    }
    if STATE.asked.load(Ordering::Relaxed)
        && let Some(layout) = STATE.layout.get()
        && !ctx.is_null()
    {
        // SAFETY: `ctx` is non-null by the check before it and is the
        // system's own context, at its entry on its thread.
        let _read = FUSE.run(FEATURE, || unsafe {
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

    #[test]
    fn a_host_read_comes_back_whole_and_only_for_its_own_entity() {
        let read = HostRead::new();
        assert_eq!(read.snapshot(), Some((0, [0; 16], [0; 3])));
        let template = core::array::from_fn(|i| u8::try_from(i + 1).unwrap());
        read.write(42, template, [1 << 32, -(5 << 32), 7]);
        assert_eq!(
            read.snapshot(),
            Some((42, template, [1 << 32, -(5 << 32), 7]))
        );
        read.seq.fetch_add(1, Ordering::Relaxed);
        assert_eq!(read.snapshot(), None, "mid-write");
    }

    #[test]
    fn the_interface_asks_for_its_host_and_gets_it_once_a_step_read_it() {
        assert_eq!(screen_host(42), None);
        assert_eq!(STATE.host_wanted.load(Ordering::Relaxed), 42);
        STATE.host.write(42, [9; 16], [3, 4, 5]);
        STATE.host_ms.store(now_ms(), Ordering::Release);
        assert_eq!(screen_host(42), Some(([9; 16], [3, 4, 5])));
        assert_eq!(screen_host(43), None, "another host's read");
        STATE.host.write(43, [0; 16], [0; 3]);
        assert_eq!(screen_host(43), None, "no template read");
        assert_eq!(screen_host(0), None);
        assert_eq!(STATE.host_wanted.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_host_read_no_step_renewed_is_no_answer() {
        assert!(!fresh(0, 5), "never read");
        assert!(fresh(100, 100 + FRESH_MS));
        assert!(
            !fresh(100, 101 + FRESH_MS),
            "a read from before a world load"
        );
    }

    #[test]
    fn a_read_after_the_reads_turned_off_or_went_stale_is_no_answer() {
        let read = Some((42, [9; 16], [3, 4, 5]));
        assert_eq!(answer(42, true, read), Some(([9; 16], [3, 4, 5])));
        assert_eq!(answer(42, false, read), None);
    }
}
