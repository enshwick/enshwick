//! Terrain edits in the server's world: the terrain materials, and adding,
//! digging and flattening ground as the game's terraforming does.
//!
//! The world's ground is a voxel store, half a metre to a voxel, each voxel
//! a material (0 air, 1 to 127 terrain, 128 and up building) and a density.
//! The game's three terraforming edits work on the store directly: add a
//! mound of a material, dig a sphere shell by shell against a budget that
//! each material's cost spends, flatten to a plane. They take the store's
//! own lock, mark what changed for the voxel channel that the server sends
//! right after its post-step, and the save writes the store; building
//! voxels are left alone. The loader calls them on the game thread between
//! steps, without the game's undo, so what a player's tool adds around them
//! (permission, cost, loot, experience, feedback) is the mod's to decide.

use std::sync::OnceLock;

use enshwick::abi::{Header, Status, TerrainEdit, TerrainMaterialInfo};

use crate::host::Loader;
use crate::memory::read;
use crate::pinned;
use crate::pinned::Symbol;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::server::PLAYER_INFOS.id(),
    pinned::terrain::ADD.id(),
    pinned::terrain::DIG.id(),
    pinned::terrain::EDITOR.id(),
    pinned::terrain::FLATTEN.id(),
    pinned::voxel::BUILDING_FIRST.id(),
    pinned::voxel::MATERIALS.id(),
    pinned::terrain::REGISTRY.id(),
    pinned::terrain::REGISTRY_RESOURCE.id(),
    pinned::terrain::VOXEL_WORLD.id(),
];

/// The materials the edits work on: terrain, not air nor buildings, which
/// start at the build table's `voxel.building_first`.
const TERRAIN: core::ops::RangeInclusive<u8> = 1..=0x7f;
/// Materials a voxel can have, as the build table's `voxel.materials`
/// counts them; the per-material arrays the game reads are this long.
const MATERIALS: usize = 0x100;

/// Whether this build's material numbering is the one [`TERRAIN`] and
/// [`MATERIALS`] describe.
fn materials_hold(materials: Option<usize>, building_first: Option<usize>) -> bool {
    materials == Some(MATERIALS) && building_first == Some(usize::from(*TERRAIN.end()) + 1)
}
/// The largest radius the loader takes, in metres: the game reads a region
/// of `(4r)^3` voxels for an edit.
pub(crate) const MAX_RADIUS: f32 = 16.0;
/// The cost of a voxel that leaves the dig's budget whole.
const FREE: f32 = 1e-6;

type AddFn = unsafe extern "system" fn(
    editor: *mut u8,
    undo: *mut u8,
    alloc: *mut u8,
    position: *const Vector,
    radius: f32,
    height: f32,
    shape: u16,
    material: u8,
    dry: u8,
) -> f32;
type DigFn = unsafe extern "system" fn(
    editor: *mut u8,
    undo: *mut u8,
    alloc: *mut u8,
    out: *mut DigOut,
    cost: *const Table<f32>,
    position: *const Vector,
    direction: *const Vector,
    radius: f32,
    shape: u8,
    dry: u8,
);
type FlattenFn = unsafe extern "system" fn(
    editor: *mut u8,
    undo: *mut u8,
    alloc: *mut u8,
    allow: *const Table<u8>,
    position: *const Vector,
    normal: *const Vector,
    radius: f32,
    shape: u8,
    material: u8,
    dry: u8,
) -> u8;

/// A point or a direction as the game's edits read it: four floats,
/// 16-byte aligned, since they load it with aligned moves.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct Vector([f32; 4]);

/// A table by material the game's edits take: entries and their number.
#[repr(C)]
struct Table<T> {
    entries: *const T,
    count: u64,
}

/// What a dig removed, as the game fills it.
#[repr(C)]
struct DigOut {
    per_material: *mut f32,
    count: u64,
    dominant: u8,
    unused: f32,
    removed: f32,
}

/// Where everything lies, from the build table and reflection.
#[derive(Debug)]
struct Layout {
    add: AddFn,
    dig: DigFn,
    flatten: FlattenFn,
    editor: usize,
    world: usize,
    game: usize,
    registry: usize,
    registry_resource: usize,
    configs: usize,
    config_size: usize,
    config_id: usize,
    config_item: usize,
    config_hardness: usize,
    config_health: usize,
    config_flatten: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            if !materials_hold(
                loader.offset(pinned::voxel::MATERIALS),
                loader.offset(pinned::voxel::BUILDING_FIRST),
            ) {
                crate::log!(
                    WARN,
                    "terrain edits off: this build numbers its voxel materials differently"
                );
                return None;
            }
            let reflection = &loader.reflection;
            let field = |ty: &str, name: &str| {
                reflection
                    .field_offset(ty, name)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let function = |name: Symbol| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let config = "keen::TerrainTerraformingConfig";
            let (add, dig, flatten) = (
                function(pinned::terrain::ADD)?,
                function(pinned::terrain::DIG)?,
                function(pinned::terrain::FLATTEN)?,
            );
            Some(Layout {
                // SAFETY: the table's `terrain.add` is the address of the game
                // function with the `AddFn` signature on this build, in the
                // image, which stays mapped.
                add: unsafe { core::mem::transmute::<usize, AddFn>(add) },
                // SAFETY: the table's `terrain.dig` is the address of the game
                // function with the `DigFn` signature on this build, in the
                // image, which stays mapped.
                dig: unsafe { core::mem::transmute::<usize, DigFn>(dig) },
                // SAFETY: the table's `terrain.flatten` is the address of the
                // game function with the `FlattenFn` signature on this build,
                // in the image, which stays mapped.
                flatten: unsafe { core::mem::transmute::<usize, FlattenFn>(flatten) },
                editor: loader.offset(pinned::terrain::EDITOR)?,
                world: loader.offset(pinned::terrain::VOXEL_WORLD)?,
                game: loader.offset(pinned::server::PLAYER_INFOS)?,
                registry: loader.offset(pinned::terrain::REGISTRY)?,
                registry_resource: loader.offset(pinned::terrain::REGISTRY_RESOURCE)?,
                configs: field(
                    "keen::TerraformingEfficiencyRegistryResource",
                    "terrainConfigs",
                )?,
                config_size: reflection
                    .type_info(config)
                    .ok()
                    .and_then(|info| usize::try_from(info.size()).ok())?,
                config_id: field(config, "terrainMaterial")?,
                config_item: field(config, "terrainItem")?,
                config_hardness: field(config, "hardness")?,
                config_health: field(config, "healthPoints")?,
                config_flatten: field(config, "canBeFlattened")?,
            })
        })
        .as_ref()
}

/// The voxel editor, the voxel world's allocator and the game object, on
/// the game thread.
fn editor(layout: &Layout) -> Result<(usize, usize, usize), Status> {
    let server = crate::world::server()?;
    // SAFETY: `server` comes from `crate::world::server`, on the game thread
    // that alone writes it; the table's `terrain.editor` offset holds its
    // voxel editor pointer.
    let editor: usize = unsafe { read(server, layout.editor) };
    // SAFETY: `server` comes from `crate::world::server`, on the game thread
    // that alone writes it; the table's `terrain.voxel_world` offset holds
    // its voxel world pointer.
    let world: usize = unsafe { read(server, layout.world) };
    // SAFETY: `server` comes from `crate::world::server`, on the game thread
    // that alone writes it; the table's `server.player_infos` offset holds
    // its game object pointer.
    let game: usize = unsafe { read(server, layout.game) };
    if editor == 0 || world == 0 || game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `world` is non-zero (`world == 0` returns first), the server's
    // voxel world, which starts with its allocator pointer; read on the game
    // thread.
    let alloc: usize = unsafe { read(world, 0) };
    if alloc == 0 {
        return Err(Status::MISSING);
    }
    Ok((editor, alloc, game))
}

/// The server's voxel editor and its voxel world's allocator, on the game
/// thread at a quiet moment.
pub(crate) fn editor_and_allocator(loader: &Loader) -> Result<(usize, usize), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (editor, alloc, _) = editor(layout)?;
    Ok((editor, alloc))
}

/// The running game's terraforming registry resource, on the game thread.
fn resource(layout: &Layout) -> Result<usize, Status> {
    let (_, _, game) = editor(layout)?;
    // SAFETY: `game` is the non-zero game object `editor` read on the game
    // thread; the table's `terrain.registry` offset holds its terraforming
    // registry pointer.
    let registry: usize = unsafe { read(game, layout.registry) };
    if registry == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `registry` is non-zero (`registry == 0` returns first); the
    // table's `terrain.registry_resource` offset holds its resource pointer
    // once loaded; read on the game thread.
    let resource: usize = unsafe { read(registry, layout.registry_resource) };
    if resource == 0 {
        return Err(Status::MISSING);
    }
    Ok(resource)
}

/// The array a reflected array header at `header` points at: its first
/// entry and its count; `None` for an empty one.
///
/// # Safety
///
/// - `header` is a live reflected array header: an `i32` offset from itself,
///   then a `u32` count, read on the game thread.
unsafe fn array(header: usize) -> Option<(usize, u32)> {
    // SAFETY: the caller guarantees a live array header at `header`, which
    // starts with its `i32` offset.
    let offset: i32 = unsafe { read(header, 0) };
    // SAFETY: the caller guarantees a live array header at `header`, whose
    // `u32` count follows its offset.
    let count: u32 = unsafe { read(header, 4) };
    (offset != 0).then(|| (header.wrapping_add_signed(offset as isize), count))
}

/// The building materials' terraforming configs of the running game: each
/// one's material, counted from 0x80, and its hardness.
pub(crate) fn building_configs(loader: &Loader) -> Result<Vec<(u8, u8)>, Status> {
    use enshwick_gamedata::catalogue;

    let layout = layout(loader).ok_or(Status::MISSING)?;
    let at = |id| {
        catalogue::constant(id, &loader.reflection)
            .and_then(|at| usize::try_from(at).ok())
            .ok_or(Status::MISSING)
    };
    let configs = at(catalogue::TERRAFORMING_BUILDING_CONFIGS)?;
    let material_at = at(catalogue::BUILDING_CONFIG_MATERIAL)?;
    let hardness_at = at(catalogue::BUILDING_CONFIG_HARDNESS)?;
    let size = loader
        .reflection
        .type_info(catalogue::type_name(
            catalogue::BUILDING_TERRAFORMING_CONFIG,
        ))
        .ok()
        .and_then(|info| usize::try_from(info.size()).ok())
        .ok_or(Status::MISSING)?;
    let resource = resource(layout)?;
    // SAFETY: `resource` is the live terraforming registry resource
    // `resource` returned, whose reflected `buildingConfigs` array header
    // sits at the catalogue's offset.
    let Some((start, count)) = (unsafe { array(resource + configs) }) else {
        return Ok(Vec::new());
    };
    Ok((0..count as usize)
        .map(|i| {
            let config = start + i * size;
            (
                // SAFETY: `config` is entry `i < count` of the array, `size`
                // bytes each; the reflected `buildingMaterial` byte sits at
                // the catalogue's offset.
                unsafe { read(config, material_at) },
                // SAFETY: `config` is entry `i < count` of the array, `size`
                // bytes each; the reflected `hardness` byte sits at the
                // catalogue's offset.
                unsafe { read(config, hardness_at) },
            )
        })
        .collect())
}

/// The terrain materials of the running game.
pub(crate) fn materials(loader: &Loader) -> Result<Vec<TerrainMaterialInfo>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let resource = resource(layout)?;
    // SAFETY: `resource` is the live terraforming registry resource
    // `resource` returned, whose reflected `terrainConfigs` array header
    // sits at `layout.configs`.
    let Some((start, count)) = (unsafe { array(resource + layout.configs) }) else {
        return Ok(Vec::new());
    };
    Ok((0..count as usize)
        .map(|i| {
            let config = start + i * layout.config_size;
            // SAFETY: `config` is entry `i < count` of the array the header
            // points at, `config_size` bytes each; the reflected
            // `keen::TerrainTerraformingConfig` places its 16-byte
            // `terrainItem` at `config_item`.
            let item: [u8; 16] = unsafe { read(config, layout.config_item) };
            // SAFETY: `config` is entry `i < count` of the array the header
            // points at, `config_size` bytes each; the reflected
            // `keen::TerrainTerraformingConfig` places its `canBeFlattened`
            // byte at `config_flatten`.
            let flatten: u8 = unsafe { read(config, layout.config_flatten) };
            TerrainMaterialInfo {
                header: Header::of::<TerrainMaterialInfo>(),
                // SAFETY: `config` is entry `i < count` of the array the
                // header points at, `config_size` bytes each; the reflected
                // `keen::TerrainTerraformingConfig` places its
                // `terrainMaterial` at `config_id`.
                id: unsafe { read(config, layout.config_id) },
                flags: if flatten == 0 {
                    0
                } else {
                    TerrainMaterialInfo::FLATTENS
                },
                // SAFETY: `config` is entry `i < count` of the array the
                // header points at, `config_size` bytes each; the reflected
                // `keen::TerrainTerraformingConfig` places its `hardness` at
                // `config_hardness`.
                hardness: unsafe { read(config, layout.config_hardness) },
                reserved: 0,
                item: enshwick_gamedata::reflect::fnv1a32(&item),
                // SAFETY: `config` is entry `i < count` of the array the
                // header points at, `config_size` bytes each; the reflected
                // `keen::TerrainTerraformingConfig` places its `healthPoints`
                // at `config_health`.
                health: unsafe { read(config, layout.config_health) },
            }
        })
        .collect())
}

/// Runs a terrain edit; the amount it changed: volume added or removed in
/// cubic metres, or 1 when a flatten changed anything.
/// [`Status::OUTSIDE_WORLD`] when the box the edit can write leaves the
/// world, [`Status::NOT_LOADED`] unless every column of bricks in it is
/// loaded: the game's edit would otherwise load such a column holding only
/// the edit.
pub(crate) fn edit(loader: &Loader, request: &TerrainEdit) -> Result<f32, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let radius = request.radius;
    if !(radius > 0.0 && radius <= MAX_RADIUS)
        || request
            .position
            .iter()
            .any(|axis| !axis.is_finite() || *axis < 0.0)
        || request.shape > TerrainEdit::SQUARE
    {
        return Err(Status::BAD_ARGS);
    }
    let (editor, alloc, _) = editor(layout)?;
    let extent = crate::voxels::extent(loader)?;
    let bricks = crate::transactions::cover::cover(request)
        .filter(|bricks| bricks.inside(extent))
        .ok_or(Status::OUTSIDE_WORLD)?;
    for column in bricks.columns() {
        crate::voxels::column_loaded(loader, column.min())?;
    }
    let [x, y, z] = request.position;
    let site = Site {
        editor: core::ptr::with_exposed_provenance_mut(editor),
        alloc: core::ptr::with_exposed_provenance_mut(alloc),
        position: Vector([x, y, z, 0.0]),
        radius,
        shape: u8::try_from(request.shape).map_err(|_| Status::BAD_ARGS)?,
        dry: u8::from(request.flags & TerrainEdit::DRY_RUN != 0),
    };
    match request.kind {
        TerrainEdit::ADD => add(layout, &site, request),
        TerrainEdit::DIG => Ok(dig(layout, &site, &request.protect)),
        TerrainEdit::FLATTEN => flatten(layout, &site, request.material),
        _ => Err(Status::BAD_ARGS),
    }
}

/// What every edit takes: the editor, the allocator, the place and the
/// shape.
struct Site {
    editor: *mut u8,
    alloc: *mut u8,
    position: Vector,
    radius: f32,
    shape: u8,
    dry: u8,
}

fn add(layout: &Layout, site: &Site, request: &TerrainEdit) -> Result<f32, Status> {
    let material = terrain_material(request.material)?;
    if !(request.height.is_finite() && request.height > 0.0 && request.height <= MAX_RADIUS) {
        return Err(Status::BAD_ARGS);
    }
    // SAFETY: `add` is the table's `terrain.add`, called on the server's
    // live, non-zero voxel editor and allocator (checked by `editor`) at a
    // quiet moment on the game thread (checked by `crate::world::server`),
    // with no undo; `site.position` is a 16-byte aligned `Vector` for the
    // call, and the radius and height are positive and at most `MAX_RADIUS`
    // (checked in `edit` and by the height check).
    let voxels = unsafe {
        (layout.add)(
            site.editor,
            core::ptr::null_mut(),
            site.alloc,
            &raw const site.position,
            site.radius,
            request.height,
            u16::from(site.shape),
            material,
            site.dry,
        )
    };
    Ok(voxels * VOXEL_VOLUME)
}

fn dig(layout: &Layout, site: &Site, protect: &[u8; 16]) -> f32 {
    let mut cost = [0.0f32; MATERIALS];
    for material in TERRAIN {
        let protected = protect
            .get(usize::from(material / 8))
            .is_some_and(|bits| bits & (1 << (material % 8)) != 0);
        if !protected {
            cost[usize::from(material)] = FREE;
        }
    }
    let table = Table {
        entries: cost.as_ptr(),
        count: MATERIALS as u64,
    };
    let mut removed = [0.0f32; MATERIALS];
    let mut out = DigOut {
        per_material: removed.as_mut_ptr(),
        count: MATERIALS as u64,
        dominant: 0,
        unused: 0.0,
        removed: 0.0,
    };
    let direction = Vector([0.0; 4]);
    // SAFETY: `dig` is the table's `terrain.dig`, called on the server's live,
    // non-zero voxel editor and allocator (checked by `editor`) at a quiet
    // moment on the game thread (checked by `crate::world::server`), with no
    // undo; `out` and `table` point at `MATERIALS` entries each on this stack,
    // the vectors are 16-byte aligned, the radius is positive and at most
    // `MAX_RADIUS` and the shape a plain sphere or cube (checked in `edit`).
    unsafe {
        (layout.dig)(
            site.editor,
            core::ptr::null_mut(),
            site.alloc,
            &raw mut out,
            &raw const table,
            &raw const site.position,
            &raw const direction,
            site.radius,
            site.shape,
            site.dry,
        );
    };
    out.removed * VOXEL_VOLUME
}

fn flatten(layout: &Layout, site: &Site, material: u32) -> Result<f32, Status> {
    let material = terrain_material(material)?;
    let mut fillable = [0u8; MATERIALS];
    for slot in fillable.iter_mut().take(0x80) {
        *slot = 1;
    }
    let table = Table {
        entries: fillable.as_ptr(),
        count: MATERIALS as u64,
    };
    let normal = Vector([0.0, 1.0, 0.0, 0.0]);
    // SAFETY: `flatten` is the table's `terrain.flatten`, called on the
    // server's live, non-zero voxel editor and allocator (checked by `editor`)
    // at a quiet moment on the game thread (checked by
    // `crate::world::server`), with no undo; `table` points at `MATERIALS`
    // entries on this stack, the vectors are 16-byte aligned, the radius is
    // positive and at most `MAX_RADIUS` and the shape a plain sphere or cube
    // (checked in `edit`).
    let changed = unsafe {
        (layout.flatten)(
            site.editor,
            core::ptr::null_mut(),
            site.alloc,
            &raw const table,
            &raw const site.position,
            &raw const normal,
            site.radius,
            site.shape,
            material,
            site.dry,
        )
    };
    Ok(f32::from(u8::from(changed != 0)))
}

/// The volume of a voxel, half a metre a side, in cubic metres.
const VOXEL_VOLUME: f32 = 0.125;

fn terrain_material(material: u32) -> Result<u8, Status> {
    u8::try_from(material)
        .ok()
        .filter(|id| TERRAIN.contains(id))
        .ok_or(Status::BAD_ARGS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_need_the_material_numbering_their_arrays_hold() {
        assert!(materials_hold(Some(0x100), Some(0x80)));
        assert!(!materials_hold(Some(0x200), Some(0x80)), "more materials");
        assert!(!materials_hold(Some(0x100), Some(0x40)), "fewer terrain");
        assert!(!materials_hold(None, Some(0x80)), "not in the table");
        assert!(!materials_hold(Some(0x100), None), "not in the table");
    }
}
