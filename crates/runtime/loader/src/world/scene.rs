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
    layout: OnceLock<Result<SceneLayout, Lacks>>,
    frame_layout: OnceLock<Result<FrameLayout, Lacks>>,
    markers: SceneCache<Arc<[MapMarkerInfo]>, ReadSceneError>,
    frame: SceneCache<MapFrameInfo, ReadFrameError>,
    /// The client's marker types, once read; `None` when they do not read.
    marker_types: OnceLock<Option<Arc<[MapMarkerTypeInfo]>>>,
    /// The hosted game the caches belong to; [`forget`] moves it on, so a
    /// read started in an ended game cannot land in the next one's cache.
    generation: u64,
}

static STATE: Mutex<SceneState> = Mutex::new(SceneState {
    layout: OnceLock::new(),
    frame_layout: OnceLock::new(),
    markers: SceneCache::new(),
    frame: SceneCache::new(),
    marker_types: OnceLock::new(),
    generation: 0,
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

/// A catalogue entry a layout lacks or that does not fit its parse, and
/// why.
type Lacks = (&'static str, &'static str);

/// Why an entry whose offsets read is still left out.
const UNSOUND: &str = "does not fit the parse";

/// The offset of the catalogue's field `id`, or what the layout lacks.
fn field_of(loader: &Loader, id: &'static str) -> Result<usize, Lacks> {
    field(loader, id).ok_or((id, crate::engine::builds::ABSENT))
}

/// The size of the catalogue's type `id`, or what the layout lacks.
fn size_of(loader: &Loader, id: &'static str) -> Result<usize, Lacks> {
    size(loader, id).ok_or((id, crate::engine::builds::ABSENT))
}

/// The reflected layout the marker parse reads, from the catalogue's facts,
/// or the first entry it lacks.
fn layout(loader: &Loader) -> Result<SceneLayout, Lacks> {
    let field = |id| field_of(loader, id);
    let size = |id| size_of(loader, id);
    let [x0, x1, y0, y1, z0, z1] = names::SCENE_TRANSFORM_POSITION;
    Ok(SceneLayout {
        templates: field(names::ids::SCENE_CHUNK_TEMPLATES)?,
        entities: field(names::ids::SCENE_CHUNK_ENTITIES)?,
        entry_size: size(names::ids::SCENE_SPAWN_ENTRY)?,
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
        marker_size: size(names::ids::MAP_MARKER)?,
    })
    .and_then(|layout| {
        layout
            .is_sound()
            .then_some(layout)
            .ok_or((names::ids::SCENE_SPAWN_ENTRY, UNSOUND))
    })
}

/// The reflected layout the frame parse reads, from the catalogue's facts,
/// or the first entry it lacks.
fn frame_layout(loader: &Loader) -> Result<FrameLayout, Lacks> {
    let field = |id| field_of(loader, id);
    let size = |id| size_of(loader, id);
    Ok(FrameLayout {
        has_biome_map: field(names::ids::SCENE_RESOURCE_HAS_BIOME_MAP)?,
        scene_size: size(names::ids::SCENE_RESOURCE)?,
        world_origin: field(names::ids::BIOME_MAP_WORLD_ORIGIN)?,
        world_size: field(names::ids::BIOME_MAP_WORLD_SIZE)?,
        map_scale: field(names::ids::BIOME_MAP_MAP_SCALE)?,
        biome_map_size: size(names::ids::BIOME_MAP)?,
    })
    .and_then(|layout| {
        layout
            .is_sound()
            .then_some(layout)
            .ok_or((names::ids::BIOME_MAP, UNSOUND))
    })
}

/// The layout `found` holds, or [`Status::MISSING`] with `fuse` turned off
/// for the entry it lacks.
fn sound<L>(loader: &Loader, fuse: &'static Fuse, found: Result<L, Lacks>) -> Result<L, Status> {
    found.map_err(|(id, why)| {
        if !fuse.is_off() {
            let _off = loader.fuse_off(fuse, id, why);
        }
        Status::MISSING
    })
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
    layout: SceneLayout,
    key: SceneKey,
) -> Result<Arc<[MapMarkerInfo]>, ReadSceneError> {
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
fn read_frame(layout: FrameLayout, key: SceneKey) -> Result<MapFrameInfo, ReadFrameError> {
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

/// A failed read of a scene: a fault of the loader's read turns its
/// feature off; a scene's own data, or a thread that did not start, leaves
/// the next scene to read again.
trait SceneFailure: ReadFailure + Clone {
    /// Whether the read itself failed: it panicked, or its parse disagrees
    /// with the layout the feature checked before it started.
    fn is_fault(&self) -> bool;
}

impl SceneFailure for ReadSceneError {
    fn is_fault(&self) -> bool {
        matches!(self, Self::Layout | Self::Panicked)
    }
}

impl SceneFailure for ReadFrameError {
    fn is_fault(&self) -> bool {
        matches!(self, Self::Layout | Self::Panicked)
    }
}

/// The switch of the markers the scene places, read from its data; apart
/// from the interface's `map markers`, which draws mods' own.
static MARKERS_FUSE: Fuse = Fuse::new("world map markers");
/// The switch of the map's frame.
static FRAME_FUSE: Fuse = Fuse::new("map frame");
/// The switch of the client's marker types.
static TYPES_FUSE: Fuse = Fuse::new("map marker types");

/// The read of `key` that `cache` keeps, or [`Status::NOT_LOADED`] while it
/// runs: the first call for a scene starts `read` on a thread of its own,
/// named for `fuse`'s feature. A fault of the read turns the feature off.
fn cached<V: Described, E: SceneFailure>(
    key: SceneKey,
    fuse: &'static Fuse,
    cache: fn(&mut SceneState) -> &mut SceneCache<V, E>,
    read: impl FnOnce() -> Result<V, E> + Send + 'static,
) -> Result<V, Status> {
    if fuse.is_off() {
        return Err(Status::MISSING);
    }
    let mut state = STATE.relock();
    let generation = state.generation;
    match cache(&mut state).lookup(key) {
        Lookup::Ready(found) => return found.map_err(|_| Status::MISSING),
        Lookup::Pending => return Err(Status::NOT_LOADED),
        Lookup::Start => {}
    }
    drop(state);
    let what = fuse.feature();
    crate::engine::off_thread::read(what, read, move |found, took| {
        match &found {
            Ok(value) => crate::log!(DEBUG, "{}", value.done(took.as_millis())),
            Err(why) if why.is_fault() => fuse.turn_off(&Off::fault(what, &why.to_string())),
            Err(why) => crate::log!(WARN, "{what}: this scene does not read: {why}"),
        }
        let _kept = land(key, generation, cache, found);
    });
    Err(Status::NOT_LOADED)
}

/// Keeps the read of `key` started in `generation`, unless its hosted game
/// has ended since; whether it was kept.
fn land<V: Described, E: SceneFailure>(
    key: SceneKey,
    generation: u64,
    cache: fn(&mut SceneState) -> &mut SceneCache<V, E>,
    found: Result<V, E>,
) -> bool {
    let mut state = STATE.relock();
    state.generation == generation && cache(&mut state).finish(key, found)
}

/// The markers of the scene the server runs; on its game thread at a quiet
/// moment, since the scene comes from the running world. The first call
/// for a scene starts reading the archive on a thread of its own and
/// answers [`Status::NOT_LOADED`] until it is done.
pub(crate) fn markers(loader: &Loader) -> Result<Arc<[MapMarkerInfo]>, Status> {
    let key = scene_key(loader)?;
    let found = *STATE.relock().layout.get_or_init(|| layout(loader));
    let layout = sound(loader, &MARKERS_FUSE, found)?;
    cached(
        key,
        &MARKERS_FUSE,
        |state| &mut state.markers,
        move || read_markers(layout, key),
    )
}

/// The map's frame of the scene the server runs; on its game thread at a
/// quiet moment, read like [`markers`].
pub(crate) fn frame_info(loader: &Loader) -> Result<MapFrameInfo, Status> {
    let key = scene_key(loader)?;
    let found = *STATE
        .relock()
        .frame_layout
        .get_or_init(|| frame_layout(loader));
    let layout = sound(loader, &FRAME_FUSE, found)?;
    cached(
        key,
        &FRAME_FUSE,
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
    let known = state
        .marker_types
        .get_or_init(|| kept_types(loader, &TYPES_FUSE, || read_marker_types(loader)));
    known.clone().ok_or(Status::MISSING)
}

/// The marker types `read` gives, run under `fuse`; `None`, with `fuse`
/// off, when they do not read or `read` panics.
fn kept_types(
    loader: &Loader,
    fuse: &'static Fuse,
    read: impl FnOnce() -> Result<Vec<MapMarkerTypeInfo>, ReadTypesError>,
) -> Option<Arc<[MapMarkerTypeInfo]>> {
    let started = Instant::now();
    match fuse.run("the marker types read", read)? {
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
            let _off = loader.fuse_off(fuse, names::ids::MAP_MARKER_REGISTRY, &why.to_string());
            None
        }
    }
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

/// Forgets the markers and the frame read for the last scene, when a hosted
/// game ends: the next world's served data may differ. A read still under
/// way is dropped, even when the next game asks for the same scene; the
/// build's layouts stay.
pub(crate) fn forget() {
    let mut state = STATE.relock();
    state.markers = SceneCache::new();
    state.frame = SceneCache::new();
    state.generation = state.generation.wrapping_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test at a time over the scene caches, which are the process's own.
    static TURN: Mutex<()> = Mutex::new(());

    #[test]
    fn a_scene_read_once_its_hosted_game_ended_is_dropped_and_read_again() {
        let _turn = TURN.relock();
        let key = SceneKey::new([7; 16], 3).unwrap();
        let mut state = STATE.relock();
        assert!(matches!(state.frame.lookup(key), Lookup::Start));
        drop(state);
        forget();
        let mut state = STATE.relock();
        assert!(
            !state.frame.finish(key, Err(ReadFrameError::Scene)),
            "the read of the ended game's scene is dropped"
        );
        assert!(
            matches!(state.frame.lookup(key), Lookup::Start),
            "and asked again"
        );
    }

    #[test]
    fn a_read_of_an_ended_game_does_not_land_when_the_next_game_asks_for_the_same_scene() {
        let _turn = TURN.relock();
        let key = SceneKey::new([8; 16], 3).unwrap();
        let ended = STATE.relock().generation;
        assert!(matches!(STATE.relock().frame.lookup(key), Lookup::Start));
        forget();
        let next = STATE.relock().generation;
        assert!(matches!(STATE.relock().frame.lookup(key), Lookup::Start));
        let frame: fn(&mut SceneState) -> &mut SceneCache<MapFrameInfo, ReadFrameError> =
            |state| &mut state.frame;
        assert!(
            !land(key, ended, frame, Err(ReadFrameError::Scene)),
            "the ended game's read is dropped"
        );
        assert!(
            land(key, next, frame, Err(ReadFrameError::BiomeMap)),
            "the next game's read lands"
        );
        assert!(matches!(
            STATE.relock().frame.lookup(key),
            Lookup::Ready(Err(ReadFrameError::BiomeMap))
        ));
    }

    /// Asks `cache` for `key` under `fuse` until the read is done.
    fn settled<V: Described, E: SceneFailure>(
        key: SceneKey,
        fuse: &'static Fuse,
        cache: fn(&mut SceneState) -> &mut SceneCache<V, E>,
        read: impl Fn() -> Result<V, E> + Clone + Send + 'static,
    ) -> Result<V, Status> {
        let began = Instant::now();
        loop {
            let found = cached(key, fuse, cache, read.clone());
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
        let _turn = TURN.relock();
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
            cached(other, &MARKS, |state| &mut state.markers, unread).err(),
            Some(Status::MISSING)
        );
    }

    #[test]
    fn a_scene_without_its_biome_map_leaves_the_map_frame_on_for_the_next_scene() {
        static FRAMES: Fuse = Fuse::new("test scene frames");
        let _turn = TURN.relock();
        let key = SceneKey::new([3; 16], 2).unwrap();
        let read = || -> Result<MapFrameInfo, ReadFrameError> { Err(ReadFrameError::NoBiomeMap) };
        assert_eq!(
            settled(key, &FRAMES, |state| &mut state.frame, read).err(),
            Some(Status::MISSING)
        );
        assert!(!FRAMES.is_off());
    }

    #[test]
    fn only_a_fault_of_the_read_turns_a_scene_feature_off_and_a_thread_that_did_not_start_retries()
    {
        assert!(ReadSceneError::Layout.is_fault() && ReadSceneError::Panicked.is_fault());
        assert!(!ReadSceneError::Thread.is_fault() && !ReadSceneError::Chunk.is_fault());
        assert!(!ReadSceneError::Archive.is_fault() && !ReadSceneError::Templates.is_fault());
        assert!(ReadFrameError::Panicked.is_fault() && !ReadFrameError::Thread.is_fault());
        assert!(!ReadFrameError::BiomeMap.is_fault());
    }

    #[test]
    fn a_scene_read_whose_thread_does_not_start_leaves_its_feature_on() {
        static FRAMES: Fuse = Fuse::new("test scene threads");
        let _turn = TURN.relock();
        let key = SceneKey::new([4; 16], 2).unwrap();
        let read = || -> Result<MapFrameInfo, ReadFrameError> { Err(ReadFrameError::Thread) };
        assert_eq!(
            settled(key, &FRAMES, |state| &mut state.frame, read).err(),
            Some(Status::MISSING)
        );
        assert!(!FRAMES.is_off());
    }

    #[test]
    fn a_scene_layout_the_build_lacks_turns_its_feature_off_naming_the_first_entry() {
        static LAID: Fuse = Fuse::new("test scene layout");
        let loader = crate::start::tests::bare_loader();
        assert_eq!(
            sound(&loader, &LAID, layout(&loader)).err(),
            Some(Status::MISSING)
        );
        assert!(LAID.is_off());
        let listed = crate::engine::diag::parts_off();
        let laid = listed.iter().find(|p| p.part == "test scene layout");
        assert!(
            laid.is_some_and(|p| p.reason.starts_with(names::ids::SCENE_CHUNK_TEMPLATES)),
            "names the entry, not a placeholder: {listed:?}"
        );
    }

    #[test]
    fn a_marker_types_read_that_panics_turns_them_off_instead_of_unwinding() {
        static KINDS: Fuse = Fuse::new("test marker types");
        let loader = crate::start::tests::bare_loader();
        let read = || -> Result<Vec<MapMarkerTypeInfo>, ReadTypesError> { panic!("a broken read") };
        assert!(kept_types(&loader, &KINDS, read).is_none());
        assert!(KINDS.is_off());
        assert!(
            crate::engine::diag::parts_off()
                .iter()
                .any(|p| p.part == "test marker types"),
            "listed for the Mods screen"
        );
    }
}
