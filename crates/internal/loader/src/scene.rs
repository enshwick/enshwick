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
//! change only with the scene, so they are read once per scene, and a
//! scene that does not read is reported once.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use enshwick::abi::{Header, MapMarkerInfo, ResourceKey, Status};
use enshwick_gamedata::catalogue as names;

use crate::catalogue::types;
use crate::host::Loader;
use crate::locks::Relock;
use crate::pinned;
use crate::scene_markers::{
    Lookup, SceneFailure, SceneKey, SceneLayout, SceneMarkers, collect, placements,
};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::world::SCENE_CHUNKS.id(),
    pinned::world::SCENE_CHUNKS_COUNT.id(),
    pinned::world::SCENE_CHUNKS_SCENE.id(),
];

/// What the loader knows of scenes: the reflected layout, resolved once,
/// and the markers of the last scene asked for.
struct SceneState {
    layout: OnceLock<Option<SceneLayout>>,
    markers: SceneMarkers<MapMarkerInfo>,
}

static STATE: Mutex<SceneState> = Mutex::new(SceneState {
    layout: OnceLock::new(),
    markers: SceneMarkers::new(),
});

/// The reflected layout the parse reads, from the catalogue's facts.
fn layout(loader: &Loader) -> Option<SceneLayout> {
    let field = |id: &str| {
        let offset = crate::catalogue::constant(id, &loader.reflection)?;
        usize::try_from(offset).ok()
    };
    let size = |id: &str| {
        let info = loader.reflection.type_info(names::type_name(id)).ok()?;
        usize::try_from(info.size()).ok()
    };
    let [x0, x1, y0, y1, z0, z1] = names::SCENE_TRANSFORM_POSITION;
    Some(SceneLayout {
        templates: field(names::SCENE_CHUNK_TEMPLATES)?,
        entities: field(names::SCENE_CHUNK_ENTITIES)?,
        entry_size: size(names::SCENE_SPAWN_ENTRY)?,
        entry_template: field(names::SCENE_SPAWN_TEMPLATE)?,
        entry_transform: field(names::SCENE_SPAWN_TRANSFORM)?,
        position: [
            field(x0)?,
            field(x1)?,
            field(y0)?,
            field(y1)?,
            field(z0)?,
            field(z1)?,
        ],
        components: field(names::TEMPLATE_COMPONENTS)?,
        marker_id: field(names::MAP_MARKER_ID)?,
        marker_type: field(names::MAP_MARKER_TYPE)?,
    })
    .filter(SceneLayout::is_sound)
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
    let count_at = offset(pinned::world::SCENE_CHUNKS_COUNT)?;
    let scene_at = offset(pinned::world::SCENE_CHUNKS_SCENE)?;
    // SAFETY: `world` is the running server's non-zero entity world, read
    // on its game thread at a quiet moment (checked by `server`), which
    // alone writes it; the table's `world.scene_chunks` offset holds its
    // scene chunk manager pointer on server 399 and client 426.
    let chunks = unsafe { crate::memory::read_ptr::<u8>(world, chunks_at) };
    if chunks.is_null() {
        return Err(Status::MISSING);
    }
    let chunks = chunks.expose_provenance();
    // SAFETY: `chunks` is the non-null manager the world holds (checked
    // earlier), read on the game thread at a quiet moment; the table's
    // offset of its chunk count (u64) lies inside it.
    let count = unsafe { crate::memory::read::<u64>(chunks, count_at) };
    // SAFETY: `chunks` is the non-null manager the world holds (checked
    // earlier), read on the game thread at a quiet moment; the table's
    // offset of the requested scene GUID (16 bytes) lies inside it.
    let scene = unsafe { crate::memory::read::<[u8; 16]>(chunks, scene_at) };
    SceneKey::new(scene, count).ok_or(Status::MISSING)
}

/// Reads the markers of the scene `key` from the archive.
fn read(layout: Option<SceneLayout>, key: SceneKey) -> Result<Vec<MapMarkerInfo>, SceneFailure> {
    let layout = layout.ok_or(SceneFailure::Layout)?;
    let chunk_hash = names::type_hash(names::SCENE_CHUNK).ok_or(SceneFailure::Layout)?;
    let marker_hash = names::type_hash(names::MAP_MARKER).ok_or(SceneFailure::Layout)?;
    let archive = crate::archive::game().ok_or(SceneFailure::Archive)?;
    let chunk = archive
        .resource(&ResourceKey {
            guid: key.scene,
            type_hash: chunk_hash,
            part: key.part,
        })
        .ok_or(SceneFailure::Chunk)?;
    let placed = placements(&layout, &chunk).ok_or(SceneFailure::ChunkUnreadable)?;
    let (found, skipped) = collect(&layout, &placed, marker_hash, |guid| {
        archive.resource(&ResourceKey {
            guid,
            type_hash: types::TEMPLATE,
            part: 0,
        })
    })?;
    if skipped.missing + skipped.unreadable > 0 {
        crate::log!(
            WARN,
            "map markers: {} templates of the scene are missing and {} do not read; their markers are left out",
            skipped.missing,
            skipped.unreadable
        );
    }
    Ok(found
        .into_iter()
        .map(|(placed, data)| MapMarkerInfo {
            header: Header::of::<MapMarkerInfo>(),
            position: placed.position,
            marker_type: data.marker_type,
            label: data.label,
        })
        .collect())
}

/// The markers of the scene the server runs; on its game thread at a quiet
/// moment, since the scene comes from the running world. The first call
/// for a scene starts reading the archive on a thread of its own and
/// answers [`Status::NOT_LOADED`] until it is done.
pub(crate) fn markers(loader: &Loader) -> Result<Arc<[MapMarkerInfo]>, Status> {
    let key = scene_key(loader)?;
    let mut state = STATE.relock();
    let layout = *state.layout.get_or_init(|| layout(loader));
    match state.markers.lookup(key) {
        Lookup::Ready(found) => found.map_err(|_| Status::MISSING),
        Lookup::Pending => Err(Status::NOT_LOADED),
        Lookup::Start => {
            drop(state);
            let work = move || {
                let began = Instant::now();
                let found = read(layout, key);
                match &found {
                    Ok(markers) => crate::log!(
                        DEBUG,
                        "map markers: {} in the scene's global chunk, read in {} ms",
                        markers.len(),
                        began.elapsed().as_millis()
                    ),
                    Err(why) => crate::log!(WARN, "map markers off for this scene: {why}"),
                }
                let _kept = STATE.relock().markers.finish(key, found);
            };
            if let Err(why) = std::thread::Builder::new()
                .name("enshwick map markers".to_owned())
                .spawn(work)
            {
                crate::log!(WARN, "map markers: no thread to read the scene ({why})");
                let _kept = STATE
                    .relock()
                    .markers
                    .finish(key, Err(SceneFailure::Thread));
            }
            Err(Status::NOT_LOADED)
        }
    }
}
