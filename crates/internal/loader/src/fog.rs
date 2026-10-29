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

use enshwick::abi::{FogInfo, Header, HookFlags, Side, Status};

use crate::chains::Next;
use crate::fog_image::{FogImage, FogShape, Plan, ReadError, TileRect, plan};
use crate::host::{Loader, Target};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;

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

type RevealFn = unsafe extern "system" fn(fog: *mut u8, xz: *const [f32; 2], range: f32) -> usize;

// SAFETY: `RevealFn` is the signature of the game's fog reveal at the
// table's `map.fog_reveal`, its result passed through whatever it is, and
// the slot is used only as the `next` of the `reveal` link on that function.
static NEXT: Next<RevealFn> = unsafe { Next::new() };

type DestroyFn = unsafe extern "system" fn(allocator: *mut u8, fog: *mut u8) -> usize;

// SAFETY: `DestroyFn` is the signature of the game's fog destroy at the
// table's `map.fog_destroy`, its result passed through whatever it is, and
// the slot is used only as the `next` of the `destroy` link on that
// function.
static DESTROY_NEXT: Next<DestroyFn> = unsafe { Next::new() };

/// The switch of the fog copy.
static FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("fog of war");

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

/// The loader's copy of one fog object.
#[derive(Debug)]
struct Mirror {
    fog: usize,
    session: u64,
    image: FogImage,
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

fn layout(loader: &Loader) -> Option<Layout> {
    let shift = |id| {
        loader
            .offset(id)
            .and_then(|shift| u32::try_from(shift).ok())
            .filter(|&shift| shift < 16)
    };
    let edge_shift = shift(pinned::map::FOG_TILE_EDGE_SHIFT)?;
    let tile_shift = shift(pinned::map::FOG_TILE_SHIFT)?;
    // A tile is `edge` × `edge` bytes: the two shifts must agree.
    (tile_shift == edge_shift * 2 && 1 << tile_shift <= MAX_TILE_BYTES).then_some(())?;
    Some(Layout {
        size_z: loader.offset(pinned::map::FOG_SIZE_Z)?,
        tiles: loader.offset(pinned::map::FOG_TILES)?,
        tiles_wide: loader.offset(pinned::map::FOG_TILES_WIDE)?,
        tile_bytes: 1 << tile_shift,
        edge: 1 << edge_shift,
    })
}

/// Links the reveal on the client when a mod opened the fog.
pub(crate) fn link(loader: &'static Loader) {
    if loader.side != Side::CLIENT || !WANTED.load(Ordering::Acquire) {
        return;
    }
    let Some(found) = layout(loader) else {
        crate::log!(WARN, "fog of war off: its layout is not in this build");
        return;
    };
    let _first = LAYOUT.set(found);
    let checked = [pinned::map::FOG_REVEAL, pinned::map::FOG_DESTROY];
    if let Some(symbol) = checked.iter().find(|s| !loader.code_holds(**s)) {
        crate::log!(
            WARN,
            "fog of war off: the code of {symbol} on {} is not the table's",
            loader.build.name
        );
        return;
    }
    // The destroy goes first: a copy must never outlive its fog.
    if !loader.link_feature(
        "fog of war",
        Target::Pinned(pinned::map::FOG_DESTROY),
        &DESTROY_NEXT,
        destroy,
        HookFlags::NONE,
    ) {
        return;
    }
    loader.link_feature(
        "fog of war",
        Target::Pinned(pinned::map::FOG_REVEAL),
        &NEXT,
        reveal,
        HookFlags::NONE,
    );
}

unsafe extern "system" fn destroy(allocator: *mut u8, fog: *mut u8) -> usize {
    if !fog.is_null() {
        forget(fog.expose_provenance());
    }
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the destroy's signature.
    unsafe { DESTROY_NEXT.get()(allocator, fog) }
}

/// Drops the copy of the fog at `fog`, which the game frees now.
#[cold]
fn forget(fog: usize) {
    let mut mirror = MIRROR.relock();
    if mirror.as_ref().is_some_and(|m| m.fog == fog) {
        *mirror = None;
        SKIPPED.store(0, Ordering::Release);
    }
}

unsafe extern "system" fn reveal(fog: *mut u8, xz: *const [f32; 2], range: f32) -> usize {
    let at = if xz.is_null() {
        None
    } else {
        // SAFETY: the game passes the reveal a non-null (checked earlier)
        // pointer to the two f32 of the place it uncovers, valid for the
        // call.
        Some(unsafe { enshwick::raw::read_at::<[f32; 2]>(xz.cast(), 0) })
    };
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the reveal's signature.
    let result = unsafe { NEXT.get()(fog, xz, range) };
    if let Some(layout) = LAYOUT.get()
        && !fog.is_null()
    {
        let _caught = FUSE.run("the copy", || {
            copy(layout, fog.expose_provenance(), at, range);
        });
    }
    result
}

/// The shape of the fog object at `fog`, `None` when it reads as none the
/// game holds.
fn shape(layout: &Layout, fog: usize) -> Option<FogShape> {
    // SAFETY: `fog` is the live fog object the reveal just wrote, on the
    // thread that alone writes it; the table's offsets of the tile counts
    // (two u32, across then down) and the z size (f32) lie inside it on
    // client build 1273043.
    let [wide, high] = unsafe { crate::memory::read::<[u32; 2]>(fog, layout.tiles_wide) };
    // SAFETY: `fog` is the live fog object the reveal just wrote, on the
    // thread that alone writes it; the table's offset of its z size (f32)
    // lies inside it on client build 1273043.
    let size_z = unsafe { crate::memory::read::<f32>(fog, layout.size_z) };
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
    let rect = match plan(held, fog, shape, revealed, skipped) {
        Plan::Nothing => return,
        Plan::Tiles(rect) => rect,
        Plan::Full => {
            *mirror = Some(Mirror {
                fog,
                session: SESSIONS.fetch_add(1, Ordering::AcqRel) + 1,
                image: FogImage::new(shape),
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
    let tiles = unsafe { crate::memory::read_ptr::<u8>(fog, layout.tiles) };
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
            unsafe { crate::memory::read_bytes(tiles.expose_provenance(), start, tile) };
            mirror.image.put_tile(tx, ty, tile);
        }
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
    let mirror = MIRROR.relock();
    let Some(mirror) = mirror.as_ref() else {
        return Status::MISSING;
    };
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
