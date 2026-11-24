//! The client's fog of war, copied for mods.
//!
//! Engine facts from client build 1273043 (age 426): the game keeps what a
//! player has uncovered of the map in one fog object, made when the client
//! enters a world and freed when it leaves. The ECS job `fog_of_war`
//! uncovers it on a worker thread through one reveal call per discovering
//! entity, `reveal(fog, &[x, z], range)`, its only caller; the load from
//! the player's save writes the tiles once at the start, and nothing else
//! writes them. The fog is square tiles of one byte per pixel, laid out
//! one after the other, row-major.
//!
//! The loader detours the reveal. After the game's own reveal returns, on
//! the same thread, it copies the tiles the reveal touched into its own
//! image, and all of them the first time it sees a fog, which mods read
//! from any thread. The reveal never waits for a mod: when a mod holds the
//! image, it notes the tiles it skipped and the next reveal copies them.
//! The loader also detours the fog's destroy, which the client's world
//! teardown alone calls, and drops its copy there, so no copy outlives its
//! world and a new fog at the same address starts a new copy.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use enshwick_core::abi::{FogInfo, Header, HookFlags, Side};

use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::pinned::Constant;
use crate::engine::registry::JoinFeatureError;
use crate::world::fog_image::{
    FogImage, FogShape, Plan, ReadError, TileRect, UNCOVER_RANGE_PER_RADIUS, plan,
};
use crate::world::openers::Openers;
use enshwick_base::Relock;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::map::FOG_DESTROY.id(),
    pinned::map::FOG_REVEAL.id(),
    pinned::map::FOG_SIZE_Z.id(),
    pinned::map::FOG_TILES.id(),
    pinned::map::FOG_TILES_WIDE.id(),
    pinned::map::FOG_TILE_EDGE_SHIFT.id(),
    pinned::map::FOG_TILE_SHIFT.id(),
];

/// The switch of the reveal and destroy links: the copy and the uncovers
/// the server asks for.
static FUSE: Fuse = Fuse::new("fog of war");

/// The switch of the copy alone, off on a build without its layout or
/// after a panic in the copy, while the uncovers run on.
static COPY_FUSE: Fuse = Fuse::new("fog of war copy");

/// The largest tile the copy takes, on the stack of the reveal's thread.
const MAX_TILE_BYTES: usize = 4096;

/// Where the fog object keeps what the copy reads, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    size_z: usize,
    tiles: usize,
    tiles_wide: usize,
    tile_bytes: usize,
    edge: u32,
}

static LAYOUT: std::sync::OnceLock<Layout> = std::sync::OnceLock::new();
/// What the fog keeps while the game runs.
struct State {
    /// The mods that opened the fog.
    openers: Openers,
    /// Fogs seen so far, never reset: a mod tells a new fog by its session.
    sessions: AtomicU64,
    /// Whether the reveal is linked, so an uncover has a reveal to run in.
    reveal_linked: AtomicBool,
    /// What the fog knows of the world that runs.
    current: World,
}

static STATE: State = State {
    openers: Openers::new(),
    sessions: AtomicU64::new(0),
    reveal_linked: AtomicBool::new(false),
    current: World::new(),
};

/// What the fog knows of one world, made new whole when its fog goes.
struct World {
    /// The copy, with the fog object it copies.
    mirror: Mutex<Option<Mirror>>,
    /// The tiles reveals skipped while a mod held the copy, as
    /// [`TileRect::pack`] writes them; 0 for none.
    skipped: AtomicU64,
    uncovers: Uncovers,
}

impl World {
    const fn new() -> Self {
        Self {
            mirror: Mutex::new(None),
            skipped: AtomicU64::new(0),
            uncovers: Uncovers {
                waiting: AtomicBool::new(false),
                warned: AtomicBool::new(false),
                circles: Mutex::new(Vec::new()),
            },
        }
    }

    /// Makes every field as [`World::new`] does, the copy through `mirror`,
    /// its lock held; the pattern names each field, so a field added later
    /// fails to build until it is reset here too.
    fn forget(&self, mut mirror: std::sync::MutexGuard<'_, Option<Mirror>>) {
        let Self {
            mirror: _,
            skipped,
            uncovers,
        } = self;
        *mirror = None;
        skipped.store(0, Ordering::Release);
        drop(mirror);
        uncovers.forget();
    }
}

/// The circles the server asked this client to uncover, each a centre and a
/// radius in metres, which the next reveal hands the game.
struct Uncovers {
    /// Whether `circles` holds any, read first on every reveal.
    waiting: AtomicBool,
    /// Whether an uncover the client dropped was logged since the last fog
    /// went, so a server that keeps asking fills no log.
    warned: AtomicBool,
    circles: Mutex<Vec<([f32; 2], f32)>>,
}

/// The circles that wait at most; a mod asks for a few, so one past this
/// is dropped with a warning.
const MAX_UNCOVERS: usize = 64;

/// The loader's copy of one fog object.
#[derive(Debug)]
struct Mirror {
    fog: usize,
    session: u64,
    image: FogImage,
    /// When the copy began, the clock of its windows of uncovering.
    started: std::time::Instant,
}

impl Mirror {
    /// The last closed window of uncovering, closing the pending one when
    /// it is due.
    fn uncovered(&mut self) -> crate::world::fog_image::Uncovered {
        let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.image.close_uncovered(ms)
    }
}

/// Opens the fog for mod `owner`; during init only, on a client.
///
/// # Errors
///
/// [`JoinFeatureError`] after init or off a client.
pub(crate) fn open(loader: &Loader, owner: ModIndex) -> Result<(), JoinFeatureError> {
    loader.before_delivery()?;
    if loader.side != Side::CLIENT {
        return Err(JoinFeatureError::Unavailable);
    }
    Ok(STATE.openers.open(owner)?)
}

/// Whether mod `owner` opened the fog.
pub(crate) fn opened(owner: ModIndex) -> bool {
    STATE.openers.opened(owner)
}

/// The fog's layout, or the first entry that is not in the build or does
/// not fit the copy, and why.
fn layout(loader: &Loader) -> Result<Layout, (Constant, &'static str)> {
    let offset = |id: Constant| loader.offset(id).ok_or((id, crate::engine::builds::ABSENT));
    let shift = |id: Constant| {
        u32::try_from(offset(id)?)
            .ok()
            .filter(|&shift| shift < 16)
            .ok_or((id, UNFIT))
    };
    let edge_shift = shift(pinned::map::FOG_TILE_EDGE_SHIFT)?;
    let tile_shift = shift(pinned::map::FOG_TILE_SHIFT)?;
    let (tile_bytes, edge) =
        tile(edge_shift, tile_shift).ok_or((pinned::map::FOG_TILE_SHIFT, UNFIT))?;
    Ok(Layout {
        size_z: offset(pinned::map::FOG_SIZE_Z)?,
        tiles: offset(pinned::map::FOG_TILES)?,
        tiles_wide: offset(pinned::map::FOG_TILES_WIDE)?,
        tile_bytes,
        edge,
    })
}

/// Why an entry the build has is still left out of the copy.
const UNFIT: &str = "does not fit the copy's tiles";

/// A tile's bytes and edge from the shifts of each, when they agree: a
/// tile is `edge` × `edge` bytes, at most [`MAX_TILE_BYTES`]; both shifts
/// are under 16.
fn tile(edge_shift: u32, tile_shift: u32) -> Option<(usize, u32)> {
    (tile_shift == edge_shift * 2 && 1 << tile_shift <= MAX_TILE_BYTES)
        .then(|| (1 << tile_shift, 1 << edge_shift))
}

/// Seals the mods that opened the fog, before any link.
pub(crate) fn seal() {
    let _openers = STATE.openers.seal();
}

/// Links the reveal on every client, for the uncovers its server asks for,
/// and copies the fog there when a mod opened it.
pub(crate) fn link(loader: &'static Loader) {
    if loader.side != Side::CLIENT {
        return;
    }
    if !STATE.openers.sealed().is_empty() {
        find_layout(loader);
    }
    let linked = link_at(
        loader,
        Target::Pinned(pinned::map::FOG_DESTROY),
        Target::Pinned(pinned::map::FOG_REVEAL),
    );
    STATE.reveal_linked.store(linked, Ordering::Release);
}

/// Keeps the fog's layout for the copy, or turns the copy off for the
/// first entry that is not in the build or does not fit.
fn find_layout(loader: &Loader) {
    match layout(loader) {
        Ok(found) => {
            let _first = LAYOUT.set(found);
        }
        Err((id, why)) => {
            let _off = loader.fuse_off(&COPY_FUSE, id.id(), why);
        }
    }
}

/// Links the fog's `destroy` and its `reveal`, both or neither; true when
/// both linked.
fn link_at(loader: &Loader, destroy_at: Target<'_>, reveal_at: Target<'_>) -> bool {
    // The destroy goes first: a copy must never outlive its fog.
    loader.link_feature(&FUSE, destroy_at, &DESTROY_NEXT, destroy, HookFlags::NONE)
        && loader.link_feature(&FUSE, reveal_at, &REVEAL_NEXT, reveal, HookFlags::NONE)
}

crate::engine::chains::chain_detour! {
    /// The game's fog destroy at the table's `map.fog_destroy`, which the
    /// client's world teardown alone calls: drops the copy of the fog
    /// first.
    fn destroy(allocator: *mut u8, fog: *mut u8) -> usize;
    next DESTROY_NEXT;
    fuse &FUSE, "the destroy";
    |onward| {
        if !fog.is_null() {
            forget(fog.expose_provenance());
        }
        onward.call()
    }
}

/// Drops the copy of the fog at `fog`, which the game frees now, and the
/// circles waiting to uncover, which belonged to its world.
#[cold]
fn forget(fog: usize) {
    let mirror = STATE.current.mirror.relock();
    if mirror.as_ref().is_some_and(|m| m.fog == fog) {
        STATE.current.forget(mirror);
    } else {
        drop(mirror);
        STATE.current.uncovers.forget();
    }
}

/// Drops the copy and the circles waiting to uncover when a hosted game
/// ends, whether or not the free of its fog was seen.
pub(crate) fn forget_world() {
    STATE.current.forget(STATE.current.mirror.relock());
}

impl Uncovers {
    /// Makes every field new; the pattern names each, so a field added later
    /// fails to build until it is reset here too.
    fn forget(&self) {
        let Self {
            waiting,
            warned,
            circles,
        } = self;
        circles.relock().clear();
        waiting.store(false, Ordering::Release);
        warned.store(false, Ordering::Release);
    }
}

crate::engine::chains::chain_detour! {
    /// The game's fog reveal at the table's `map.fog_reveal`, `reveal(fog,
    /// &[x, z], range)`: once it returns, copies the tiles it touched and
    /// hands it the circles the server asked to uncover.
    fn reveal(fog: *mut u8, xz: *const [f32; 2], range: f32) -> usize;
    next REVEAL_NEXT;
    fuse &FUSE, "the reveal";
    |onward| {
        let at = (!xz.is_null()).then(|| {
            // SAFETY: the game passes the reveal a non-null (the `then`
            // condition) pointer to the two f32 of the place it uncovers,
            // valid for the call.
            unsafe { enshwick_core::raw::read_at::<[f32; 2]>(xz.cast(), 0) }
        });
        let result = onward.call();
        if !fog.is_null() {
            copy_after(fog, at, range);
            if STATE.current.uncovers.waiting.load(Ordering::Acquire) {
                uncover(fog);
            }
        }
        result
    }
}

/// Hands the game's reveal on `fog`, the fog it reveals on this thread, the
/// circles the server asked for: a call of the loader's own into the rest
/// of the chain, so mods' links hear it as a reveal. A reveal that finds
/// the circles held by the thread that queues one leaves them for the next.
#[cold]
fn uncover(fog: *mut u8) {
    let Some(next) = REVEAL_NEXT.wired() else {
        return;
    };
    let Some(mut circles) = STATE.current.uncovers.circles.try_relock() else {
        return;
    };
    let taken = core::mem::take(&mut *circles);
    STATE
        .current
        .uncovers
        .waiting
        .store(false, Ordering::Release);
    drop(circles);
    for (at, radius) in taken {
        let range = radius * UNCOVER_RANGE_PER_RADIUS;
        // SAFETY: the rest of the chain of the game's reveal, wired since
        // this runs inside its link, on the live fog the game revealed on
        // this same thread a moment ago; `at` lives for the call.
        let _uncovered = unsafe { next(fog, &raw const at, range) };
        copy_after(fog, Some(at), range);
    }
}

/// Copies what a reveal of `range` around `at` changed on `fog` while the
/// copy has its layout, under the copy's own fuse: a panic there turns off
/// the copy alone, and the uncovers run on.
fn copy_after(fog: *mut u8, at: Option<[f32; 2]>, range: f32) {
    if let Some(layout) = LAYOUT.get() {
        let _copied = COPY_FUSE.run("the copy", || {
            copy(layout, fog.expose_provenance(), at, range);
        });
    }
}

/// Queues a circle to uncover on this client, which its server asked for,
/// or the server half of a client that hosts for its own player; the next
/// reveal runs it. False when it was dropped, with a warning once.
pub(crate) fn uncover_here(at: [f32; 2], radius: f32) -> bool {
    let dropped = |why: &str| {
        if !STATE.current.uncovers.warned.swap(true, Ordering::AcqRel) {
            crate::log!(
                WARN,
                "fog of war uncovers dropped until the fog goes: {why}"
            );
        }
    };
    if !STATE.reveal_linked.load(Ordering::Acquire) || FUSE.is_off() {
        dropped("the fog of war is off on this client");
        return false;
    }
    let mut circles = STATE.current.uncovers.circles.relock();
    if circles.len() >= MAX_UNCOVERS {
        drop(circles);
        dropped(&format!("{MAX_UNCOVERS} wait already"));
        return false;
    }
    circles.push((at, radius));
    STATE
        .current
        .uncovers
        .waiting
        .store(true, Ordering::Release);
    true
}

/// Why a circle of a player's fog was not sent to uncover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum UncoverError {
    /// No server runs here.
    #[error("no server runs here")]
    NotServer,
    /// A circle the message cannot carry.
    #[error("the circle is out of range")]
    BadCircle,
    /// The player's client has no ready link.
    #[error("the player's link is not ready")]
    NotReady,
    /// The message was not sent.
    #[error("the message was not sent")]
    NotSent,
}

/// Sends the client of the player with account key `key` a circle of its
/// fog of war to uncover, for `HostApi::fog_uncover`; on a client that
/// hosts, its own player's circle is queued here, as no message reaches
/// the game the loader runs in.
///
/// # Errors
///
/// [`UncoverError`] off a server, for a circle out of range, a link not
/// ready, or a message not sent or circle not queued.
pub(crate) fn uncover_for(
    loader: &Loader,
    key: u64,
    at: [f32; 2],
    radius: f32,
) -> Result<(), UncoverError> {
    uncover_as(loader, key, crate::base::carrier::is_host(key), at, radius)
}

/// [`uncover_for`] for the player `key`, who is the hosting client's own
/// player when `own`.
fn uncover_as(
    loader: &Loader,
    key: u64,
    own: bool,
    at: [f32; 2],
    radius: f32,
) -> Result<(), UncoverError> {
    if loader.side != Side::SERVER && !crate::engine::world::hosting() {
        return Err(UncoverError::NotServer);
    }
    if !crate::base::remote::Message::uncover_fits(at, radius) {
        return Err(UncoverError::BadCircle);
    }
    if own {
        return uncover_here(at, radius)
            .then_some(())
            .ok_or(UncoverError::NotSent);
    }
    if !crate::base::net::ready(key) {
        return Err(UncoverError::NotReady);
    }
    let message = crate::base::remote::Message::FogUncover { at, radius };
    if crate::base::net::to_player(key, &message) {
        Ok(())
    } else {
        Err(UncoverError::NotSent)
    }
}

/// The shape of the fog object at `fog`, `None` when it reads as none the
/// game holds.
fn shape(layout: &Layout, fog: usize) -> Option<FogShape> {
    // SAFETY: `fog` is the live fog object the reveal just wrote, on the
    // thread that alone writes it; the table's offsets of the tile counts
    // (two u32, across then down) and the z size (f32) lie inside it on
    // client build 1273043.
    let [wide, high] = unsafe { crate::engine::memory::read::<[u32; 2]>(fog, layout.tiles_wide) };
    // SAFETY: `fog` is the live fog object the reveal just wrote, on the
    // thread that alone writes it; the table's offset of its z size (f32)
    // lies inside it on client build 1273043.
    let size_z = unsafe { crate::engine::memory::read::<f32>(fog, layout.size_z) };
    FogShape::new(wide, high, layout.edge, size_z)
        .filter(|shape| shape.edge() as usize * shape.edge() as usize == layout.tile_bytes)
}

/// Copies the tiles a reveal at `at` with `range` touched, those earlier
/// reveals skipped, or the whole fog when the copy does not hold it yet.
fn copy(layout: &Layout, fog: usize, at: Option<[f32; 2]>, range: f32) {
    let Some(shape) = shape(layout, fog) else {
        return;
    };
    let revealed = at.and_then(|xz| shape.revealed(xz, range));
    let Some(mut mirror) = STATE.current.mirror.try_relock() else {
        if let Some(rect) = revealed {
            skip(&rect);
        }
        return;
    };
    let held = mirror.as_ref().map(|m| (m.fog, m.image.shape()));
    let skipped = TileRect::unpack(STATE.current.skipped.swap(0, Ordering::AcqRel));
    let planned = plan(held, fog, shape, revealed, skipped);
    let full = matches!(planned, Plan::Full);
    let rect = match planned {
        Plan::Nothing => return,
        Plan::Tiles(rect) => rect,
        Plan::Full => {
            *mirror = Some(Mirror {
                fog,
                session: STATE.sessions.fetch_add(1, Ordering::AcqRel) + 1,
                image: FogImage::new(shape),
                started: std::time::Instant::now(),
            });
            let [wide, high] = shape.tiles();
            TileRect {
                x: 0..wide,
                y: 0..high,
            }
        }
    };
    let Some(mirror) = mirror.as_mut() else {
        return;
    };
    // SAFETY: `fog` is the live fog object the reveal just wrote, on the
    // thread that alone writes it, and `shape` read a fog from it; the
    // table's offset of its tile block pointer lies inside it on client
    // build 1273043.
    let tiles = unsafe { crate::engine::memory::read_ptr::<u8>(fog, layout.tiles) };
    if tiles.is_null() {
        return;
    }
    let wide = shape.tiles()[0] as usize;
    let mut buffer = [0u8; MAX_TILE_BYTES];
    let tile = &mut buffer[..layout.tile_bytes];
    for ty in rect.y {
        for tx in rect.x.clone() {
            let start = (ty as usize * wide + tx as usize) * layout.tile_bytes;
            // SAFETY: `tiles` is the fog's non-null (checked earlier) block
            // of `wide` × `high` tiles of `tile_bytes` each, which `shape`
            // read from the same object; `plan` clips its tiles to those
            // counts, so `start` plus a tile stays inside the block. Only
            // this thread writes it.
            unsafe { crate::engine::memory::read_bytes(tiles.expose_provenance(), start, tile) };
            mirror.image.put_tile(tx, ty, tile);
        }
    }
    if full {
        // A fog the client loaded shows what the save explored: none of it
        // was uncovered now.
        mirror.image.settle();
    }
}

/// Notes tiles `rect` for the next reveal, which a mod's read kept this
/// one from copying.
#[cold]
fn skip(rect: &TileRect) {
    let mut held = STATE.current.skipped.load(Ordering::Acquire);
    loop {
        let merged = TileRect::unpack(held).map_or_else(|| rect.clone(), |old| old.union(rect));
        match STATE.current.skipped.compare_exchange_weak(
            held,
            merged.pack(),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return,
            Err(now) => held = now,
        }
    }
}

/// Why the fog's info was not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum FogReadError {
    /// The mod did not open the fog.
    #[error("the mod did not open the fog")]
    Unopened,
    /// The fog or its copy is off, or no copy is made yet.
    #[error("no copy of the fog runs")]
    NoCopy,
}

/// Writes the fog's info to `info` and the box at `min` of `size` pixels
/// into `out`, for mod `owner`: the box's own read error once the info is
/// written.
///
/// # Errors
///
/// [`FogReadError`] when nothing was written.
pub(crate) fn read(
    owner: ModIndex,
    info: &mut FogInfo,
    min: [u32; 2],
    size: [u32; 2],
    out: &mut [u8],
) -> Result<Result<(), ReadError>, FogReadError> {
    if !opened(owner) {
        return Err(FogReadError::Unopened);
    }
    read_opened(info, min, size, out)
}

/// [`read`] for a mod that opened the fog.
fn read_opened(
    info: &mut FogInfo,
    min: [u32; 2],
    size: [u32; 2],
    out: &mut [u8],
) -> Result<Result<(), ReadError>, FogReadError> {
    // A copy turned off mid-run may hold a stale image.
    if FUSE.is_off() || COPY_FUSE.is_off() {
        return Err(FogReadError::NoCopy);
    }
    let mut mirror = STATE.current.mirror.relock();
    let Some(mirror) = mirror.as_mut() else {
        return Err(FogReadError::NoCopy);
    };
    let window = mirror.uncovered();
    let shape = mirror.image.shape();
    let [width, height] = shape.size();
    *info = FogInfo {
        header: Header::of::<FogInfo>(),
        width,
        height,
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a z size of an f32 over a pixel count holds in an f32"
        )]
        metres_per_pixel: shape.metres_per_pixel() as f32,
        size_z: shape.size_z(),
        version: mirror.image.version(),
        session: mirror.session,
        uncovered: window.seq,
        uncovered_pixels: window.pixels,
        reserved: 0,
        uncovered_min: window.min,
        uncovered_size: window.size,
    };
    if size[0] == 0 || size[1] == 0 {
        return Ok(Ok(()));
    }
    Ok(mirror.image.read(min, size, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(part: &str) -> Option<String> {
        crate::engine::diag::parts_off()
            .into_iter()
            .find(|p| p.part == part)
            .map(|p| p.reason)
    }

    /// The only test that queues uncovers, which are the process's own.
    #[test]
    fn a_host_s_own_circle_queues_here_and_a_hosted_game_s_end_drops_what_waits() {
        let loader = crate::start::tests::pinning_loader(Side::SERVER, &[], &[]);
        STATE.reveal_linked.store(true, Ordering::Release);
        assert_eq!(uncover_as(&loader, 7, true, [1.0, 2.0], 3.0), Ok(()));
        assert_eq!(
            uncover_as(&loader, 8, false, [1.0, 2.0], 3.0),
            Err(UncoverError::NotReady),
            "a remote player without Enshwick is not reached"
        );
        STATE.reveal_linked.store(false, Ordering::Release);
        assert_eq!(
            uncover_as(&loader, 7, true, [1.0, 2.0], 3.0),
            Err(UncoverError::NotSent),
            "the host's own fog is off"
        );
        assert!(STATE.current.uncovers.warned.load(Ordering::Acquire));
        assert_eq!(STATE.current.uncovers.circles.relock().len(), 1);
        STATE
            .current
            .uncovers
            .waiting
            .store(true, Ordering::Release);
        STATE.current.skipped.store(7, Ordering::Release);
        forget_world();
        assert!(STATE.current.mirror.relock().is_none());
        assert_eq!(STATE.current.skipped.load(Ordering::Acquire), 0);
        assert!(STATE.current.uncovers.circles.relock().is_empty());
        assert!(!STATE.current.uncovers.waiting.load(Ordering::Acquire));
        assert!(
            !STATE.current.uncovers.warned.load(Ordering::Acquire),
            "the next world warns again"
        );
    }

    #[test]
    fn a_tile_is_its_edge_squared_and_fits_the_copy_s_buffer() {
        assert_eq!(tile(6, 12), Some((4096, 64)));
        assert_eq!(tile(5, 10), Some((1024, 32)));
        assert_eq!(tile(0, 0), Some((1, 1)));
        assert_eq!(tile(6, 13), None, "the shifts disagree");
        assert_eq!(tile(6, 11), None, "the shifts disagree");
        assert_eq!(tile(7, 14), None, "past the buffer");
    }

    #[test]
    fn a_fog_whose_reveal_fails_to_link_is_wholly_off_and_its_destroy_never_armed() {
        let loader = crate::start::tests::bare_loader();
        let linked = link_at(
            &loader,
            Target::Found("fog.destroy", Some(0x1000)),
            Target::Found("fog.reveal", None),
        );
        assert!(FUSE.is_off());
        assert!(!linked);
        let mut chains = loader.chains.relock();
        chains.seal();
        assert!(chains.is_empty(), "the destroy went with its feature");
        drop(chains);
        assert!(reason("fog of war").is_some_and(|r| r.starts_with("fog.reveal ")));
    }

    #[test]
    fn a_fog_read_once_the_copy_is_off_answers_missing_while_the_reveal_stays_on() {
        COPY_FUSE.blow("the copy");
        assert!(!FUSE.is_off(), "the reveal and its uncovers run on");
        let mut info: FogInfo = bytemuck::Zeroable::zeroed();
        assert_eq!(
            read_opened(&mut info, [0, 0], [0, 0], &mut []),
            Err(FogReadError::NoCopy)
        );
    }

    #[test]
    fn a_fog_copy_without_its_layout_is_listed_off_with_the_first_entry_missing() {
        find_layout(&crate::start::tests::bare_loader());
        assert!(COPY_FUSE.is_off());
        assert!(
            reason("fog of war copy").is_some_and(|r| r.starts_with("map.fog_tile_edge_shift ")),
            "listed for the Mods screen, not only logged"
        );
    }
}
