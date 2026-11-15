//! The game controls the local player holds, read on the client at the
//! entry of the interface's own player system, `player_ui`, which runs on
//! the local player: its `ClientPlayerInput`'s digital mask, kept for any
//! thread to read.
//!
//! `player_ui` walks a query over the players and serves only the row whose
//! entity is the local player's (client 26430625); at its entry no row is
//! taken yet, so the read walks the query itself and puts the row counter
//! back. The mask is the last step's, since the system writes the new one
//! as it runs.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering, fence};
use std::time::Instant;

use enshwick_core::Entity;
use enshwick_core::abi::{HookFlags, Side};
use enshwick_gamedata::catalogue;

use crate::engine::ecs::{Components, Engine};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;
use crate::engine::world::TemplateWalk;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::COMPONENT_READ_RAW.id(),
    pinned::ecs::ITERATOR_NEXT.id(),
    pinned::ecs::LOCAL_PLAYER_ID.id(),
    pinned::ecs::PLAYER_UI_ITERATOR.id(),
    pinned::ecs::PLAYER_UI_ITERATOR_SIZE.id(),
    pinned::ecs::PLAYER_UI_LOCAL_PLAYER.id(),
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
/// `player_ui`'s components, in the order its iterator lays them out.
/// Found on client 26430625 and checked against the system's reflected
/// components on every build.
const READS: [&str; 19] = [
    "BuffContainer",
    "ClientActor",
    "ClientCameraControl",
    "ClientCursor",
    "ClientInteractionQuery",
    "CspHackInventoryStockSelection",
    "InventorySetup",
    "NetworkCursor",
    "NetworkFishing",
    "DynamicNetworkLocomotion",
    "NetworkPlayerAttributes",
    "NetworkRangedWeapon",
    "NetworkSelectedTargets",
    "PlayerPinnedRecipes",
    "PlayerQuestProgressionMode",
    "PlayerSkillsUi",
    "PlayerState",
    "RenderTransform",
    "SlotSelection",
];
const WRITES: [&str; 1] = ["ClientPlayerInput"];
/// `player_ui`'s iterator size in words, 0x3c8 bytes (client 26430625);
/// a build whose routed `ecs.player_ui_iterator_size` differs has the read
/// off.
const WORDS: usize = 121;

/// Where the mask is read on this build.
#[derive(Debug)]
struct Layout {
    engine: &'static Engine,
    components: Components,
    input: u32,
    digital: usize,
    /// Where the local player check's word holds the local player's id.
    local_player_id: usize,
    /// The iterator word `player_ui` hands its local player check with each
    /// row's entity.
    local_player: usize,
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
    /// Why the last step that tried found no mask, a [`Miss`], or 0 when
    /// it read one.
    miss: AtomicU8,
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
    miss: AtomicU8::new(0),
    host_layout: OnceLock::new(),
    host_wanted: AtomicU32::new(0),
    host: HostRead::new(),
    host_ms: AtomicU64::new(0),
};
static FUSE: Fuse = Fuse::new(FEATURE);
static HOST_FUSE: Fuse = Fuse::new(HOST_FEATURE);
/// The switch of the link on [`SYSTEM`], which both reads share: each read
/// runs under its own fuse inside it, so a panic in one leaves the other
/// linked.
static LINK_FUSE: Fuse = Fuse::new("the local player's interface reads");

fn now_ms() -> u64 {
    let start = STATE.start.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX - 1) + 1
}

fn layout(loader: &Loader) -> Result<Layout, &'static str> {
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
    let routed = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let local_player = local_player_word(
        routed(pinned::ecs::PLAYER_UI_ITERATOR_SIZE)?,
        routed(pinned::ecs::PLAYER_UI_ITERATOR)?,
        routed(pinned::ecs::PLAYER_UI_LOCAL_PLAYER)?,
    )
    .ok_or(pinned::ecs::PLAYER_UI_ITERATOR_SIZE.id())?;
    Ok(Layout {
        local_player,
        engine: crate::engine::ecs::engine(loader).ok_or(pinned::ecs::ITERATOR_NEXT.id())?,
        components: Components::resolve(loader).ok_or(pinned::ecs::COMPONENT_READ_RAW.id())?,
        input: catalogue::type_hash(catalogue::ids::CLIENT_PLAYER_INPUT)
            .ok_or(catalogue::ids::CLIENT_PLAYER_INPUT)?,
        digital,
        local_player_id: loader
            .offset(pinned::ecs::LOCAL_PLAYER_ID)
            .ok_or(pinned::ecs::LOCAL_PLAYER_ID.id())?,
    })
}

/// The iterator word `player_ui` hands its local player check, from its
/// routed iterator size and the frame offsets of its iterator and that word;
/// `None` unless the iterator is the [`WORDS`] the read lays out and the
/// word is a whole word inside it, so a reshuffled build turns the read off.
fn local_player_word(size: usize, iterator: usize, word: usize) -> Option<usize> {
    let at = word.checked_sub(iterator)?;
    (size == WORDS * size_of::<u64>()
        && at % size_of::<u64>() == 0
        && at / size_of::<u64>() < WORDS)
        .then_some(at / size_of::<u64>())
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
    let entry = loader
        .reflection
        .system(SYSTEM)
        .ok()
        .map(enshwick_gamedata::reflect::System::entry);
    if entry.is_none() {
        // Without the system neither read links: one cause, one entry off.
        let _linked = link_system(loader, None);
        return;
    }
    let mut any = false;
    let held = crate::engine::ecs::components_match(loader, SYSTEM, &READS, &WRITES)
        .map_err(|why| (SYSTEM, why.to_string()))
        .and_then(|()| layout(loader).map_err(|id| (id, "this build lacks it".to_owned())));
    match held {
        Ok(layout) => any |= STATE.layout.set(layout).is_ok(),
        Err((id, why)) => {
            let _off = loader.fuse_off(&FUSE, id, &why);
        }
    }
    match host_layout(loader) {
        Ok(at) => any |= STATE.host_layout.set(at).is_ok(),
        Err(id) => {
            let _off = loader.fuse_off(&HOST_FUSE, id, "this build lacks it");
        }
    }
    if !any {
        return;
    }
    let _linked = link_system(loader, entry);
}

/// Links the reads at `entry`, the system's, under [`LINK_FUSE`], which
/// alone lists a failed link off: no step renews either read after it, so
/// both answer as stale. True when linked.
fn link_system(loader: &Loader, entry: Option<u64>) -> bool {
    let target = Target::Found(SYSTEM, entry);
    loader.link_fused(&LINK_FUSE, target, &NEXT, player_ui, HookFlags::NONE)
}

/// Reads the local player's mask from `player_ui`'s context.
///
/// # Safety
///
/// - `ctx` is the `player_ui` system's live, non-null context, at its entry
///   on its thread; the caller puts its row counter back afterwards, even
///   after a panic.
unsafe fn read(ctx: *mut u64, layout: &Layout) {
    // SAFETY: the caller's live context, on its thread, which alone writes
    // the row counter while the system runs; the caller puts it back.
    unsafe { layout.engine.set_row(ctx, 0) };
    // SAFETY: the caller's live `player_ui` context at its entry on its
    // thread, its row counter set to 0 above.
    let Some(player) = (unsafe { local_player(ctx, layout) }) else {
        STATE.miss.store(Miss::NoPlayer as u8, Ordering::Relaxed);
        return;
    };
    // SAFETY: the caller's live context at the system's entry on its thread.
    let Some(input) = (unsafe { layout.components.component(ctx, player.0, layout.input) }) else {
        STATE.miss.store(Miss::NoInput as u8, Ordering::Relaxed);
        return;
    };
    // SAFETY: `input` is the player's live `ClientPlayerInput`, unwritten
    // until the system runs; `layout` checked that the u64 at `digital` lies
    // inside the type.
    let bits: u64 = unsafe { enshwick_core::raw::read_at(input, layout.digital) };
    STATE.bits.store(bits, Ordering::Relaxed);
    STATE.miss.store(0, Ordering::Relaxed);
    STATE.read_ms.store(now_ms(), Ordering::Release);
}

/// The local player's entity: the row of `player_ui`'s query that passes
/// the system's own local player check, or `None` when no row does.
///
/// # Safety
///
/// - `ctx` is the `player_ui` system's live, non-null context, at its entry
///   on its thread, with its row counter at 0; the caller puts it back.
unsafe fn local_player(ctx: *mut u64, layout: &Layout) -> Option<Entity> {
    let engine = layout.engine;
    // SAFETY: the caller's contract; `link` checked the routed iterator size
    // is `WORDS`, and `query` drops at the end of this function, inside the
    // system's run.
    let mut query = unsafe { engine.query::<WORDS>(ctx) };
    let rows = core::iter::from_fn(|| {
        let words = query.next()?;
        let local = usize::try_from(words[layout.local_player]).unwrap_or(0);
        if local == 0 {
            return Some(None);
        }
        // SAFETY: `local` is the non-null word `player_ui` itself reads its
        // local player id from at `ecs.local_player_id` for this row, at the
        // routed iterator word, live while the system runs.
        let id: u32 = unsafe { crate::engine::memory::read(local, layout.local_player_id) };
        // SAFETY: the caller's live context on its thread, whose row counter
        // `query` set to this row.
        let row = unsafe { engine.entity(ctx) };
        Some(Some((Entity(id), Entity(row))))
    });
    first_local(rows.flatten())
}

/// The first row entity that equals the local player id read beside it,
/// from `(id, row entity)` pairs; entity 0, the answer before a query takes
/// a row, is never the local player.
fn first_local(rows: impl IntoIterator<Item = (Entity, Entity)>) -> Option<Entity> {
    rows.into_iter()
        .find_map(|(id, row)| (row.0 != 0 && row == id).then_some(row))
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
        transform.map(|data| unsafe { enshwick_core::raw::read_at::<[i64; 3]>(data, at.position) });
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

crate::engine::chains::chain_detour! {
    /// The reflected `player_ui` system's entry, where both reads run, each
    /// under its own fuse, not this link's: a panic in one turns off only
    /// that read. Until a mod asks, the system runs as the game's alone.
    fn player_ui(ctx: *mut u64) -> u64;
    next NEXT;
    fuse &LINK_FUSE, "the local player's interface reads";
    |onward| {
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
            let row = unsafe { layout.engine.row(ctx) };
            // SAFETY: `ctx` is non-null by the check before it and is the
            // system's own context, at its entry on its thread; its row
            // counter goes back below, whether the read panics or not.
            let _read = FUSE.run(FEATURE, || unsafe { read(ctx, layout) });
            // SAFETY: `ctx` is non-null by the check before it and is the
            // system's own context on its thread, which alone writes the row
            // counter while the system runs.
            unsafe { layout.engine.set_row(ctx, row) };
        }
        onward.call()
    }
}

/// The local player's held mask is unread: no step read it for
/// [`FRESH_MS`], on a server or on a build without it.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("the held controls are not read")]
pub(crate) struct HeldError;

/// Why a step that tried to read the mask found none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Miss {
    /// No row of `player_ui`'s query passed its local player check.
    NoPlayer = 1,
    /// The local player's row had no `ClientPlayerInput`.
    NoInput = 2,
}

impl Miss {
    fn from_raw(raw: u8) -> Option<Self> {
        [Self::NoPlayer, Self::NoInput]
            .into_iter()
            .find(|miss| *miss as u8 == raw)
    }
}

/// The held mask as the reads keep it: a mask read with no control held is
/// a sample of zero bits, and a stale one stays apart from none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sample {
    /// The read is off on this build or after a panic.
    Off,
    /// No step read a mask yet; `miss` says why the last one that tried
    /// found none.
    Never { miss: Option<Miss> },
    /// The last mask read, older than [`FRESH_MS`], as in a menu; `miss`
    /// says why the steps since found none, when one tried.
    Stale {
        bits: u64,
        age_ms: u64,
        miss: Option<Miss>,
    },
    /// A mask a step read within [`FRESH_MS`].
    Fresh { bits: u64, age_ms: u64 },
}

/// The sample from the reads' raw state: `read` milliseconds as
/// [`now_ms`] counts, 0 for never, the mask `bits` and the raw `miss`.
fn sample_at(off: bool, read: u64, now: u64, bits: u64, miss: u8) -> Sample {
    let miss = Miss::from_raw(miss);
    let age_ms = now.saturating_sub(read);
    if off {
        Sample::Off
    } else if read == 0 {
        Sample::Never { miss }
    } else if age_ms > FRESH_MS {
        Sample::Stale { bits, age_ms, miss }
    } else {
        Sample::Fresh { bits, age_ms }
    }
}

/// The held mask's sample now; asking starts the reads.
pub(crate) fn sample() -> Sample {
    if !STATE.asked.load(Ordering::Relaxed) {
        STATE.asked.store(true, Ordering::Relaxed);
    }
    let read = STATE.read_ms.load(Ordering::Acquire);
    sample_at(
        FUSE.is_off(),
        read,
        now_ms(),
        STATE.bits.load(Ordering::Relaxed),
        STATE.miss.load(Ordering::Relaxed),
    )
}

/// The local player's held mask, zero bits when none is held, or
/// [`HeldError`] while no fresh one is read. The first ask starts the
/// reads, so it answers [`HeldError`]; a step that tries and finds no mask
/// logs why, once per cause.
pub(crate) fn held() -> Result<u64, HeldError> {
    match sample() {
        Sample::Fresh { bits, .. } => Ok(bits),
        Sample::Never { miss } | Sample::Stale { miss, .. } => {
            log_miss(miss);
            Err(HeldError)
        }
        Sample::Off => Err(HeldError),
    }
}

#[cold]
fn log_miss(miss: Option<Miss>) {
    match miss {
        Some(Miss::NoPlayer) => crate::log_once!(
            INFO,
            "{FEATURE}: no row of {SYSTEM}'s query passed its local player check"
        ),
        Some(Miss::NoInput) => crate::log_once!(
            INFO,
            "{FEATURE}: the local player's row has no ClientPlayerInput"
        ),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_in_the_held_read_leaves_the_host_reads_linked() {
        use enshwick_base::Relock;

        let loader = crate::start::tests::bare_loader();
        assert!(link_system(&loader, Some(0x7000)));
        let head: &'static core::sync::atomic::AtomicUsize =
            Box::leak(Box::new(core::sync::atomic::AtomicUsize::new(0)));
        {
            let mut chains = loader.chains.relock();
            chains.seal();
            for chain in chains.iter_mut() {
                chain.head = Some(head);
                chain.original = 0xff;
                chain.wire(|_| false);
            }
        }
        let ours = crate::engine::chains::Next::address_of(
            player_ui as unsafe extern "system" fn(*mut u64) -> u64,
        );
        assert_eq!(head.load(Ordering::Acquire), ours);
        let caught = FUSE.run(FEATURE, || panic!("a test"));
        assert_eq!(caught, None::<()>);
        loader.rewire();
        assert!(FUSE.is_off());
        assert!(!HOST_FUSE.is_off() && !LINK_FUSE.is_off());
        assert_eq!(
            head.load(Ordering::Acquire),
            ours,
            "the host reads still run in the system"
        );
    }

    #[test]
    fn a_system_that_does_not_link_lists_only_the_shared_link_off() {
        let loader = crate::start::tests::bare_loader();
        assert!(!link_system(&loader, None));
        assert!(LINK_FUSE.is_off());
        assert!(!FUSE.is_off() && !HOST_FUSE.is_off());
        let listed = crate::engine::diag::parts_off();
        let parts: Vec<&str> = listed.iter().map(|p| p.part.as_str()).collect();
        assert!(parts.contains(&LINK_FUSE.feature()), "{parts:?}");
        assert!(
            !parts.contains(&FEATURE) && !parts.contains(&HOST_FEATURE),
            "one cause, one entry: {parts:?}"
        );
    }

    #[test]
    fn a_client_without_player_ui_lists_only_the_shared_link_off() {
        let mut loader = crate::start::tests::bare_loader();
        loader.side = Side::CLIENT;
        link(&loader);
        let listed = crate::engine::diag::parts_off();
        let parts: Vec<&str> = listed.iter().map(|p| p.part.as_str()).collect();
        assert_eq!(parts, [LINK_FUSE.feature()], "one cause, one entry");
        assert!(!FUSE.is_off() && !HOST_FUSE.is_off());
    }

    #[test]
    fn the_held_read_takes_the_first_row_whose_entity_is_the_local_player() {
        let rows = |pairs: &[(u32, u32)]| {
            first_local(pairs.iter().map(|&(id, row)| (Entity(id), Entity(row))))
        };
        assert_eq!(
            rows(&[(0, 0)]),
            None,
            "entity 0, as at the system's entry, is never the local player"
        );
        assert_eq!(rows(&[(7, 3), (7, 7), (9, 9)]), Some(Entity(7)));
        assert_eq!(rows(&[(7, 3), (7, 4)]), None, "another player's rows");
        assert_eq!(rows(&[]), None);
    }

    #[test]
    fn the_local_player_word_comes_from_the_routed_frame_and_a_reshuffled_one_turns_the_read_off() {
        assert_eq!(
            local_player_word(0x3c8, 0x70, 0x190),
            Some(36),
            "client 26430625"
        );
        assert_eq!(
            local_player_word(0x3d0, 0x70, 0x190),
            None,
            "another iterator size"
        );
        assert_eq!(
            local_player_word(0x3c8, 0x70, 0x194),
            None,
            "not a whole word"
        );
        assert_eq!(
            local_player_word(0x3c8, 0x190, 0x70),
            None,
            "before the iterator"
        );
        assert_eq!(
            local_player_word(0x3c8, 0x70, 0x70 + 0x3c8),
            None,
            "past its end"
        );
        assert_eq!(
            local_player_word(0x3c8, 0x70, 0x70 + 0x3c0),
            Some(WORDS - 1)
        );
    }

    #[test]
    fn the_held_read_s_iterator_layout_matches_player_ui_on_the_installed_client() {
        use enshwick_gamedata::reflect::{Image, Layout, Reflection};

        let Some(exe) = std::env::var_os("ENSH_CLIENT_EXE") else {
            return;
        };
        let bytes = std::fs::read(exe).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let system = reflection.system(SYSTEM).unwrap();
        assert_eq!(system.reads(), READS);
        assert_eq!(system.writes(), WRITES);
    }

    #[test]
    fn the_first_ask_starts_the_reads_and_answers_missing() {
        assert!(
            !STATE.asked.load(Ordering::Relaxed),
            "the detour reads nothing"
        );
        assert_eq!(held(), Err(HeldError));
        assert!(STATE.asked.load(Ordering::Relaxed));
    }

    #[test]
    fn a_mask_read_with_no_control_held_is_a_fresh_sample_of_zero_bits() {
        assert_eq!(
            sample_at(false, 100, 150, 0, 0),
            Sample::Fresh {
                bits: 0,
                age_ms: 50
            }
        );
    }

    #[test]
    fn a_stale_sample_keeps_its_mask_and_age_and_stays_apart_from_never_read() {
        let stale = sample_at(false, 100, 101 + FRESH_MS, 1 << 30, Miss::NoPlayer as u8);
        assert_eq!(
            stale,
            Sample::Stale {
                bits: 1 << 30,
                age_ms: FRESH_MS + 1,
                miss: Some(Miss::NoPlayer)
            }
        );
        assert_eq!(
            sample_at(false, 0, 5_000, 0, Miss::NoInput as u8),
            Sample::Never {
                miss: Some(Miss::NoInput)
            }
        );
        assert_eq!(sample_at(false, 0, 5, 0, 0), Sample::Never { miss: None });
        assert_eq!(
            sample_at(true, 100, 150, 1, 0),
            Sample::Off,
            "a read turned off answers off, however fresh"
        );
    }

    #[test]
    fn an_unknown_raw_miss_reads_as_none() {
        assert_eq!(Miss::from_raw(0), None);
        assert_eq!(Miss::from_raw(1), Some(Miss::NoPlayer));
        assert_eq!(Miss::from_raw(2), Some(Miss::NoInput));
        assert_eq!(Miss::from_raw(3), None);
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
