//! The map markers of the scene the server runs, read from the game's
//! archive.
//!
//! Engine facts from server build 1269579 (age 399) and client 1273043
//! (age 426): the entity world keeps a scene chunk manager whose requested
//! scene GUID and chunk count the table pins; its last chunk is the global
//! one, which holds every map marker the scene places (`MapMarker` on a
//! template). A dedicated server builds those entities without a marker
//! the loader can list, so the markers come from the scene's own data:
//! the global chunk resource and the template resources it names. They
//! change only with the scene, so the list is read once per scene.

use std::sync::{Arc, Mutex};

use enshwick::abi::{Header, MapMarkerInfo, ResourceKey, Status};
use enshwick_gamedata::reflect::fnv1a32;
use rustc_hash::FxHashMap;

use crate::host::Loader;
use crate::locks::Relock;
use crate::pinned;
use crate::scene_markers::{SceneLayout, marker, placements};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::world::SCENE_CHUNKS.id(),
    pinned::world::SCENE_CHUNKS_COUNT.id(),
    pinned::world::SCENE_CHUNKS_SCENE.id(),
];

/// More chunks than any scene has: a count past it is a misread.
const MAX_CHUNKS: u64 = 1 << 20;

const CHUNK_RESOURCE: &str = "keen::SceneEntityChunkResource";
const TEMPLATE_RESOURCE: &str = "keen::ecs::TemplateResource";
const MAP_MARKER: &str = "keen::ecs::MapMarker";

/// A scene's global chunk: its scene GUID and chunk part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SceneKey {
    scene: [u8; 16],
    part: u32,
}

/// The markers of the last scene read.
static READ: Mutex<Option<(SceneKey, Arc<[MapMarkerInfo]>)>> = Mutex::new(None);

/// The reflected layout the parse reads, once per process.
fn layout(loader: &Loader) -> Option<SceneLayout> {
    let field = |ty: &str, name: &str| {
        let offset = loader.reflection.field_offset(ty, name).ok()?;
        usize::try_from(offset).ok()
    };
    let size = |ty: &str| {
        let info = loader.reflection.type_info(ty).ok()?;
        usize::try_from(info.size()).ok()
    };
    let entry = "keen::SceneEntityChunkSpawnEntry";
    let transform = "keen::SceneEntityTransform";
    let position = |name| field(transform, name);
    Some(SceneLayout {
        templates: field(CHUNK_RESOURCE, "templates")?,
        entities: field(CHUNK_RESOURCE, "entities")?,
        entry_size: size(entry)?,
        entry_template: field(entry, "templateIndex")?,
        entry_transform: field(entry, "transform")?,
        position: [
            position("positionX0")?,
            position("positionX1")?,
            position("positionY0")?,
            position("positionY1")?,
            position("positionZ0")?,
            position("positionZ1")?,
        ],
        components: field(TEMPLATE_RESOURCE, "components")?,
        marker_id: field(MAP_MARKER, "markerId")?,
        marker_type: field(MAP_MARKER, "markerType")?,
    })
}

/// The scene the server runs now and its global chunk; on the game thread
/// at a quiet moment.
fn scene_key(loader: &Loader) -> Result<SceneKey, Status> {
    crate::world::server()?;
    let world = usize::try_from(crate::world::server_world())
        .ok()
        .filter(|&world| world != 0)
        .ok_or(Status::MISSING)?;
    let offset = |id| loader.offset(id).ok_or(Status::MISSING);
    let chunks_at = offset(pinned::world::SCENE_CHUNKS)?;
    // SAFETY: `world` is the running server's non-zero entity world, read
    // on its game thread at a quiet moment (checked by `server`), which
    // alone writes it; the table's `world.scene_chunks` offset holds its
    // scene chunk manager pointer on server 399 and client 426.
    let chunks = unsafe { crate::memory::read_ptr::<u8>(world, chunks_at) };
    if chunks.is_null() {
        return Err(Status::MISSING);
    }
    let chunks = chunks.expose_provenance();
    // SAFETY: `chunks` is the non-null manager the world holds, read on
    // the game thread at a quiet moment; the table's offset of its chunk
    // count (u64) lies inside it.
    let count =
        unsafe { crate::memory::read::<u64>(chunks, offset(pinned::world::SCENE_CHUNKS_COUNT)?) };
    // SAFETY: `chunks` is the non-null manager the world holds, read on
    // the game thread at a quiet moment; the table's offset of the
    // requested scene GUID (16 bytes) lies inside it.
    let scene = unsafe {
        crate::memory::read::<[u8; 16]>(chunks, offset(pinned::world::SCENE_CHUNKS_SCENE)?)
    };
    if count == 0 || count > MAX_CHUNKS || scene == [0; 16] {
        return Err(Status::MISSING);
    }
    let part = u32::try_from(count - 1).map_err(|_| Status::MISSING)?;
    Ok(SceneKey { scene, part })
}

/// Reads the markers of the scene `key` from the archive.
fn read(loader: &Loader, key: SceneKey) -> Option<Vec<MapMarkerInfo>> {
    let layout = layout(loader)?;
    let archive = crate::archive::game()?;
    let chunk = archive.resource(&ResourceKey {
        guid: key.scene,
        type_hash: fnv1a32(CHUNK_RESOURCE.as_bytes()),
        part: key.part,
    })?;
    let marker_hash = fnv1a32(MAP_MARKER.as_bytes());
    let template_hash = fnv1a32(TEMPLATE_RESOURCE.as_bytes());
    let mut templates = FxHashMap::default();
    let markers = placements(&layout, &chunk)?
        .into_iter()
        .filter_map(|placed| {
            let found = *templates.entry(placed.template).or_insert_with(|| {
                let key = ResourceKey {
                    guid: placed.template,
                    type_hash: template_hash,
                    part: 0,
                };
                archive
                    .resource(&key)
                    .and_then(|template| marker(&layout, &template, marker_hash))
            });
            found.map(|data| MapMarkerInfo {
                header: Header::of::<MapMarkerInfo>(),
                position: placed.position,
                marker_type: data.marker_type,
                label: data.label,
            })
        })
        .collect();
    Some(markers)
}

/// The markers of the scene the server runs; on its game thread at a quiet
/// moment, since the scene comes from the running world. The first call
/// for a scene reads the archive.
pub(crate) fn markers(loader: &Loader) -> Result<Arc<[MapMarkerInfo]>, Status> {
    let key = scene_key(loader)?;
    let mut held = READ.relock();
    if let Some((read_for, markers)) = held.as_ref()
        && *read_for == key
    {
        return Ok(Arc::clone(markers));
    }
    let Some(found) = read(loader, key) else {
        crate::log!(
            WARN,
            "map markers: the scene's chunk does not read on this build"
        );
        return Err(Status::MISSING);
    };
    crate::log!(
        DEBUG,
        "map markers: {} in the scene's global chunk",
        found.len()
    );
    let markers: Arc<[MapMarkerInfo]> = found.into();
    *held = Some((key, Arc::clone(&markers)));
    Ok(markers)
}
