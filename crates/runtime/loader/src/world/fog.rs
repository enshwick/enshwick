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

use enshwick_core::abi::{FogInfo, Header, HookFlags, Side, Status};

use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::pinned::Constant;
use crate::world::fog_image::{
    FogImage, FogShape, Plan, ReadError, TileRect, UNCOVER_RANGE_PER_RADIUS, plan,
};
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

/// The switch of the copy alone, off on a build without its layout while
/// the uncovers run on.
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
/// The mods that opened the fog.
static OPENERS: Mutex<Vec<ModIndex>> = Mutex::new(Vec::new());
static WANTED: AtomicBool = AtomicBool::new(false);
/// The copy, with the fog object it copies.
static MIRROR: Mutex<Option<Mirror>> = Mutex::new(None);
/// The tiles reveals skipped while a mod held the copy, as
/// [`TileRect::pack`] writes them; 0 for none.
static SKIPPED: AtomicU64 = AtomicU64::new(0);
/// Fogs seen so far.
static SESSIONS: AtomicU64 = AtomicU64::new(0);

/// The circles the server asked this client to uncover, each a centre and a
/// radius in metres, which the next reveal hands the game.
struct Uncovers {
    /// Whether the reveal is linked, so an uncover has a reveal to run in.
    linked: AtomicBool,
    /// Whether `circles` holds any, read first on every reveal.
    waiting: AtomicBool,
    /// Whether an uncover the client dropped was logged since the last fog
    /// went, so a server that keeps asking fills no log.
    warned: AtomicBool,
    circles: Mutex<Vec<([f32; 2], f32)>>,
}

static UNCOVERS: Uncovers = Uncovers {
    linked: AtomicBool::new(false),
    waiting: AtomicBool::new(false),
    warned: AtomicBool::new(false),
    circles: Mutex::new(Vec::new()),
};

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
pub(crate) fn open(loader: &Loader, owner: ModIndex) -> Status {
    if loader.delivery.get().is_some() {
        return Status::TOO_LATE;
    }
    if loader.side != Side::CLIENT {
        return Status::MISSING;
    }
    let mut openers = OPENERS.relock();
    if !openers.contains(&owner) {
        openers.push(owner);
    }
    WANTED.store(true, Ordering::Release);
    Status::OK
}

/// Whether mod `owner` opened the fog.
pub(crate) fn opened(owner: ModIndex) -> bool {
    OPENERS.relock().contains(&owner)
}

/// The fog's layout, or the entry of the first that is not in the build
/// or does not fit the copy.
fn layout(loader: &Loader) -> Result<Layout, Constant> {
    let offset = |id: Constant| loader.offset(id).ok_or(id);
    let shift = |id: Constant| {
        offset(id)
            .ok()
            .and_then(|shift| u32::try_from(shift).ok())
            .filter(|&shift| shift < 16)
            .ok_or(id)
    };
    let edge_shift = shift(pinned::map::FOG_TILE_EDGE_SHIFT)?;
    let tile_shift = shift(pinned::map::FOG_TILE_SHIFT)?;
    // A tile is `edge` × `edge` bytes: the two shifts must agree.
    if tile_shift != edge_shift * 2 || 1 << tile_shift > MAX_TILE_BYTES {
        return Err(pinned::map::FOG_TILE_SHIFT);
    }
    Ok(Layout {
        size_z: offset(pinned::map::FOG_SIZE_Z)?,
        tiles: offset(pinned::map::FOG_TILES)?,
        tiles_wide: offset(pinned::map::FOG_TILES_WIDE)?,
        tile_bytes: 1 << tile_shift,
        edge: 1 << edge_shift,
    })
}

/// Links the reveal on every client, for the uncovers its server asks for,
/// and copies the fog there when a mod opened it.
pub(crate) fn link(loader: &'static Loader) {
    if loader.side != Side::CLIENT {
        return;
    }
    if WANTED.load(Ordering::Acquire) {
        match layout(loader) {
            Ok(found) => {
                let _first = LAYOUT.set(found);
            }
            Err(id) => {
                let reason = if loader.offset(id).is_none() {
                    crate::engine::builds::ABSENT
                } else {
                    "does not fit the copy's tiles"
                };
                let _off = loader.fuse_off(&COPY_FUSE, id.id(), reason);
            }
        }
    }
    // The destroy goes first: a copy must never outlive its fog.
    let linked = loader.link_fused(
        &FUSE,
        Target::Pinned(pinned::map::FOG_DESTROY),
        &DESTROY_NEXT,
        destroy,
        HookFlags::NONE,
    ) && loader.link_fused(
        &FUSE,
        Target::Pinned(pinned::map::FOG_REVEAL),
        &REVEAL_NEXT,
        reveal,
        HookFlags::NONE,
    );
    UNCOVERS.linked.store(linked, Ordering::Release);
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
    let mut mirror = MIRROR.relock();
    if mirror.as_ref().is_some_and(|m| m.fog == fog) {
        *mirror = None;
        SKIPPED.store(0, Ordering::Release);
    }
    drop(mirror);
    UNCOVERS.circles.relock().clear();
    UNCOVERS.waiting.store(false, Ordering::Release);
    UNCOVERS.warned.store(false, Ordering::Release);
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
            if let Some(layout) = LAYOUT.get() {
                copy(layout, fog.expose_provenance(), at, range);
            }
            if UNCOVERS.waiting.load(Ordering::Acquire) {
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
    let Some(mut circles) = UNCOVERS.circles.try_relock() else {
        return;
    };
    let taken = core::mem::take(&mut *circles);
    UNCOVERS.waiting.store(false, Ordering::Release);
    drop(circles);
    for (at, radius) in taken {
        let range = radius * UNCOVER_RANGE_PER_RADIUS;
        // SAFETY: the rest of the chain of the game's reveal, wired since
        // this runs inside its link, on the live fog the game revealed on
        // this same thread a moment ago; `at` lives for the call.
        let _uncovered = unsafe { next(fog, &raw const at, range) };
        if let Some(layout) = LAYOUT.get() {
            copy(layout, fog.expose_provenance(), Some(at), range);
        }
    }
}

/// Queues a circle the server asked this client to uncover, on the thread
/// that reads the server's messages; the next reveal runs it.
pub(crate) fn uncover_here(at: [f32; 2], radius: f32) {
    let dropped = |why: &str| {
        if !UNCOVERS.warned.swap(true, Ordering::AcqRel) {
            crate::log!(
                WARN,
                "fog of war uncovers from the server dropped until the fog goes: {why}"
            );
        }
    };
    if !UNCOVERS.linked.load(Ordering::Acquire) || FUSE.is_off() {
        dropped("the fog of war is off on this client");
        return;
    }
    let mut circles = UNCOVERS.circles.relock();
    if circles.len() >= MAX_UNCOVERS {
        drop(circles);
        dropped(&format!("{MAX_UNCOVERS} wait already"));
        return;
    }
    circles.push((at, radius));
    UNCOVERS.waiting.store(true, Ordering::Release);
}

/// Sends the client of the player with account key `key` a circle of its
/// fog of war to uncover, for `HostApi::fog_uncover`.
pub(crate) fn uncover_for(loader: &Loader, key: u64, at: [f32; 2], radius: f32) -> Status {
    if loader.side != Side::SERVER && !crate::engine::world::hosting() {
        return Status::UNAVAILABLE;
    }
    if !crate::base::remote::Message::uncover_fits(at, radius) {
        return Status::BAD_ARGS;
    }
    if !crate::base::net::ready(key) {
        return Status::UNAVAILABLE;
    }
    let message = crate::base::remote::Message::FogUncover { at, radius };
    if crate::base::net::to_player(key, &message) {
        Status::OK
    } else {
        Status::REFUSED
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
    let Some(mut mirror) = MIRROR.try_relock() else {
        if let Some(rect) = revealed {
            skip(&rect);
        }
        return;
    };
    let held = mirror.as_ref().map(|m| (m.fog, m.image.shape()));
    let skipped = TileRect::unpack(SKIPPED.swap(0, Ordering::AcqRel));
    let planned = plan(held, fog, shape, revealed, skipped);
    let full = matches!(planned, Plan::Full);
    let rect = match planned {
        Plan::Nothing => return,
        Plan::Tiles(rect) => rect,
        Plan::Full => {
            *mirror = Some(Mirror {
                fog,
                session: SESSIONS.fetch_add(1, Ordering::AcqRel) + 1,
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
    let mut held = SKIPPED.load(Ordering::Acquire);
    loop {
        let merged = TileRect::unpack(held).map_or_else(|| rect.clone(), |old| old.union(rect));
        match SKIPPED.compare_exchange_weak(
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

/// Writes the fog's info to `info` and the box at `min` of `size` pixels
/// into `out`, for mod `owner`.
pub(crate) fn read(
    owner: ModIndex,
    info: &mut FogInfo,
    min: [u32; 2],
    size: [u32; 2],
    out: &mut [u8],
) -> Status {
    if !opened(owner) {
        return Status::REFUSED;
    }
    if FUSE.is_off() {
        return Status::MISSING;
    }
    let mut mirror = MIRROR.relock();
    let Some(mirror) = mirror.as_mut() else {
        return Status::MISSING;
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
        return Status::OK;
    }
    match mirror.image.read(min, size, out) {
        Ok(()) => Status::OK,
        Err(ReadError::Outside) => Status::OUTSIDE_WORLD,
        Err(ReadError::Short) => Status::BAD_ARGS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fog_whose_reveal_fails_to_link_is_wholly_off_and_its_destroy_never_armed() {
        let mut loader = crate::start::tests::bare_loader();
        loader.side = Side::CLIENT;
        loader.build = Box::leak(Box::new(crate::engine::builds::with_rows(
            "[symbols]\n\"map.fog_destroy\" = 0x1000\n[constants]\n\"map.fog_destroy.bytes\" = 1",
        )));
        let loader: &'static Loader = Box::leak(Box::new(loader));
        assert_eq!(open(loader, ModIndex::new(0)), Status::OK);
        link(loader);
        assert!(FUSE.is_off() && COPY_FUSE.is_off());
        assert!(!UNCOVERS.linked.load(Ordering::Acquire));
        let mut chains = loader.chains.relock();
        chains.seal();
        assert!(chains.is_empty(), "the destroy went with its feature");
        drop(chains);
        let listed = crate::engine::diag::parts_off();
        let reason = |part: &str| {
            listed
                .iter()
                .find(|p| p.part == part)
                .map(|p| p.reason.clone())
        };
        assert!(
            reason("fog of war").is_some_and(|r| r.starts_with("map.fog_reveal ")),
            "{listed:?}"
        );
        assert!(
            reason("fog of war copy").is_some_and(|r| r.starts_with("map.fog_tile_edge_shift ")),
            "listed for the Mods screen, not only logged: {listed:?}"
        );
    }
}
