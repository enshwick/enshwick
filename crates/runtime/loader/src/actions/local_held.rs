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
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use enshwick_core::Entity;
use enshwick_core::abi::{HookFlags, Side};
use enshwick_gamedata::catalogue;

use crate::actions::player_state;
use crate::engine::copied::Copied;
use crate::engine::ecs::{Components, Walk};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;
use crate::engine::world::TemplateWalk;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    // The query walk's and the component reads', all of them, so this row
    // names what turns the held read off.
    pinned::ecs::COMPONENT_READ_RAW.id(),
    pinned::ecs::CONTEXT_ENTITY_WORLD.id(),
    pinned::ecs::CONTEXT_ROW.id(),
    pinned::ecs::DESCRIPTOR_ENTITY_RECORDS.id(),
    pinned::ecs::ENTITY_RECORD_SIZE.id(),
    pinned::ecs::ITERATOR_INIT.id(),
    pinned::ecs::ITERATOR_NEXT.id(),
    pinned::ecs::REGISTRY_TYPE_MAP.id(),
    pinned::ecs::REGISTRY_TYPE_MAP_FALLBACK.id(),
    pinned::ecs::SLOT_ROOT.id(),
    pinned::ecs::TYPE_INDEX.id(),
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
pub(crate) const FRESH_MS: u64 = 1000;
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
    walk: &'static Walk,
    components: Components,
    input: u32,
    digital: usize,
    /// Where the local player check's word holds the local player's id.
    local_player_id: usize,
    /// The iterator word `player_ui` hands its local player check with each
    /// row's entity.
    local_player: usize,
    /// Where the local player's own state reads, on the same row.
    state: player_state::Layout,
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
/// words and its fixed-point position, copied whole, so the interface
/// thread never takes half of one read without a lock.
struct HostRead(Copied<6>);

impl HostRead {
    const fn new() -> Self {
        Self(Copied::new())
    }

    /// Stores a read; only the system's thread writes.
    fn write(&self, entity: u32, template: [u8; 16], position: [i64; 3]) {
        let [lo, hi] = bytemuck::cast::<[u8; 16], [u64; 2]>(template);
        let [x, y, z] = position.map(i64::cast_unsigned);
        self.0.write([u64::from(entity), lo, hi, x, y, z]);
    }

    /// The last whole read, or `None` while the writer stays mid-write.
    fn snapshot(&self) -> Option<(u32, [u8; 16], [i64; 3])> {
        let [entity, lo, hi, x, y, z] = self.0.load()?;
        let template = bytemuck::cast::<[u64; 2], [u8; 16]>([lo, hi]);
        let entity = u32::try_from(entity).unwrap_or(0);
        Some((entity, template, [x, y, z].map(u64::cast_signed)))
    }
}

/// The reads' state: the layout once linked, whether a mod asked yet, the
/// last mask and when it was read, by [`now_ms`], which never reads 0.
struct State {
    layout: OnceLock<Layout>,
    asked: AtomicBool,
    linked: AtomicBool,
    bits: AtomicU64,
    /// The player's own keys of `bits`, without those the loader's presses
    /// wrote, taken in the same read so a reader never pairs two reads.
    players: AtomicU64,
    /// The local player's entity at the last mask read, 0 before one.
    player: AtomicU32,
    read_ms: AtomicU64,
    /// Why the last step that tried found no mask, a [`Miss`], or 0 when
    /// it read one.
    miss: AtomicU8,
    /// When a step last found no mask, as `read_ms` counts.
    miss_ms: AtomicU64,
    /// Where the host reads run, once linked on a build that has them.
    host_layout: OnceLock<HostLayout>,
    /// The open screen's host the interface asks about, 0 for none.
    host_wanted: AtomicU32,
    host: HostRead,
    /// When a step last wrote `host`, as `read_ms` counts.
    host_ms: AtomicU64,
    /// A higher layer's read of where entities stand, which each step runs
    /// while it asks: whether it asks, and the read.
    positions: OnceLock<Positions>,
}

/// Whether a higher layer wants where entities stand, and its read, which
/// gets the step's read of one entity's fixed-point position.
type Positions = (fn() -> bool, fn(&dyn Fn(u32) -> Option<[i64; 3]>));

/// Hands each step from now on `read`, run while `wants` answers yes, with
/// where an entity of the client's world stands: its `CurrentTransform`'s
/// 32.32 metres, `None` for one the world lacks. On a build without the
/// host reads' layout no step runs it.
pub(crate) fn read_positions(wants: fn() -> bool, read: fn(&dyn Fn(u32) -> Option<[i64; 3]>)) {
    let _first = STATE.positions.set((wants, read));
}

impl State {
    /// The state before any link, ask or read.
    const fn new() -> Self {
        Self {
            layout: OnceLock::new(),
            asked: AtomicBool::new(false),
            linked: AtomicBool::new(false),
            bits: AtomicU64::new(0),
            players: AtomicU64::new(0),
            player: AtomicU32::new(0),
            read_ms: AtomicU64::new(0),
            miss: AtomicU8::new(0),
            miss_ms: AtomicU64::new(0),
            host_layout: OnceLock::new(),
            host_wanted: AtomicU32::new(0),
            host: HostRead::new(),
            host_ms: AtomicU64::new(0),
            positions: OnceLock::new(),
        }
    }

    /// The held mask's sample at `now`, with the reads turned `off` or not;
    /// asking starts the reads.
    fn sample(&self, off: bool, now: u64) -> Sample {
        if !self.asked.load(Ordering::Relaxed) {
            self.asked.store(true, Ordering::Relaxed);
        }
        sample_at(self.raw(off), now)
    }

    /// The reads' raw state, without asking.
    fn raw(&self, off: bool) -> Raw {
        Raw {
            off,
            read: self.read_ms.load(Ordering::Acquire),
            bits: self.bits.load(Ordering::Relaxed),
            miss: self.miss.load(Ordering::Relaxed),
            miss_ms: self.miss_ms.load(Ordering::Relaxed),
        }
    }

    /// [`held`] at `now`, with the reads turned `off` or not.
    fn held(&self, off: bool, now: u64) -> Result<u64, HeldError> {
        match self.sample(off, now) {
            Sample::Fresh { bits, .. } => Ok(bits),
            Sample::Never { .. } | Sample::Stale { .. } | Sample::Off => Err(HeldError),
        }
    }

    /// [`players_held`] at `now`, with the reads turned `off` or not.
    fn players_held(&self, off: bool, now: u64) -> Option<u64> {
        matches!(self.sample(off, now), Sample::Fresh { .. })
            .then(|| self.players.load(Ordering::Relaxed))
    }

    /// [`screen_host`] at `now`, with the host reads turned `on` or not.
    fn screen_host(&self, host: u32, on: bool, now: u64) -> Option<([u8; 16], [i64; 3])> {
        self.host_wanted.store(host, Ordering::Relaxed);
        let live = on && fresh(self.host_ms.load(Ordering::Acquire), now);
        answer(host, live, self.host.snapshot())
    }

    /// [`status`] at `now`, with the shared link and the read turned on or
    /// not; never asks.
    fn status(&self, link_on: bool, read_on: bool, now: u64) -> enshwick_core::abi::LocalInputInfo {
        cached_status(
            self.asked.load(Ordering::Relaxed),
            self.layout.get().is_some(),
            self.linked.load(Ordering::Acquire) && link_on,
            read_on,
            self.read_ms.load(Ordering::Acquire),
            now,
        )
    }
}

static STATE: State = State::new();
static FUSE: Fuse = Fuse::new(FEATURE);
static HOST_FUSE: Fuse = Fuse::new(HOST_FEATURE);
/// The switch of the link on [`SYSTEM`], which both reads share: each read
/// runs under its own fuse inside it, so a panic in one leaves the other
/// linked.
static LINK_FUSE: Fuse = Fuse::new("the local player's interface reads");

pub(crate) use crate::engine::clock::now_ms;

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
        walk: crate::engine::ecs::walk(loader)?,
        components: Components::found(loader)?,
        input: catalogue::type_hash(catalogue::ids::CLIENT_PLAYER_INPUT)
            .ok_or(catalogue::ids::CLIENT_PLAYER_INPUT)?,
        digital,
        local_player_id: loader
            .offset(pinned::ecs::LOCAL_PLAYER_ID)
            .ok_or(pinned::ecs::LOCAL_PLAYER_ID.id())?,
        state: player_state::Layout::resolve(loader),
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
        components: Components::found(loader)?,
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
    let linked = loader.link_feature(&LINK_FUSE, target, &NEXT, player_ui, HookFlags::NONE);
    STATE.linked.store(linked, Ordering::Release);
    linked
}

/// Reads the local player's mask from `player_ui`'s context.
///
/// # Safety
///
/// - `ctx` is the `player_ui` system's live, non-null context, at its entry
///   on its thread; the caller puts its row counter back afterwards, even
///   after a panic.
unsafe fn read(ctx: *mut u64, layout: &Layout) {
    // SAFETY: the caller's live `player_ui` context at its entry on its
    // thread; the caller puts its row counter back.
    let found = unsafe {
        local_player(
            ctx,
            layout.walk,
            layout.local_player,
            layout.local_player_id,
        )
    };
    let Some(player) = found else {
        missed(Miss::NoPlayer);
        return;
    };
    // SAFETY: the caller's live context at the system's entry on its thread.
    let Some(input) = (unsafe { layout.components.component(ctx, player.get(), layout.input) })
    else {
        missed(Miss::NoInput);
        return;
    };
    // SAFETY: `input` is the player's live `ClientPlayerInput`, unwritten
    // until the system runs; `layout` checked that the u64 at `digital` lies
    // inside the type.
    let bits: u64 = unsafe { enshwick_core::raw::read_at(input, layout.digital) };
    STATE.bits.store(bits, Ordering::Relaxed);
    // The builder's pass that wrote this mask left the presses' bits.
    let players = bits & !crate::actions::client_input::written();
    STATE.players.store(players, Ordering::Relaxed);
    STATE.player.store(player.get(), Ordering::Relaxed);
    STATE.miss.store(0, Ordering::Relaxed);
    let at = now_ms();
    let _kept = player_state::FUSE.run(player_state::FEATURE, || {
        // SAFETY: the caller's live context at the system's entry on its
        // thread, and `input` the local player's live `ClientPlayerInput`.
        unsafe {
            player_state::read(
                ctx,
                &layout.components,
                &layout.state,
                player.get(),
                input,
                at,
            );
        }
    });
    STATE.read_ms.store(at, Ordering::Release);
}

/// Keeps why a step found no mask, logging a new cause once.
fn missed(miss: Miss) {
    STATE.miss_ms.store(now_ms(), Ordering::Relaxed);
    if STATE.miss.swap(miss as u8, Ordering::Relaxed) != miss as u8 {
        log_miss(miss);
    }
}

/// The local player's entity: the row of `player_ui`'s query that passes
/// the system's own local player check, or `None` when no row does. The
/// walk starts from the first row, wherever the counter stood; the
/// iterator word `local_player` points at the check's word, which holds
/// the local player's id at `local_player_id`.
///
/// # Safety
///
/// - `ctx` is the `player_ui` system's live, non-null context, at its entry
///   on its thread; the caller puts its row counter back.
unsafe fn local_player(
    ctx: *mut u64,
    walk: &Walk,
    local_player: usize,
    local_player_id: usize,
) -> Option<Entity> {
    // SAFETY: the caller's live context, on its thread, which alone writes
    // the row counter while the system runs; the caller puts it back.
    unsafe { walk.set_row(ctx, 0) };
    // SAFETY: the caller's contract; `link` checked the routed iterator size
    // is `WORDS`, and `query` drops at the end of this function, inside the
    // system's run.
    let mut query = unsafe { walk.query::<WORDS>(ctx) };
    let rows = core::iter::from_fn(|| {
        let words = query.next()?;
        let local = usize::try_from(words[local_player]).unwrap_or(0);
        if local == 0 {
            return Some(None);
        }
        // SAFETY: `local` is the non-null word `player_ui` itself reads its
        // local player id from at `ecs.local_player_id` for this row, at the
        // routed iterator word, live while the system runs.
        let id: u32 = unsafe { crate::engine::memory::read(local, local_player_id) };
        // SAFETY: the caller's live context on its thread, whose row counter
        // `query` set to this row.
        let row = unsafe { walk.entity(ctx) };
        Some(Some((Entity::from_engine(id), Entity::from_engine(row))))
    });
    first_local(rows.flatten())
}

/// Runs `read` on the live context `ctx` and puts its row counter back
/// afterwards; `read` catches its own panics.
///
/// # Safety
///
/// - `ctx` is a live query context on its system's thread, which alone
///   writes the row counter while the system runs.
unsafe fn row_kept<R>(walk: &Walk, ctx: *mut u64, read: impl FnOnce() -> R) -> R {
    // SAFETY: the caller's live context on its thread.
    let row = unsafe { walk.row(ctx) };
    let answer = read();
    // SAFETY: the caller's live context on its thread, which alone writes
    // the row counter.
    unsafe { walk.set_row(ctx, row) };
    answer
}

/// The first row entity that equals the local player id read beside it,
/// from `(id, row entity)` pairs; entity 0, the answer before a query takes
/// a row, is never the local player.
fn first_local(rows: impl IntoIterator<Item = (Entity, Entity)>) -> Option<Entity> {
    rows.into_iter()
        .find_map(|(id, row)| (row.get() != 0 && row == id).then_some(row))
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
    let template =
        world.and_then(|world| unsafe { at.walk.template(world, Entity::from_engine(host)) });
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
    STATE.screen_host(host, !HOST_FUSE.is_off(), now_ms())
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
pub(crate) fn fresh(read: u64, now: u64) -> bool {
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
        if let Some((wants, read)) = STATE.positions.get()
            && wants()
            && let Some(at) = STATE.host_layout.get()
            && !ctx.is_null()
        {
            read(&|entity| {
                // SAFETY: `ctx` is non-null by the check before it and is the
                // system's own context, at its entry on its thread.
                let transform = unsafe { at.components.component(ctx, entity, at.transform) }?;
                // SAFETY: `transform` is the entity's live `CurrentTransform`,
                // whose position, three fixed-point `i64`, sits at the
                // reflected offset `at.position`.
                Some(unsafe { enshwick_core::raw::read_at::<[i64; 3]>(transform, at.position) })
            });
        }
        if STATE.asked.load(Ordering::Relaxed)
            && let Some(layout) = STATE.layout.get()
            && !ctx.is_null()
        {
            // SAFETY: `ctx` is non-null by the check before it and is the
            // system's own context, at its entry on its thread; `row_kept`
            // below puts its row counter back.
            let fused = || FUSE.run(FEATURE, || unsafe { read(ctx, layout) });
            // SAFETY: `ctx` is non-null by the check before it and is the
            // system's own context on its thread, which alone writes the row
            // counter; it goes back whether the read panics or not, since the
            // fuse catches the panic.
            let _read = unsafe { row_kept(layout.walk, ctx, fused) };
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
    /// The read, or the link both reads share, is off on this build or
    /// after a panic.
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

/// The reads' raw state: `read` and `miss_ms` in milliseconds as
/// [`now_ms`] counts, 0 for never, the mask `bits` and the raw `miss`.
#[derive(Clone, Copy, Debug)]
struct Raw {
    off: bool,
    read: u64,
    bits: u64,
    miss: u8,
    miss_ms: u64,
}

/// The sample at `now` from `raw`; a miss counts only while recent, so
/// one the last step left before the steps stopped names no cause.
fn sample_at(raw: Raw, now: u64) -> Sample {
    let miss = Miss::from_raw(raw.miss).filter(|_| fresh(raw.miss_ms, now));
    let age_ms = now.saturating_sub(raw.read);
    if raw.off {
        Sample::Off
    } else if raw.read == 0 {
        Sample::Never { miss }
    } else if fresh(raw.read, now) {
        Sample::Fresh {
            bits: raw.bits,
            age_ms,
        }
    } else {
        Sample::Stale {
            bits: raw.bits,
            age_ms,
            miss,
        }
    }
}

/// The held mask's sample now; asking starts the reads.
pub(crate) fn sample() -> Sample {
    STATE.sample(off(), now_ms())
}

/// Whether the held read, or the link it runs in, is turned off.
fn off() -> bool {
    FUSE.is_off() || LINK_FUSE.is_off()
}

/// The local player's held mask, zero bits when none is held, or
/// [`HeldError`] while no fresh one is read. The first ask starts the
/// reads, so it answers [`HeldError`]; a step that tries and finds no mask
/// logs why, once per cause.
pub(crate) fn held() -> Result<u64, HeldError> {
    STATE.held(off(), now_ms())
}

/// The keys the player holds in a fresh read: its mask without the bits the
/// loader's own presses wrote into it; `None` while no fresh one is read.
pub(crate) fn players_held() -> Option<u64> {
    STATE.players_held(off(), now_ms())
}

/// Whether the build reads the local player's inventory slots: the held
/// read and its link are on, and the slot read resolved and is on.
pub(crate) fn slots_run() -> bool {
    STATE.linked.load(Ordering::Acquire)
        && !off()
        && !player_state::FUSE.is_off()
        && !player_state::SLOTS_FUSE.is_off()
        && STATE
            .layout
            .get()
            .is_some_and(|layout| layout.state.reads_slots())
}

/// The local player's entity at the last mask read; `None` before one.
pub(crate) fn player() -> Option<Entity> {
    let player = STATE.player.load(Ordering::Relaxed);
    (player != 0).then_some(Entity::from_engine(player))
}

#[cold]
fn log_miss(miss: Miss) {
    match miss {
        Miss::NoPlayer => crate::log_once!(
            INFO,
            "{FEATURE}: no row of {SYSTEM}'s query passed its local player check"
        ),
        Miss::NoInput => crate::log_once!(
            INFO,
            "{FEATURE}: the local player's row has no ClientPlayerInput"
        ),
    }
}

/// Cached loader metadata only; never starts sampling or accesses a game object.
pub(crate) fn status() -> enshwick_core::abi::LocalInputInfo {
    STATE.status(!LINK_FUSE.is_off(), !FUSE.is_off(), now_ms())
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "independent cached sampler gates are combined without inferring a game state"
)]
fn cached_status(
    requested: bool,
    layout: bool,
    linked: bool,
    enabled: bool,
    read: u64,
    now: u64,
) -> enshwick_core::abi::LocalInputInfo {
    use enshwick_core::abi::LocalInputInfo as Info;
    let mut flags = 0;
    for (present, flag) in [
        (requested, Info::REQUESTED),
        (layout, Info::LAYOUT_AVAILABLE),
        (linked, Info::LINK_AVAILABLE),
        (enabled, Info::READ_ENABLED),
        (read != 0, Info::SAMPLE_PRESENT),
        (
            requested && layout && linked && enabled && fresh(read, now),
            Info::FRESH,
        ),
    ] {
        if present {
            flags |= flag;
        }
    }
    enshwick_abi::headed!(Info {
        flags,
        age_ms: if read == 0 {
            Info::EMPTY.age_ms
        } else {
            now.saturating_sub(read)
        },
        ..Info::EMPTY
    })
}

#[cfg(test)]
mod tests;
