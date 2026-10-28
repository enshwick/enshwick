//! The map markers and the map's frame of the scene the server runs, read
//! from the game's archive.
//!
//! Engine facts from server and client build 1273043 (ages 399 and 426):
//! the entity world keeps a scene chunk manager whose requested
//! scene GUID and chunk count the table pins; its last chunk is the global
//! one, which holds every map marker the scene places (`MapMarker` on a
//! template). A dedicated server builds those entities without a marker
//! the loader can list, so the markers come from the scene's own data:
//! the global chunk resource and the template resources it names. The
//! scene's `SceneResource` and its `BiomeMapResource` carry the scene's
//! GUID too, at part 0; the biome map holds the map's frame (server 399
//! data). Both change only with the scene, so they are read once per scene,
//! and a scene that does not read is reported once.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use enshwick::abi::{Header, MapFrameInfo, MapMarkerInfo, ResourceKey, Status};
use enshwick_gamedata::catalogue as names;

use crate::catalogue::types;
use crate::host::Loader;
use crate::locks::Relock;
use crate::map_frame::{FrameLayout, ReadFrameError, frame, has_biome_map};
use crate::pinned;
use crate::scene_markers::{
    Lookup, ReadSceneError, SceneCache, SceneKey, SceneLayout, collect, placements,
};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::world::SCENE_CHUNKS.id(),
    pinned::world::SCENE_CHUNKS_COUNT.id(),
    pinned::world::SCENE_CHUNKS_SCENE.id(),
];

/// What the loader knows of scenes: the reflected layouts, resolved once,
/// and the markers and the frame of the last scene asked for.
struct SceneState {
    layout: OnceLock<Option<SceneLayout>>,
    frame_layout: OnceLock<Option<FrameLayout>>,
    markers: SceneCache<Arc<[MapMarkerInfo]>, ReadSceneError>,
    frame: SceneCache<MapFrameInfo, ReadFrameError>,
}

static STATE: Mutex<SceneState> = Mutex::new(SceneState {
    layout: OnceLock::new(),
    frame_layout: OnceLock::new(),
    markers: SceneCache::new(),
    frame: SceneCache::new(),
});

/// The offset of the catalogue's field `id` in the running game.
fn field(loader: &Loader, id: &str) -> Option<usize> {
    let offset = crate::catalogue::constant(id, &loader.reflection)?;
    usize::try_from(offset).ok()
}

/// The size of the catalogue's type `id` in the running game.
fn size(loader: &Loader, id: &str) -> Option<usize> {
    let info = loader.reflection.type_info(names::type_name(id)).ok()?;
    usize::try_from(info.size()).ok()
}

/// The reflected layout the marker parse reads, from the catalogue's facts.
fn layout(loader: &Loader) -> Option<SceneLayout> {
    let field = |id: &str| field(loader, id);
    let [x0, x1, y0, y1, z0, z1] = names::SCENE_TRANSFORM_POSITION;
    Some(SceneLayout {
        templates: field(names::SCENE_CHUNK_TEMPLATES)?,
        entities: field(names::SCENE_CHUNK_ENTITIES)?,
        entry_size: size(loader, names::SCENE_SPAWN_ENTRY)?,
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
        marker_size: size(loader, names::MAP_MARKER)?,
    })
    .filter(SceneLayout::is_sound)
}

/// The reflected layout the frame parse reads, from the catalogue's facts.
fn frame_layout(loader: &Loader) -> Option<FrameLayout> {
    let field = |id: &str| field(loader, id);
    Some(FrameLayout {
        has_biome_map: field(names::SCENE_HAS_BIOME_MAP)?,
        scene_size: size(loader, names::SCENE_RESOURCE)?,
        world_origin: field(names::BIOME_MAP_WORLD_ORIGIN)?,
        world_size: field(names::BIOME_MAP_WORLD_SIZE)?,
        map_scale: field(names::BIOME_MAP_MAP_SCALE)?,
        biome_map_size: size(loader, names::BIOME_MAP)?,
    })
    .filter(FrameLayout::is_sound)
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
fn read_markers(
    layout: Option<SceneLayout>,
    key: SceneKey,
) -> Result<Arc<[MapMarkerInfo]>, ReadSceneError> {
    let layout = layout.ok_or(ReadSceneError::Layout)?;
    let chunk_hash = names::type_hash(names::SCENE_CHUNK).ok_or(ReadSceneError::Layout)?;
    let marker_hash = names::type_hash(names::MAP_MARKER).ok_or(ReadSceneError::Layout)?;
    let archive = crate::archive::game().ok_or(ReadSceneError::Archive)?;
    let chunk = archive
        .resource(&ResourceKey {
            guid: key.scene,
            type_hash: chunk_hash,
            part: key.part,
        })
        .ok_or(ReadSceneError::Chunk)?;
    let placed = placements(&layout, &chunk).ok_or(ReadSceneError::ChunkUnreadable)?;
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

/// Reads the frame of the scene `key` from the archive: its scene
/// resource's flag, then its biome map.
fn read_frame(layout: Option<FrameLayout>, key: SceneKey) -> Result<MapFrameInfo, ReadFrameError> {
    let layout = layout.ok_or(ReadFrameError::Layout)?;
    let scene_hash = names::type_hash(names::SCENE_RESOURCE).ok_or(ReadFrameError::Layout)?;
    let biome_hash = names::type_hash(names::BIOME_MAP).ok_or(ReadFrameError::Layout)?;
    let archive = crate::archive::game().ok_or(ReadFrameError::Archive)?;
    let part0 = |type_hash| {
        archive.resource(&ResourceKey {
            guid: key.scene,
            type_hash,
            part: 0,
        })
    };
    let scene = part0(scene_hash).ok_or(ReadFrameError::Scene)?;
    if !has_biome_map(&layout, &scene)? {
        return Err(ReadFrameError::NoBiomeMap);
    }
    let biome_map = part0(biome_hash).ok_or(ReadFrameError::BiomeMap)?;
    let (origin, size, metres_per_cell) = frame(&layout, &biome_map)?.metres();
    Ok(MapFrameInfo {
        header: Header::of::<MapFrameInfo>(),
        origin,
        size,
        metres_per_cell,
    })
}

/// What a read of the scene says about itself in the log.
trait Described: Clone + Send + 'static {
    /// The line once the read is done.
    fn done(&self, millis: u128) -> String;
}

impl Described for Arc<[MapMarkerInfo]> {
    fn done(&self, millis: u128) -> String {
        format!(
            "map markers: {} in the scene's global chunk, read in {millis} ms",
            self.len()
        )
    }
}

impl Described for MapFrameInfo {
    fn done(&self, millis: u128) -> String {
        format!(
            "map frame: origin {:?} m, size {:?} m, {} m per cell, read in {millis} ms",
            self.origin, self.size, self.metres_per_cell
        )
    }
}

/// The failures every read of a scene shares.
trait ReadFailure: core::fmt::Display + Clone + Send + 'static {
    /// The read panicked.
    const PANICKED: Self;
    /// No thread could be started.
    const THREAD: Self;
}

impl ReadFailure for ReadSceneError {
    const PANICKED: Self = Self::Panicked;
    const THREAD: Self = Self::Thread;
}

impl ReadFailure for ReadFrameError {
    const PANICKED: Self = Self::Panicked;
    const THREAD: Self = Self::Thread;
}

/// The read of `key` that `cache` keeps, or [`Status::NOT_LOADED`] while it
/// runs: the first call for a scene starts `read` on a thread of its own,
/// named for `what`.
fn cached<V: Described, E: ReadFailure>(
    key: SceneKey,
    what: &'static str,
    cache: fn(&mut SceneState) -> &mut SceneCache<V, E>,
    read: impl FnOnce() -> Result<V, E> + Send + 'static,
) -> Result<V, Status> {
    match cache(&mut STATE.relock()).lookup(key) {
        Lookup::Ready(found) => return found.map_err(|_| Status::MISSING),
        Lookup::Pending => return Err(Status::NOT_LOADED),
        Lookup::Start => {}
    }
    let work = move || {
        let began = Instant::now();
        let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
            .unwrap_or(Err(E::PANICKED));
        match &found {
            Ok(value) => crate::log!(DEBUG, "{}", value.done(began.elapsed().as_millis())),
            Err(why) => crate::log!(WARN, "{what} off for this scene: {why}"),
        }
        let _kept = cache(&mut STATE.relock()).finish(key, found);
    };
    if let Err(why) = std::thread::Builder::new()
        .name(format!("enshwick {what}"))
        .spawn(work)
    {
        crate::log!(WARN, "{what}: no thread to read the scene ({why})");
        let _kept = cache(&mut STATE.relock()).finish(key, Err(E::THREAD));
    }
    Err(Status::NOT_LOADED)
}

/// The markers of the scene the server runs; on its game thread at a quiet
/// moment, since the scene comes from the running world. The first call
/// for a scene starts reading the archive on a thread of its own and
/// answers [`Status::NOT_LOADED`] until it is done.
pub(crate) fn markers(loader: &Loader) -> Result<Arc<[MapMarkerInfo]>, Status> {
    let key = scene_key(loader)?;
    let layout = *STATE.relock().layout.get_or_init(|| layout(loader));
    cached(
        key,
        "map markers",
        |state| &mut state.markers,
        move || read_markers(layout, key),
    )
}

/// The map's frame of the scene the server runs; on its game thread at a
/// quiet moment, read like [`markers`].
pub(crate) fn frame_info(loader: &Loader) -> Result<MapFrameInfo, Status> {
    let key = scene_key(loader)?;
    let layout = *STATE
        .relock()
        .frame_layout
        .get_or_init(|| frame_layout(loader));
    cached(
        key,
        "map frame",
        |state| &mut state.frame,
        move || read_frame(layout, key),
    )
}
