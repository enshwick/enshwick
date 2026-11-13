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
//! and a scene that does not read is reported once. The client's archive
//! also holds one `MapMarkerRegistryResource`, the map's marker types,
//! found by its reflected type and read once (client 426; a dedicated
//! server's archive has none).

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::engine::off_thread::ReadFailure;

use enshwick_core::abi::{
    Header, MapFrameInfo, MapMarkerInfo, MapMarkerTypeInfo, MarkerTypeSorting, ResourceKey, Side,
    Status,
};
use enshwick_gamedata::catalogue as names;

use crate::engine::catalogue::types;
use crate::engine::fuse::{Fuse, Off};
use crate::engine::loader::Loader;
use crate::engine::pinned;
use crate::world::map_frame::{FrameLayout, ReadFrameError, frame, has_biome_map};
use crate::world::map_marker_types::{ReadTypesError, TypesLayout, types};
use crate::world::scene_markers::{
    Lookup, ReadSceneError, SceneCache, SceneKey, SceneLayout, collect, placements,
};
use enshwick_base::Relock;

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
    /// The client's marker types, once read; `None` when they do not read.
    marker_types: OnceLock<Option<Arc<[MapMarkerTypeInfo]>>>,
}

static STATE: Mutex<SceneState> = Mutex::new(SceneState {
    layout: OnceLock::new(),
    frame_layout: OnceLock::new(),
    markers: SceneCache::new(),
    frame: SceneCache::new(),
    marker_types: OnceLock::new(),
});

/// The API's marker type groups, by the catalogue's id of the game's
/// constant for each.
const GROUPS: [(&str, MarkerTypeSorting); 5] = [
    (
        names::ids::MARKER_TYPE_SORTING_NONE,
        MarkerTypeSorting::NONE,
    ),
    (
        names::ids::MARKER_TYPE_SORTING_FLAME_RELATED,
        MarkerTypeSorting::FLAME_RELATED,
    ),
    (
        names::ids::MARKER_TYPE_SORTING_DUNGEONS,
        MarkerTypeSorting::DUNGEONS,
    ),
    (
        names::ids::MARKER_TYPE_SORTING_LOCATIONS,
        MarkerTypeSorting::LOCATIONS,
    ),
    (
        names::ids::MARKER_TYPE_SORTING_CUSTOM_MARKER,
        MarkerTypeSorting::CUSTOM_MARKER,
    ),
];

/// The offset of the catalogue's field `id` in the running game.
fn field(loader: &Loader, id: &str) -> Option<usize> {
    let offset = crate::engine::catalogue::constant(id, &loader.reflection)?;
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
        templates: field(names::ids::SCENE_CHUNK_TEMPLATES)?,
        entities: field(names::ids::SCENE_CHUNK_ENTITIES)?,
        entry_size: size(loader, names::ids::SCENE_SPAWN_ENTRY)?,
        entry_template: field(names::ids::SCENE_SPAWN_ENTRY_TEMPLATE_INDEX)?,
        entry_transform: field(names::ids::SCENE_SPAWN_ENTRY_TRANSFORM)?,
        position: [
            field(x0)?,
            field(x1)?,
            field(y0)?,
            field(y1)?,
            field(z0)?,
            field(z1)?,
        ],
        components: field(names::ids::TEMPLATE_COMPONENTS)?,
        marker_id: field(names::ids::MAP_MARKER_MARKER_ID)?,
        marker_type: field(names::ids::MAP_MARKER_MARKER_TYPE)?,
        marker_size: size(loader, names::ids::MAP_MARKER)?,
    })
    .filter(SceneLayout::is_sound)
}

/// The reflected layout the frame parse reads, from the catalogue's facts.
fn frame_layout(loader: &Loader) -> Option<FrameLayout> {
    let field = |id: &str| field(loader, id);
    Some(FrameLayout {
        has_biome_map: field(names::ids::SCENE_RESOURCE_HAS_BIOME_MAP)?,
        scene_size: size(loader, names::ids::SCENE_RESOURCE)?,
        world_origin: field(names::ids::BIOME_MAP_WORLD_ORIGIN)?,
        world_size: field(names::ids::BIOME_MAP_WORLD_SIZE)?,
        map_scale: field(names::ids::BIOME_MAP_MAP_SCALE)?,
        biome_map_size: size(loader, names::ids::BIOME_MAP)?,
    })
    .filter(FrameLayout::is_sound)
}

/// The scene the server runs now and its global chunk; on the game thread
/// at a quiet moment.
fn scene_key(loader: &Loader) -> Result<SceneKey, Status> {
    crate::engine::world::server()?;
    let world = usize::try_from(crate::engine::world::server_world())
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
    let chunks = unsafe { crate::engine::memory::read_ptr::<u8>(world, chunks_at) };
    if chunks.is_null() {
        return Err(Status::MISSING);
    }
    let chunks = chunks.expose_provenance();
    // SAFETY: `chunks` is the non-null manager the world holds (checked
    // earlier), read on the game thread at a quiet moment; the table's
    // offset of its chunk count (u64) lies inside it.
    let count = unsafe { crate::engine::memory::read::<u64>(chunks, count_at) };
    // SAFETY: `chunks` is the non-null manager the world holds (checked
    // earlier), read on the game thread at a quiet moment; the table's
    // offset of the requested scene GUID (16 bytes) lies inside it.
    let scene = unsafe { crate::engine::memory::read::<[u8; 16]>(chunks, scene_at) };
    SceneKey::new(scene, count).ok_or(Status::MISSING)
}

/// Reads the markers of the scene `key` from the archive.
fn read_markers(
    layout: Option<SceneLayout>,
    key: SceneKey,
) -> Result<Arc<[MapMarkerInfo]>, ReadSceneError> {
    let layout = layout.ok_or(ReadSceneError::Layout)?;
    let chunk_hash = names::type_hash(names::ids::SCENE_CHUNK).ok_or(ReadSceneError::Layout)?;
    let marker_hash = names::type_hash(names::ids::MAP_MARKER).ok_or(ReadSceneError::Layout)?;
    let archive = crate::engine::archive::game().ok_or(ReadSceneError::Archive)?;
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
    let scene_hash = names::type_hash(names::ids::SCENE_RESOURCE).ok_or(ReadFrameError::Layout)?;
    let biome_hash = names::type_hash(names::ids::BIOME_MAP).ok_or(ReadFrameError::Layout)?;
    let archive = crate::engine::archive::game().ok_or(ReadFrameError::Archive)?;
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

impl ReadFailure for ReadSceneError {
    const PANICKED: Self = Self::Panicked;
    const THREAD: Self = Self::Thread;
}

impl ReadFailure for ReadFrameError {
    const PANICKED: Self = Self::Panicked;
    const THREAD: Self = Self::Thread;
}

/// A failed read of a scene, which turns its feature off when no scene
/// could read: the build lacks the reflected layout, or the read itself
/// failed.
trait SceneFailure: ReadFailure + Clone {
    /// Why `feature` is off on the build named `build`; `None` for a
    /// failure of one scene's data, which leaves the next scene to read.
    fn off(&self, feature: &str, build: &str) -> Option<Off>;
}

/// What the line names when the build lacks part of a read's layout, and
/// why.
const LAYOUT_OFF: (&str, &str) = ("its reflected layout", "is not all in this build");

impl SceneFailure for ReadSceneError {
    fn off(&self, feature: &str, build: &str) -> Option<Off> {
        match self {
            Self::Layout => Some(Off::build(feature, build, LAYOUT_OFF.0, LAYOUT_OFF.1)),
            Self::Panicked | Self::Thread => Some(Off::fault(feature, &self.to_string())),
            Self::Archive | Self::Chunk | Self::ChunkUnreadable | Self::Templates => None,
        }
    }
}

impl SceneFailure for ReadFrameError {
    fn off(&self, feature: &str, build: &str) -> Option<Off> {
        match self {
            Self::Layout => Some(Off::build(feature, build, LAYOUT_OFF.0, LAYOUT_OFF.1)),
            Self::Panicked | Self::Thread => Some(Off::fault(feature, &self.to_string())),
            Self::Archive
            | Self::Scene
            | Self::SceneUnreadable
            | Self::NoBiomeMap
            | Self::BiomeMap
            | Self::BiomeMapUnreadable => None,
        }
    }
}

/// The switch of the map markers.
static MARKERS_FUSE: Fuse = Fuse::new("map markers");
/// The switch of the map's frame.
static FRAME_FUSE: Fuse = Fuse::new("map frame");
/// The switch of the client's marker types.
static TYPES_FUSE: Fuse = Fuse::new("map marker types");

/// The read of `key` that `cache` keeps, or [`Status::NOT_LOADED`] while it
/// runs: the first call for a scene starts `read` on a thread of its own,
/// named for `fuse`'s feature, on the game build named `build`. A failure
/// that leaves no scene to read turns the feature off.
fn cached<V: Described, E: SceneFailure>(
    key: SceneKey,
    fuse: &'static Fuse,
    build: &str,
    cache: fn(&mut SceneState) -> &mut SceneCache<V, E>,
    read: impl FnOnce() -> Result<V, E> + Send + 'static,
) -> Result<V, Status> {
    if fuse.is_off() {
        return Err(Status::MISSING);
    }
    match cache(&mut STATE.relock()).lookup(key) {
        Lookup::Ready(found) => return found.map_err(|_| Status::MISSING),
        Lookup::Pending => return Err(Status::NOT_LOADED),
        Lookup::Start => {}
    }
    let what = fuse.feature();
    let build = build.to_owned();
    crate::engine::off_thread::read(what, read, move |found, took| {
        match &found {
            Ok(value) => crate::log!(DEBUG, "{}", value.done(took.as_millis())),
            Err(why) => {
                if let Some(off) = why.off(what, &build) {
                    fuse.turn_off(&off);
                } else {
                    crate::log!(WARN, "{what}: this scene does not read: {why}");
                }
            }
        }
        let _kept = cache(&mut STATE.relock()).finish(key, found);
    });
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
        &MARKERS_FUSE,
        &loader.build.name,
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
        &FRAME_FUSE,
        &loader.build.name,
        |state| &mut state.frame,
        move || read_frame(layout, key),
    )
}

/// The client's marker types, read from the game's archive at the first ask
/// and kept; any thread. A dedicated server's archive has no registry. The
/// first ask reads under the scene lock, a few milliseconds once, so two
/// first asks read and report once.
pub(crate) fn marker_types(loader: &Loader) -> Result<Arc<[MapMarkerTypeInfo]>, Status> {
    if loader.side == Side::SERVER {
        return Err(Status::MISSING);
    }
    let state = STATE.relock();
    let known = state.marker_types.get_or_init(|| {
        let started = Instant::now();
        match read_marker_types(loader) {
            Ok(read) => {
                crate::log!(
                    INFO,
                    "map marker types: {} in the registry, read in {} ms",
                    read.len(),
                    started.elapsed().as_millis()
                );
                Some(Arc::from(read))
            }
            Err(why) => {
                let _off = loader.fuse_off(
                    &TYPES_FUSE,
                    names::ids::MAP_MARKER_REGISTRY,
                    &why.to_string(),
                );
                None
            }
        }
    });
    known.clone().ok_or(Status::MISSING)
}

/// The reflected layout the marker type parse reads, from the catalogue's
/// facts.
fn types_layout(loader: &Loader) -> Option<TypesLayout> {
    let field = |id: &str| field(loader, id);
    Some(TypesLayout {
        types: field(names::ids::MAP_MARKER_REGISTRY_TYPES)?,
        registry_size: size(loader, names::ids::MAP_MARKER_REGISTRY)?,
        type_size: size(loader, names::ids::MAP_MARKER_TYPE)?,
        type_name: field(names::ids::MAP_MARKER_TYPE_TYPE_NAME)?,
        marker_id: field(names::ids::MAP_MARKER_TYPE_MARKER_ID)?,
        sorting_category: field(names::ids::MAP_MARKER_TYPE_SORTING_CATEGORY)?,
        is_fast_travel_destination: field(names::ids::MAP_MARKER_TYPE_IS_FAST_TRAVEL_DESTINATION)?,
        is_custom_marker_type: field(names::ids::MAP_MARKER_TYPE_IS_CUSTOM)?,
        show_above_fog_of_war: field(names::ids::MAP_MARKER_TYPE_SHOW_ABOVE_FOG)?,
        show_label: field(names::ids::MAP_MARKER_TYPE_SHOW_LABEL)?,
    })
    .filter(TypesLayout::is_sound)
}

/// Reads the one marker type registry the client's archive holds.
fn read_marker_types(loader: &Loader) -> Result<Vec<MapMarkerTypeInfo>, ReadTypesError> {
    let layout = types_layout(loader).ok_or(ReadTypesError::Layout)?;
    let hash = names::type_hash(names::ids::MAP_MARKER_REGISTRY).ok_or(ReadTypesError::Layout)?;
    let groups: Vec<(u8, MarkerTypeSorting)> = GROUPS
        .iter()
        .filter_map(|&(id, sorting)| {
            let value = crate::engine::catalogue::constant(id, &loader.reflection)?;
            Some((u8::try_from(value).ok()?, sorting))
        })
        .collect();
    let archive = crate::engine::archive::game().ok_or(ReadTypesError::Archive)?;
    let keys = archive.keys_of(hash);
    let [key] = keys.as_slice() else {
        return Err(if keys.is_empty() {
            ReadTypesError::NoRegistry
        } else {
            ReadTypesError::Registries(keys.len())
        });
    };
    let registry = archive.resource(key).ok_or(ReadTypesError::Unreadable)?;
    types(&layout, &groups, &registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asks `cache` for `key` under `fuse` until the read is done.
    fn settled<V: Described, E: SceneFailure>(
        key: SceneKey,
        fuse: &'static Fuse,
        cache: fn(&mut SceneState) -> &mut SceneCache<V, E>,
        read: impl Fn() -> Result<V, E> + Clone + Send + 'static,
    ) -> Result<V, Status> {
        let began = Instant::now();
        loop {
            let found = cached(key, fuse, "build 1", cache, read.clone());
            if found.as_ref().err() != Some(&Status::NOT_LOADED) {
                return found;
            }
            assert!(began.elapsed().as_secs() < 10, "the read never finished");
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_scene_read_that_panics_turns_its_feature_off_and_starts_no_other_read() {
        static MARKS: Fuse = Fuse::new("test scene marks");
        let key = SceneKey::new([1; 16], 2).unwrap();
        let read = || -> Result<Arc<[MapMarkerInfo]>, ReadSceneError> { panic!("a broken read") };
        assert_eq!(
            settled(key, &MARKS, |state| &mut state.markers, read).err(),
            Some(Status::MISSING)
        );
        assert!(MARKS.is_off());
        let began = Instant::now();
        let marks = loop {
            let listed = crate::engine::diag::parts_off();
            if let Some(marks) = listed.into_iter().find(|p| p.part == "test scene marks") {
                break marks;
            }
            assert!(began.elapsed().as_secs() < 10, "never listed off");
            std::thread::yield_now();
        };
        assert_eq!(marks.reason, "the read panicked");
        let other = SceneKey::new([2; 16], 2).unwrap();
        let unread = || -> Result<Arc<[MapMarkerInfo]>, ReadSceneError> { unreachable!("off") };
        assert_eq!(
            cached(other, &MARKS, "build 1", |state| &mut state.markers, unread).err(),
            Some(Status::MISSING)
        );
    }

    #[test]
    fn a_scene_without_its_biome_map_leaves_the_map_frame_on_for_the_next_scene() {
        static FRAMES: Fuse = Fuse::new("test scene frames");
        let key = SceneKey::new([3; 16], 2).unwrap();
        let read = || -> Result<MapFrameInfo, ReadFrameError> { Err(ReadFrameError::NoBiomeMap) };
        assert_eq!(
            settled(key, &FRAMES, |state| &mut state.frame, read).err(),
            Some(Status::MISSING)
        );
        assert!(!FRAMES.is_off());
    }

    #[test]
    fn only_a_missing_layout_a_panic_or_no_thread_turns_a_scene_read_off() {
        let off = |why: ReadSceneError| why.off("marks", "build 1").is_some();
        assert!(off(ReadSceneError::Layout) && off(ReadSceneError::Panicked));
        assert!(off(ReadSceneError::Thread) && !off(ReadSceneError::Chunk));
        assert!(!off(ReadSceneError::Archive) && !off(ReadSceneError::Templates));
        let frame_off = |why: ReadFrameError| why.off("frames", "build 1").is_some();
        assert!(frame_off(ReadFrameError::Layout) && !frame_off(ReadFrameError::BiomeMap));
    }
}
