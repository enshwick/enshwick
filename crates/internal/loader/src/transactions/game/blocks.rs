//! Building blocks in the game: resolving a block, writing it and announcing it.

use enshwick::abi::{BlockPlace, Status};
use enshwick_gamedata::catalogue;

use crate::host::Loader;
use crate::memory::read;
use crate::pinned;
use crate::transactions::block::{Block, EventLayout, GameBox};
use crate::transactions::paste::Region;

/// The game's lookup of a shape's `VoxelBlueprintItem` by its item in a
/// blueprint registry; null when the item has none (server 399, client
/// 426).
type LookupFn = unsafe extern "system" fn(registry: *mut u8, item: u32) -> *const u8;

/// The game's lookup of the feedback (sound and effects) of a voxel
/// material in the terraforming registry (server 399, client 426).
type FeedbackFn = unsafe extern "system" fn(registry: *mut u8, material: u8) -> u32;

/// Where and how a block sits, as the game's block builder reads it: the
/// position in 32.32 fixed-point metres, then the rotation quaternion x, y,
/// z, w (server 399, client 426).
#[repr(C)]
struct PosRot {
    position: [i64; 3],
    rotation: [f32; 4],
}

const _: () = assert!(core::mem::offset_of!(PosRot, rotation) == 0x18);

/// The game's block builder: fills `out` from the blueprint `item`, the
/// place `at` and `extra`, which the game passes as 0; it reads only its
/// arguments (server 399, client 426).
type BlockBoxFn =
    unsafe extern "system" fn(out: *mut GameBox, item: *const u8, at: *const PosRot, extra: u64);

/// One byte for each voxel material the block may write over, as the
/// game's block write reads it.
#[repr(C)]
pub(super) struct Table {
    entries: *const u8,
    count: u64,
}

/// The game's block write: writes the block's voxels in `material` over
/// those `replace` allows, through the same core as every edit; `undo`
/// null keeps no undo, `snap` 0 adds no blend, `flag` is 0 as the game
/// passes it (server 399, client 426).
pub(super) type WriteBlockFn = unsafe extern "system" fn(
    editor: *mut u8,
    undo: *mut u8,
    alloc: *mut u8,
    out: *mut u32,
    replace: *const Table,
    block: *const GameBox,
    material: u8,
    snap: u8,
    flag: u8,
);

/// The voxel materials a byte names.
const MATERIALS: usize = 0x100;
/// The first building voxel material.
const BUILDING_FIRST: u8 = 0x80;
/// Metres to the game's 32.32 fixed point.
const FIXED_ONE: f64 = 4_294_967_296.0;

fn function(loader: &Loader, id: pinned::Symbol) -> Result<usize, Status> {
    loader
        .address(id)
        .and_then(|address| usize::try_from(address).ok())
        .ok_or(Status::MISSING)
}

/// The block `request` asks for, resolved: its blueprint, its building
/// material and the box the game builds for it. [`Status::BAD_ARGS`] for a
/// position or rotation out of range, a shape without a blueprint or a
/// material item that places no building material; [`Status::MISSING`]
/// while no world runs or on a build without the pieces. Any thread: it
/// reads only what the world sets at load (the item and blueprint
/// registries) and calls the game's pure builder.
pub(crate) fn resolve(loader: &Loader, request: &BlockPlace) -> Result<Block, Status> {
    let position_ok = request.position.iter().all(|axis| {
        axis.is_finite() && *axis >= 0.0 && *axis < f64::from(enshwick::abi::Voxel::END)
    });
    let norm: f32 = request.rotation.iter().map(|v| v * v).sum();
    if !position_ok || !norm.is_finite() || norm < 1e-6 {
        return Err(Status::BAD_ARGS);
    }
    let material = building_material(loader, request.material)?;
    let lookup = function(loader, pinned::building::BLUEPRINT_LOOKUP)?;
    let builder = function(loader, pinned::building::BLOCK_BOX)?;
    let holder_at = loader
        .offset(pinned::server::PLAYER_INFOS)
        .ok_or(Status::MISSING)?;
    let registry_at = loader
        .offset(pinned::server::BLUEPRINT_REGISTRY)
        .ok_or(Status::MISSING)?;
    let server = crate::world::SERVER.load(core::sync::atomic::Ordering::Acquire);
    if server == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `server` is the running server the post-step link captured,
    // non-zero (checked above); its game-systems holder, set at load and
    // kept for the world's life, sits at the table's `server.player_infos`.
    let holder: usize = unsafe { read(server, holder_at) };
    if holder == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `holder` is the non-zero game-systems holder (checked above);
    // its voxel blueprint registry, set at load, sits at the table's
    // `server.blueprint_registry`.
    let registry: usize = unsafe { read(holder, registry_at) };
    if registry == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the table's `building.blueprint_lookup` is the address of the
    // game function with the `LookupFn` signature on this build, in the
    // image, which stays mapped.
    let lookup = unsafe { core::mem::transmute::<usize, LookupFn>(lookup) };
    // SAFETY: `lookup` is the game's read-only lookup in the non-zero
    // blueprint registry (checked above), which the world keeps unchanged
    // after load.
    let item = unsafe {
        lookup(
            core::ptr::with_exposed_provenance_mut(registry),
            request.shape,
        )
    };
    if item.is_null() {
        return Err(Status::BAD_ARGS);
    }
    let at = PosRot {
        position: request.position.map(fixed),
        rotation: request.rotation,
    };
    let mut game = GameBox([0; GameBox::SIZE]);
    // SAFETY: the table's `building.block_box` is the address of the game
    // function with the `BlockBoxFn` signature on this build, in the
    // image, which stays mapped.
    let builder = unsafe { core::mem::transmute::<usize, BlockBoxFn>(builder) };
    // SAFETY: `builder` reads only its arguments: `item` is the non-null
    // `VoxelBlueprintItem` the lookup returned (checked above), `at` a
    // `PosRot` on this stack, and it writes the `GameBox::SIZE` bytes of
    // `game`, aligned to 8 as the game's box is.
    unsafe { builder(&raw mut game, item, &raw const at, 0) };
    let region = game.region()?;
    Ok(Block {
        shape: request.shape,
        feedback: feedback(loader, holder, material),
        material,
        position: request.position,
        rotation: request.rotation,
        region,
        game,
    })
}

/// The feedback the game gives `material`, as its place op looks it up;
/// 0, none, on a build without the lookup or before the registry loads.
fn feedback(loader: &Loader, holder: usize, material: u8) -> u32 {
    let Ok(function) = function(loader, pinned::building::MATERIAL_FEEDBACK) else {
        return 0;
    };
    let Some(registry_at) = loader.offset(pinned::terrain::REGISTRY) else {
        return 0;
    };
    // SAFETY: `holder` is the non-zero game-systems holder `resolve` read;
    // its terraforming registry, set at load, sits at the table's
    // `terrain.registry`.
    let registry: usize = unsafe { read(holder, registry_at) };
    if registry == 0 {
        return 0;
    }
    // SAFETY: the table's `building.material_feedback` is the address of the
    // game function with the `FeedbackFn` signature on this build, in the
    // image, which stays mapped.
    let lookup = unsafe { core::mem::transmute::<usize, FeedbackFn>(function) };
    // SAFETY: `lookup` reads the non-zero terraforming registry (checked
    // above), which the world keeps unchanged after load.
    unsafe { lookup(core::ptr::with_exposed_provenance_mut(registry), material) }
}

/// A position in metres as the game's 32.32 fixed point; the caller keeps
/// it at 0 or more and below [`enshwick::abi::Voxel::END`].
#[expect(
    clippy::cast_possible_truncation,
    reason = "a position below 2^30 metres fits 32.32 fixed point in an i64"
)]
fn fixed(metres: f64) -> i64 {
    (metres * FIXED_ONE) as i64
}

/// The building voxel material `item` places; [`Status::BAD_ARGS`] for an
/// item that places none.
fn building_material(loader: &Loader, item: u32) -> Result<u8, Status> {
    let info = crate::inventory::item_info_at(loader, item).map_err(|_| Status::BAD_ARGS)?;
    let at = |id| {
        catalogue::constant(id, &loader.reflection)
            .and_then(|at| usize::try_from(at).ok())
            .ok_or(Status::MISSING)
    };
    let flag_at = at(catalogue::ITEM_BUILDING_VOXEL)?;
    let id_at = at(catalogue::ITEM_PLACE_VOXEL_MATERIAL_ID)?;
    // SAFETY: `info` is the item's live `ItemInfo` from the game's lookup,
    // with its `isBuildingVoxel` byte at the reflected path's offset.
    let building: u8 = unsafe { enshwick::raw::read_at(info, flag_at) };
    // SAFETY: `info` is the item's live `ItemInfo` from the game's lookup,
    // with its `placeVoxelMaterialId` byte at the reflected path's offset.
    let material: u8 = unsafe { enshwick::raw::read_at(info, id_at) };
    if building == 0 || material < BUILDING_FIRST {
        return Err(Status::BAD_ARGS);
    }
    Ok(material)
}

/// Writes `block` through the game's block write, over any material.
///
/// # Safety
///
/// - `write` is the table's `voxel.write_block`.
/// - `editor` and `alloc` are the server's live voxel editor and its
///   allocator, on the game thread at a quiet moment.
pub(super) unsafe fn write(write: WriteBlockFn, editor: *mut u8, alloc: *mut u8, block: &Block) {
    let anything = [1u8; MATERIALS];
    let replace = Table {
        entries: anything.as_ptr(),
        count: MATERIALS as u64,
    };
    let mut out = 0u32;
    // SAFETY: the caller guarantees `write`, `editor` and `alloc` on the game
    // thread at a quiet moment; `replace` points at `MATERIALS` entries on
    // this stack, `block.game` is the box the game's own builder made for
    // this block, whose data points into the blueprint registry the world
    // keeps until it ends, which ends the transaction first; no undo.
    unsafe {
        write(
            editor,
            core::ptr::null_mut(),
            alloc,
            &raw mut out,
            &raw const replace,
            &raw const block.game,
            block.material,
            0,
            0,
        );
    };
}

/// The game's place and tear-down events: each type's hash and layout.
#[derive(Clone, Copy, Debug)]
pub(super) struct Events {
    place: Option<(u32, EventLayout)>,
    tear_down: Option<(u32, EventLayout)>,
}

impl Events {
    pub(super) fn new(loader: &Loader) -> Self {
        Self {
            place: layout(
                loader,
                catalogue::BUILDING_PLACE_EVENT,
                &catalogue::BUILDING_PLACE_EVENT_FIELDS,
            ),
            tear_down: layout(
                loader,
                catalogue::BUILDING_TEAR_DOWN_EVENT,
                &catalogue::BUILDING_TEAR_DOWN_EVENT_FIELDS,
            ),
        }
    }

    /// Raises the game's `BuildingPlaceEvent` for `block`, when this build
    /// has it.
    pub(super) fn placed(self, loader: &Loader, block: &Block, actor: u32) {
        if let Some((hash, layout)) = self.place
            && let Some(bytes) = layout.place(block, actor)
        {
            let _raised = crate::emits::push(loader, hash, &bytes);
        }
    }

    /// Raises the game's `BuildingTearDownEvent` for the building voxels
    /// of `region`, when this build has it.
    pub(super) fn torn_down(self, loader: &Loader, region: Region, actor: u32) {
        if let Some((hash, layout)) = self.tear_down
            && let Some(bytes) = layout.tear_down(region, actor)
        {
            let _raised = crate::emits::push(loader, hash, &bytes);
        }
    }
}

/// The layout of the event type `id` from reflection, its fields named by
/// `fields`: position, orientation, lowest and highest corner, material,
/// then the tracking item when there are seven, then the owner.
fn layout(loader: &Loader, id: &str, fields: &[&str]) -> Option<(u32, EventLayout)> {
    let hash = catalogue::type_hash(id)?;
    let size = loader
        .reflection
        .type_info(catalogue::type_name(id))
        .ok()
        .and_then(|info| usize::try_from(info.size()).ok())?;
    let at = |field: &&str| {
        catalogue::constant(field, &loader.reflection).and_then(|at| usize::try_from(at).ok())
    };
    let offsets: Vec<usize> = fields.iter().map(at).collect::<Option<_>>()?;
    let (item, owner) = match offsets.as_slice() {
        [.., item, owner] if offsets.len() == 7 => (Some(*item), *owner),
        [.., owner] => (None, *owner),
        [] => return None,
    };
    Some((
        hash,
        EventLayout {
            size,
            position: *offsets.first()?,
            orientation: *offsets.get(1)?,
            min: *offsets.get(2)?,
            max: *offsets.get(3)?,
            material: *offsets.get(4)?,
            item,
            owner,
        },
    ))
}
