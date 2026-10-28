//! Building blocks in the game: resolving a block, writing it and announcing it.

use enshwick::abi::{BlockPlace, Status};
use enshwick_gamedata::catalogue;

use crate::host::Loader;
use crate::memory::read;
use crate::pinned;
use crate::transactions::block::{Block, EventLayout, GameBox, MATERIALS, replace_table};
use crate::transactions::paste::Region;
use crate::transactions::props::Prop;
use crate::world::ServerAccess;

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

/// The block `request` asks for, checked: its blueprint, its building
/// material, its feedback and the voxels of the box the game builds for it.
/// [`Status::BAD_ARGS`] for a position or rotation out of range, a shape
/// without a blueprint or a material item that places no building
/// material; [`Status::MISSING`] while no world runs or on a build without
/// the pieces. Any thread: under an access to the server, which keeps its
/// teardown waiting, it reads only what the world sets at load (the item
/// and blueprint registries) and calls the game's pure builder; the box
/// itself is built again at the step, since a world that ends between the
/// two frees what it points into.
pub(crate) fn resolve(loader: &Loader, request: &BlockPlace) -> Result<Block, Status> {
    let position_ok = request.position.iter().all(|axis| {
        axis.is_finite() && *axis >= 0.0 && *axis < f64::from(enshwick::abi::Voxel::END)
    });
    let norm: f32 = request.rotation.iter().map(|v| v * v).sum();
    if !position_ok || !norm.is_finite() || norm < 1e-6 {
        return Err(Status::BAD_ARGS);
    }
    let access = crate::world::access(&"block transaction commit").ok_or(Status::MISSING)?;
    let material = building_material(loader, &access, request.material.get())?;
    let (game, holder) = game_box(
        loader,
        &access,
        request.shape.get(),
        request.position,
        request.rotation,
    )?;
    Ok(Block {
        shape: request.shape.get(),
        feedback: feedback(loader, &access, holder, material),
        material,
        position: request.position,
        rotation: request.rotation,
        region: game.region()?,
    })
}

/// The game's box for `block`, built now from the live blueprint registry;
/// [`Status::MISSING`] when it no longer gives the voxels the commit
/// checked, as after the world changed.
pub(super) fn rebuild(loader: &Loader, block: &Block) -> Result<GameBox, Status> {
    let access = crate::world::access(&"block rebuild").ok_or(Status::MISSING)?;
    let (game, _) = game_box(loader, &access, block.shape, block.position, block.rotation)?;
    if game.region()? != block.region {
        return Err(Status::MISSING);
    }
    Ok(game)
}

/// The game's box for the block of `shape` at `position` turned by
/// `rotation`, and the game-systems holder it was found through; the box
/// points into the blueprint registry, which lives until the world ends,
/// and the holder lives while `access` is held.
fn game_box(
    loader: &Loader,
    access: &ServerAccess<'_>,
    shape: u32,
    position: [f64; 3],
    rotation: [f32; 4],
) -> Result<(GameBox, usize), Status> {
    let lookup = function(loader, pinned::building::BLUEPRINT_LOOKUP)?;
    let builder = function(loader, pinned::building::BLOCK_BOX)?;
    let holder_at = loader
        .offset(pinned::server::PLAYER_INFOS)
        .ok_or(Status::MISSING)?;
    let registry_at = loader
        .offset(pinned::server::BLUEPRINT_REGISTRY)
        .ok_or(Status::MISSING)?;
    // SAFETY: `access` holds the running server, non-zero. The game frees
    // it, with its game-systems holder, only in the teardown on its game
    // thread after set_state(…, 7) (server 399, client 426). At that call
    // `world::tear_down` stops new pins, waits for every pin to drop and
    // then forgets the server, so a game-thread access, which ends before
    // the teardown runs, finds none after it, and one elsewhere is a pin it
    // waited for. The holder sits at the table's `server.player_infos`.
    let holder: usize = unsafe { read(access.server(), holder_at) };
    if holder == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `holder` is the pinned server's non-zero game-systems holder
    // (checked above), which `access` keeps alive; its voxel blueprint
    // registry, set at load, sits at the table's
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
    // after load and `access` keeps alive.
    let item = unsafe { lookup(core::ptr::with_exposed_provenance_mut(registry), shape) };
    if item.is_null() {
        return Err(Status::BAD_ARGS);
    }
    let at = PosRot {
        position: position.map(fixed),
        rotation,
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
    Ok((game, holder))
}

/// The feedback the game gives `material`, as its place op looks it up;
/// 0, none, on a build without the lookup or before the registry loads.
/// `holder` is the game-systems holder of the server `_access` holds, which
/// keeps it alive for the call.
fn feedback(loader: &Loader, _access: &ServerAccess<'_>, holder: usize, material: u8) -> u32 {
    let Ok(function) = function(loader, pinned::building::MATERIAL_FEEDBACK) else {
        return 0;
    };
    let Some(registry_at) = loader.offset(pinned::terrain::REGISTRY) else {
        return 0;
    };
    // SAFETY: `holder` is the non-zero game-systems holder `resolve` read
    // through `_access`, which keeps it alive until it drops; its
    // terraforming registry, set at load, sits at the table's
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
    // above), which the world keeps unchanged after load and `_access` keeps
    // alive.
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
/// item the game does not have or that places none, [`Status::MISSING`]
/// while the loader knows no item registry yet.
fn building_material(loader: &Loader, access: &ServerAccess<'_>, item: u32) -> Result<u8, Status> {
    let info = crate::inventory::item_info_at(loader, access, item)?.ok_or(Status::BAD_ARGS)?;
    let at = |id| {
        catalogue::constant(id, &loader.reflection)
            .and_then(|at| usize::try_from(at).ok())
            .ok_or(Status::MISSING)
    };
    let flag_at = at(catalogue::ids::ITEM_INFO_BUILDING_VOXEL)?;
    let id_at = at(catalogue::ids::ITEM_INFO_PLACE_VOXEL_MATERIAL_ID)?;
    // SAFETY: `info` is the item's `ItemInfo` from the game's lookup, live
    // while `access` is held, with its `isBuildingVoxel` byte at the reflected
    // path's offset.
    let building: u8 = unsafe { enshwick::raw::read_at(info, flag_at) };
    // SAFETY: `info` is the item's `ItemInfo` from the game's lookup, live
    // while `access` is held, with its `placeVoxelMaterialId` byte at the
    // reflected path's offset.
    let material: u8 = unsafe { enshwick::raw::read_at(info, id_at) };
    if building == 0 || material < BUILDING_FIRST {
        return Err(Status::BAD_ARGS);
    }
    Ok(material)
}

/// The materials a building block may write over in the running game, as
/// the game's building tool decides for an ordinary building shape; on the
/// game thread.
pub(super) fn replaceable(loader: &Loader) -> Result<[u8; MATERIALS], Status> {
    let terrain: Vec<(u8, u8)> = crate::terrain::materials(loader)?
        .iter()
        .map(|config| (config.id, config.hardness))
        .collect();
    let building = crate::terrain::building_configs(loader)?;
    Ok(replace_table(&terrain, &building))
}

/// Writes `block` through the game's block write, over the materials
/// `replace` allows.
///
/// # Safety
///
/// - `write` is the table's `voxel.write_block`.
/// - `editor` and `alloc` are the server's live voxel editor and its
///   allocator, on the game thread at a quiet moment.
/// - `game` is `block`'s box from [`rebuild`] in this step.
pub(super) unsafe fn write(
    write: WriteBlockFn,
    editor: *mut u8,
    alloc: *mut u8,
    replace: &[u8; MATERIALS],
    block: &Block,
    game: &GameBox,
) {
    let replace = Table {
        entries: replace.as_ptr(),
        count: MATERIALS as u64,
    };
    let mut out = 0u32;
    // SAFETY: the caller guarantees `write`, `editor` and `alloc` on the game
    // thread at a quiet moment; `replace` points at `MATERIALS` entries
    // borrowed for the call; `game` is the box the game's own builder made
    // for this block in this step, on this thread, from the live blueprint
    // registry its data points into; no undo.
    unsafe {
        write(
            editor,
            core::ptr::null_mut(),
            alloc,
            &raw mut out,
            &raw const replace,
            &raw const *game,
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
    /// The `LogicEntityPlacedEvent`: its hash, size, and where it keeps who
    /// placed it and the prop.
    logic: Option<(u32, usize, usize, usize)>,
}

impl Events {
    pub(super) fn new(loader: &Loader) -> Self {
        Self {
            place: layout(
                loader,
                catalogue::ids::BUILDING_PLACE_EVENT,
                &catalogue::BUILDING_PLACE_EVENT_FIELDS,
            ),
            tear_down: layout(
                loader,
                catalogue::ids::BUILDING_TEAR_DOWN_EVENT,
                &catalogue::BUILDING_TEAR_DOWN_EVENT_FIELDS,
            ),
            logic: logic_layout(loader),
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

    /// Raises the game's events for `prop` placed as `entity`: its
    /// `BuildingPlaceEvent`, and its `LogicEntityPlacedEvent` for a logic
    /// prop, as the game's place-prop op does.
    pub(super) fn prop_placed(self, loader: &Loader, prop: &Prop, entity: u32, actor: u32) {
        if let Some((hash, layout)) = self.place
            && let Some(bytes) = layout.place_prop(prop, actor)
        {
            let _raised = crate::emits::push(loader, hash, &bytes);
        }
        if prop.logic
            && let Some((hash, size, owner_at, entity_at)) = self.logic
            && owner_at + 4 <= size
            && entity_at + 4 <= size
        {
            let mut bytes = vec![0u8; size];
            bytes[owner_at..owner_at + 4].copy_from_slice(&actor.to_le_bytes());
            bytes[entity_at..entity_at + 4].copy_from_slice(&entity.to_le_bytes());
            let _raised = crate::emits::push(loader, hash, &bytes);
        }
    }

    /// Raises the game's `BuildingTearDownEvent` for `prop` removed, as the
    /// game's tear-down does.
    pub(super) fn prop_removed(self, loader: &Loader, prop: &Prop, actor: u32) {
        if let Some((hash, layout)) = self.tear_down
            && let Some(bytes) = layout.remove_prop(prop, actor)
        {
            let _raised = crate::emits::push(loader, hash, &bytes);
        }
    }
}

/// The `LogicEntityPlacedEvent` from reflection: its hash, size, and the
/// offsets of who placed it and of the prop.
fn logic_layout(loader: &Loader) -> Option<(u32, usize, usize, usize)> {
    let id = catalogue::ids::LOGIC_ENTITY_PLACED_EVENT;
    let hash = catalogue::type_hash(id)?;
    let size = loader
        .reflection
        .type_info(catalogue::type_name(id))
        .ok()
        .and_then(|info| usize::try_from(info.size()).ok())?;
    let at = |field: &str| {
        catalogue::constant(field, &loader.reflection).and_then(|at| usize::try_from(at).ok())
    };
    let [owner, entity] = catalogue::LOGIC_ENTITY_PLACED_FIELDS;
    Some((hash, size, at(owner)?, at(entity)?))
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
