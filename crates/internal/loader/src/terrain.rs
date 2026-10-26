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

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "server.player_infos",
    "terrain.add",
    "terrain.dig",
    "terrain.editor",
    "terrain.flatten",
    "terrain.registry",
    "terrain.registry_resource",
    "terrain.voxel_world",
];

/// The materials the edits work on: terrain, not air nor buildings.
const TERRAIN: core::ops::RangeInclusive<u8> = 1..=0x7f;
/// Materials a voxel can have.
const MATERIALS: usize = 0x100;
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
            let reflection = &loader.reflection;
            let field = |ty: &str, name: &str| {
                reflection
                    .field_offset(ty, name)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let function = |name: &str| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let config = "keen::TerrainTerraformingConfig";
            let (add, dig, flatten) = (
                function("terrain.add")?,
                function("terrain.dig")?,
                function("terrain.flatten")?,
            );
            Some(Layout {
                // SAFETY: each symbol is the game function with this
                // signature on this build.
                add: unsafe { core::mem::transmute::<usize, AddFn>(add) },
                // SAFETY: as above.
                dig: unsafe { core::mem::transmute::<usize, DigFn>(dig) },
                // SAFETY: as above.
                flatten: unsafe { core::mem::transmute::<usize, FlattenFn>(flatten) },
                editor: loader.offset("terrain.editor")?,
                world: loader.offset("terrain.voxel_world")?,
                game: loader.offset("server.player_infos")?,
                registry: loader.offset("terrain.registry")?,
                registry_resource: loader.offset("terrain.registry_resource")?,
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
    // SAFETY: the server holds its voxel editor, voxel world and game object.
    let editor: usize = unsafe { read(server, layout.editor) };
    // SAFETY: as above.
    let world: usize = unsafe { read(server, layout.world) };
    // SAFETY: as above.
    let game: usize = unsafe { read(server, layout.game) };
    if editor == 0 || world == 0 || game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the voxel world starts with its allocator.
    let alloc: usize = unsafe { read(world, 0) };
    if alloc == 0 {
        return Err(Status::MISSING);
    }
    Ok((editor, alloc, game))
}

/// The terrain materials of the running game.
pub(crate) fn materials(loader: &Loader) -> Result<Vec<TerrainMaterialInfo>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (_, _, game) = editor(layout)?;
    // SAFETY: the game object holds the terraforming registry, which holds
    // its resource once loaded.
    let registry: usize = unsafe { read(game, layout.registry) };
    if registry == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: as above.
    let resource: usize = unsafe { read(registry, layout.registry_resource) };
    if resource == 0 {
        return Err(Status::MISSING);
    }
    let header = resource + layout.configs;
    // SAFETY: the resource's array of terrain configs: an offset from itself
    // and a count.
    let offset: i32 = unsafe { read(header, 0) };
    // SAFETY: as above.
    let count: u32 = unsafe { read(header, 4) };
    if offset == 0 {
        return Ok(Vec::new());
    }
    let start = header.wrapping_add_signed(offset as isize);
    Ok((0..count as usize)
        .map(|i| {
            let config = start + i * layout.config_size;
            // SAFETY: a config of the array, its fields as reflected.
            let item: [u8; 16] = unsafe { read(config, layout.config_item) };
            // SAFETY: as above.
            let flatten: u8 = unsafe { read(config, layout.config_flatten) };
            TerrainMaterialInfo {
                header: Header::of::<TerrainMaterialInfo>(),
                // SAFETY: as above.
                id: unsafe { read(config, layout.config_id) },
                flags: if flatten == 0 {
                    0
                } else {
                    TerrainMaterialInfo::FLATTENS
                },
                // SAFETY: as above.
                hardness: unsafe { read(config, layout.config_hardness) },
                reserved: 0,
                item: enshwick_reflect::fnv1a32(&item),
                // SAFETY: as above.
                health: unsafe { read(config, layout.config_health) },
            }
        })
        .collect())
}

/// Runs a terrain edit; the amount it changed: volume added or removed in
/// cubic metres, or 1 when a flatten changed anything.
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
    // SAFETY: the game's add on its live voxel editor and allocator, with no
    // undo, on the game thread between steps.
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
    // SAFETY: the game's dig on its live voxel editor and allocator, with no
    // undo, tables of its size and a plain sphere or cube, on the game thread
    // between steps.
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
    // SAFETY: the game's flatten on its live voxel editor and allocator, with
    // no undo, a table of its size and the vertical normal, on the game thread
    // between steps.
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
