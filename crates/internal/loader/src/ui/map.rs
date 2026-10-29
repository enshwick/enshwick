//! Mod markers on the client's map.
//!
//! Engine facts from clients 410 and 426: every frame the ECS system
//! `replicate_map_markers_for_ui` fills a list of the map's markers in the
//! interface's play data, which the clear system empties first. The loader
//! appends the mods' markers to that list right after the game fills it, on
//! the same thread, so a marker a mod drops is gone the next frame. A marker
//! shows only with a marker type the game's registry knows; the loader picks
//! a custom marker type with a label from the registry once it loads.
//!
//! The list, its entries and the walk to them come from the build table,
//! the marker types from reflection; map markers stay off where either
//! lacks one.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwap;
use enshwick::abi::{HookFlags, Status, UiMarker};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;

use enshwick_gamedata::catalogue;

use crate::chains::Next;
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned::{self, Constant};

use super::engine::{Buf, UiText};

/// The most markers one mod shows.
const MAX_MARKERS: usize = 256;
/// Frames an old marker set stays alive after it was replaced, since the map
/// may still read a label it was handed the frame before.
const RETIRED_FRAMES: u64 = 2;
/// Old marker sets kept at most while no frame runs to retire them.
const MAX_RETIRED: usize = 64;

/// Room for one marker entry, which the build table's entry size must fit.
const ENTRY_ROOM: usize = 0x100;

/// Where the map's markers lie, from the build table and reflection.
#[derive(Debug)]
struct Layout {
    /// The play data's main marker list, and its `u64` count and capacity
    /// from the list.
    list: usize,
    list_count: usize,
    list_capacity: usize,
    /// A marker entry and its fields.
    entry_size: usize,
    entry_position: usize,
    entry_type: usize,
    entry_visibility: usize,
    entry_label: usize,
    entry_color: usize,
    /// The visibility of a marker the map shows.
    visible: u8,
    /// The marker type registry in the play data, its blob, and the blob's
    /// type array `{i32 offset from the array, u32 count}`.
    registry: usize,
    registry_blob: usize,
    blob_types: usize,
    /// A `MapMarkerType` entry and its fields.
    type_size: usize,
    type_id: usize,
    type_custom: usize,
    type_above_fog: usize,
    type_label: usize,
    /// The interface context's G38 state and the play data pointer in it.
    context_state: usize,
    state_play_data: usize,
}

impl Layout {
    fn read(loader: &Loader) -> Result<Self, LayoutError> {
        let table = |id: Constant| loader.offset(id).ok_or(LayoutError::Table(id));
        let reflected = |id: &'static str| {
            catalogue::constant(id, &loader.reflection)
                .and_then(|v| usize::try_from(v).ok())
                .ok_or(LayoutError::Reflection(id))
        };
        let type_size = loader
            .reflection
            .type_info(catalogue::type_name(catalogue::ids::MAP_MARKER_TYPE))
            .ok()
            .and_then(|info| usize::try_from(info.size()).ok())
            .ok_or(LayoutError::Reflection(catalogue::ids::MAP_MARKER_TYPE))?;
        Ok(Self {
            list: table(pinned::map::LIST)?,
            list_count: table(pinned::map::LIST_COUNT)?,
            list_capacity: table(pinned::map::LIST_CAPACITY)?,
            entry_size: table(pinned::map::ENTRY_SIZE)?,
            entry_position: table(pinned::map::ENTRY_POSITION)?,
            entry_type: table(pinned::map::ENTRY_TYPE)?,
            entry_visibility: table(pinned::map::ENTRY_VISIBILITY)?,
            entry_label: table(pinned::map::ENTRY_LABEL)?,
            entry_color: table(pinned::map::ENTRY_COLOR)?,
            visible: u8::try_from(reflected(
                catalogue::ids::MAP_MARKER_VISIBILITY_TYPE_VISIBLE,
            )?)
            .map_err(|_| {
                LayoutError::Reflection(catalogue::ids::MAP_MARKER_VISIBILITY_TYPE_VISIBLE)
            })?,
            registry: table(pinned::map::REGISTRY)?,
            registry_blob: table(pinned::map::REGISTRY_BLOB)?,
            blob_types: reflected(catalogue::ids::MAP_MARKER_REGISTRY_TYPES)?,
            type_size,
            type_id: reflected(catalogue::ids::MAP_MARKER_TYPE_MARKER_ID)?,
            type_custom: reflected(catalogue::ids::MAP_MARKER_TYPE_IS_CUSTOM)?,
            type_above_fog: reflected(catalogue::ids::MAP_MARKER_TYPE_SHOW_ABOVE_FOG)?,
            type_label: reflected(catalogue::ids::MAP_MARKER_TYPE_SHOW_LABEL)?,
            context_state: table(pinned::map::CONTEXT_STATE)?,
            state_play_data: table(pinned::map::STATE_PLAY_DATA)?,
        })
    }

    /// Whether an entry fits [`ENTRY_ROOM`] and holds every field the loader
    /// writes, and a marker type every field it reads.
    fn fits(&self) -> bool {
        let entry_ends = [
            self.entry_position.saturating_add(3 * 8),
            self.entry_type.saturating_add(4),
            self.entry_visibility.saturating_add(1),
            self.entry_label.saturating_add(3 * 8),
            self.entry_color.saturating_add(1),
        ];
        let type_ends = [
            self.type_id.saturating_add(4),
            self.type_custom.saturating_add(1),
            self.type_above_fog.saturating_add(1),
            self.type_label.saturating_add(1),
        ];
        self.entry_size <= ENTRY_ROOM
            && entry_ends.iter().all(|&end| end <= self.entry_size)
            && type_ends.iter().all(|&end| end <= self.type_size)
    }
}

/// Why the map markers' layout does not read on this build.
#[derive(Debug, thiserror::Error)]
enum LayoutError {
    /// The build table lacks a row.
    #[error("{0} is not in this build")]
    Table(Constant),
    /// The game's reflection lacks a fact, or gives one out of range.
    #[error("the game does not reflect {0}")]
    Reflection(&'static str),
}

/// The map markers' state: the layout, the mods' markers and what the
/// replicate system's frames noted.
struct Markers {
    /// The layout [`link`] read, once the build has every part of it.
    layout: OnceLock<Layout>,
    /// Each mod's markers, by owner.
    sets: Mutex<Vec<(usize, Arc<[Owned]>)>>,
    /// Every mod's markers, swapped whole for the system's thread.
    snapshot: OnceLock<ArcSwap<Sets>>,
    /// Replaced marker sets, with the frame they were replaced in.
    retired: Mutex<VecDeque<(u64, Arc<Sets>)>>,
    /// Frames the replicate system has run.
    frame: AtomicU64,
    /// The interface's play data, as the last frame found it; 0 outside a
    /// world.
    play_data: AtomicUsize,
    /// When a frame last found it, in milliseconds since the system
    /// started.
    noted_at: AtomicU64,
    /// The marker type mod markers show with, once the registry is read.
    marker_type: OnceLock<Option<u32>>,
}

static MARKERS: Markers = Markers {
    layout: OnceLock::new(),
    sets: Mutex::new(Vec::new()),
    snapshot: OnceLock::new(),
    retired: Mutex::new(VecDeque::new()),
    frame: AtomicU64::new(0),
    play_data: AtomicUsize::new(0),
    noted_at: AtomicU64::new(0),
    marker_type: OnceLock::new(),
};

/// One marker, owned by the loader.
struct Owned {
    owner: ModIndex,
    position: [i64; 3],
    label: Box<[u8]>,
    color: u8,
}

type SystemFn = unsafe extern "system" fn(context: *mut u8);

// SAFETY: `SystemFn` is the exact signature of the reflected ECS system
// `replicate_map_markers_for_ui`, and the slot is used only as the `next` of
// the `detour` link on that system's entry.
static NEXT: Next<SystemFn> = unsafe { Next::new() };
/// Every mod's markers, one shared set each.
type Sets = Vec<Arc<[Owned]>>;
/// How old the play data may be: frames that stop noting it, such as under a
/// loading screen between two worlds, leave it to a world that may be gone.
const FRESH_MS: u64 = 500;
/// More marker types than any build has: a count past it is a misread.
const MAX_TYPES: u32 = 4096;

fn snapshot() -> &'static ArcSwap<Sets> {
    MARKERS
        .snapshot
        .get_or_init(|| ArcSwap::from_pointee(Vec::new()))
}

/// Replaces mod `owner`'s markers with `markers`.
pub(crate) fn set(owner: ModIndex, given: &[UiMarker]) -> Status {
    if given.len() > MAX_MARKERS {
        return Status::BAD_ARGS;
    }
    let copied: Vec<Owned> = given
        .iter()
        .map(|m| Owned {
            owner,
            position: m.position,
            // SAFETY: the mod's label is borrowed for this call and copied
            // here before `set` returns, so it outlives every use.
            label: unsafe { m.label.as_str() }.unwrap_or("").as_bytes().into(),
            color: u8::try_from(m.color.min(UiMarker::BLACK)).unwrap_or(0),
        })
        .collect();
    let mut sets = MARKERS.sets.relock();
    sets.retain(|(o, _)| *o != owner.get());
    if !copied.is_empty() {
        sets.push((owner.get(), copied.into()));
    }
    let next: Sets = sets.iter().map(|(_, list)| Arc::clone(list)).collect();
    drop(sets);
    let old = snapshot().swap(Arc::new(next));
    let mut retired = MARKERS.retired.relock();
    retired.push_back((MARKERS.frame.load(Ordering::Acquire), old));
    retire(&mut retired, MARKERS.frame.load(Ordering::Acquire));
    Status::OK
}

/// Frees the replaced sets no frame can still read at frame `now`, and the
/// oldest beyond [`MAX_RETIRED`].
fn retire(retired: &mut VecDeque<(u64, Arc<Sets>)>, now: u64) {
    while retired
        .front()
        .is_some_and(|&(at, _)| at + RETIRED_FRAMES <= now || retired.len() > MAX_RETIRED)
    {
        retired.pop_front();
    }
}

/// Notes the play data of the interface context `context`, for the
/// system's thread.
pub(super) fn note(context: usize) {
    let (Some(_), Some(layout)) = (NEXT.wired(), MARKERS.layout.get()) else {
        return;
    };
    // SAFETY: the one caller passes the live interface context of the frame
    // it draws, on the interface thread, which keeps its G38 state pointer
    // at the build table's `map.context_state`.
    let state = unsafe { crate::memory::read_ptr::<u8>(context, layout.context_state) };
    let play = if state.is_null() {
        core::ptr::null_mut()
    } else {
        // SAFETY: `state` is the context's non-null G38 state (checked
        // earlier), whose first word is the play state pointer or null
        // (clients 410 and 426).
        unsafe { crate::memory::read_ptr::<u8>(state.expose_provenance(), 0) }
    };
    let data = if play.is_null() {
        0
    } else {
        // SAFETY: `play` is the non-null play state (checked earlier), which
        // holds its play data pointer at the build table's
        // `map.state_play_data`.
        unsafe { crate::memory::read_ptr::<u8>(play.expose_provenance(), layout.state_play_data) }
            .expose_provenance()
    };
    MARKERS.play_data.store(data, Ordering::Release);
    // SAFETY: `GetTickCount64` takes no arguments and has no preconditions.
    MARKERS
        .noted_at
        .store(unsafe { GetTickCount64() }, Ordering::Release);
}

/// Links the replicate system on the client, on a build whose table and
/// reflection give the markers' whole layout.
pub(crate) fn link(loader: &Loader) {
    let layout = match Layout::read(loader) {
        Ok(layout) if layout.fits() => layout,
        Ok(_) => {
            crate::log!(
                WARN,
                "map markers off: this build's marker entries do not fit the loader's"
            );
            return;
        }
        Err(why) => {
            crate::log!(WARN, "map markers off: {why}");
            return;
        }
    };
    let _set = MARKERS.layout.set(layout);
    let target = loader
        .reflection
        .system("replicate_map_markers_for_ui")
        .ok()
        .map(enshwick_gamedata::reflect::System::entry);
    loader.link_feature(
        "map markers",
        target,
        &NEXT,
        detour as SystemFn,
        HookFlags::NONE,
    );
}

/// Turns the mods' map markers off at their first panic.
static MARKERS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("map markers");

unsafe extern "system" fn detour(context: *mut u8) {
    // SAFETY: the game's own argument, passed on unchanged to the rest of the
    // chain, which has the replicate system's signature.
    unsafe { NEXT.get()(context) };
    let _appended = MARKERS_FUSE.run("adding the markers", append);
    let now = MARKERS.frame.fetch_add(1, Ordering::AcqRel) + 1;
    if let Some(mut retired) = MARKERS.retired.try_relock() {
        retire(&mut retired, now);
    }
}

fn append() {
    let sets = snapshot().load();
    let Some(layout) = MARKERS.layout.get() else {
        return;
    };
    if sets.is_empty() {
        return;
    }
    let data = MARKERS.play_data.load(Ordering::Acquire);
    // SAFETY: `GetTickCount64` takes no arguments and has no preconditions.
    let age = unsafe { GetTickCount64() }.saturating_sub(MARKERS.noted_at.load(Ordering::Acquire));
    if data == 0 || age > FRESH_MS {
        return;
    }
    // SAFETY: `data` is the non-zero play data `note` found under 500 ms ago
    // (checked earlier), and `append` runs inside the replicate system's
    // detour, on its thread while it runs.
    let Some(kind) = (unsafe { marker_type(layout, data) }) else {
        return;
    };
    let list = data + layout.list;
    // SAFETY: `data` is the non-zero play data `note` found under 500 ms ago
    // (checked earlier), read on the replicate system's thread, which owns it
    // while the system runs; it keeps the marker list's entry pointer at the
    // build table's `map.list`.
    let entries = unsafe { crate::memory::read_ptr::<u8>(list, 0) };
    // SAFETY: the live play data's marker list, checked fresh and non-zero
    // earlier, on the replicate system's thread; its `u64` count sits at the
    // build table's `map.list_count`.
    let mut count: u64 = unsafe { crate::memory::read(list, layout.list_count) };
    // SAFETY: the live play data's marker list, checked fresh and non-zero
    // earlier, on the replicate system's thread; its `u64` capacity sits at
    // the build table's `map.list_capacity`.
    let capacity: u64 = unsafe { crate::memory::read(list, layout.list_capacity) };
    if entries.is_null() {
        return;
    }
    for marker in sets.iter().flat_map(|set| set.iter()) {
        if count >= capacity {
            break;
        }
        if crate::host::is_off(marker.owner) {
            continue;
        }
        let Ok(index) = usize::try_from(count) else {
            break;
        };
        let mut entry = Buf::<ENTRY_ROOM>::zeroed();
        for (i, axis) in marker.position.iter().enumerate() {
            entry.u64(layout.entry_position + 8 * i, axis.cast_unsigned());
        }
        entry.u32(layout.entry_type, kind);
        entry.u8(layout.entry_visibility, layout.visible);
        // The label lives in the snapshot, kept among the retired sets after it
        // is replaced.
        entry.text(layout.entry_label, &UiText::new(&marker.label));
        entry.u8(layout.entry_color, marker.color);
        let slot = entries.wrapping_add(index * layout.entry_size);
        // SAFETY: `index` is the list's count, below its capacity by the
        // earlier check, so `slot` is a free entry of the table's
        // `map.entry_size` bytes in the list's non-null storage; `link` took
        // the layout only where that size fits `ENTRY_ROOM`, the loader's own
        // buffer `entry`; the replicate system's thread owns the list while
        // the system runs.
        unsafe { core::ptr::copy_nonoverlapping(entry.ptr(), slot, layout.entry_size) };
        count += 1;
    }
    // SAFETY: the live play data's marker list, checked fresh and non-zero
    // earlier, on the replicate system's thread, which owns it while the
    // system runs; its `u64` count sits at the build table's
    // `map.list_count`.
    unsafe { crate::memory::write(list, layout.list_count, count) };
}

/// The marker type for mod markers: the registry's first custom marker type
/// with a label, else its first type with a label shown above the fog;
/// `None` until the registry has loaded, and for good when it has none.
///
/// # Safety
///
/// - `data` is the address of the interface's live play data, of the build
///   `layout` was read for.
/// - The call runs on the replicate system's thread while the system runs,
///   so nothing writes the play data or its registry during the call.
unsafe fn marker_type(layout: &Layout, data: usize) -> Option<u32> {
    if let Some(found) = MARKERS.marker_type.get() {
        return *found;
    }
    // SAFETY: the caller's contract makes `data` the live play data, read on
    // the replicate system's thread; it keeps the marker type registry
    // pointer at the build table's `map.registry`.
    let registry = unsafe { crate::memory::read_ptr::<u8>(data, layout.registry) };
    if registry.is_null() {
        return None;
    }
    // SAFETY: `registry` is the non-null marker type registry (checked
    // earlier), which holds its blob pointer at the build table's
    // `map.registry_blob`.
    let blob = unsafe {
        crate::memory::read_ptr::<u8>(registry.expose_provenance(), layout.registry_blob)
    };
    if blob.is_null() {
        return None;
    }
    let array = blob.expose_provenance() + layout.blob_types;
    // SAFETY: the non-null blob (checked earlier) holds its type array at the
    // reflected `MapMarkerRegistryResource` path; a blob array starts with
    // its `i32` offset from itself, the game's resource format.
    let offset: i32 = unsafe { crate::memory::read(array, 0) };
    // SAFETY: the same blob array, whose `u32` count follows its offset.
    let count: u32 = unsafe { crate::memory::read(array, 4) };
    if offset == 0 || count == 0 || count > MAX_TYPES {
        return None;
    }
    let first = array.wrapping_add_signed(offset as isize);
    let types: Vec<(u32, bool, bool, bool)> = (0..count as usize)
        .map(|i| {
            let at = first + i * layout.type_size;
            // SAFETY: the blob's type array holds `count` reflected
            // `MapMarkerType` entries from `first` (count bounded by
            // `MAX_TYPES` earlier) and `i` is below `count`; each keeps its
            // reflected `u32` id inside the entry (`Layout::fits`).
            let id: u32 = unsafe { crate::memory::read(at, layout.type_id) };
            // SAFETY: entry `i` of the blob's type array, `i` below `count`;
            // its reflected custom flag is a byte inside the entry, read as a
            // byte and compared.
            let custom: u8 = unsafe { crate::memory::read(at, layout.type_custom) };
            // SAFETY: entry `i` of the blob's type array, `i` below `count`;
            // its reflected label flag is a byte inside the entry, read as a
            // byte and compared.
            let label: u8 = unsafe { crate::memory::read(at, layout.type_label) };
            // SAFETY: entry `i` of the blob's type array, `i` below `count`;
            // its reflected above-fog flag is a byte inside the entry, read as
            // a byte and compared.
            let above_fog: u8 = unsafe { crate::memory::read(at, layout.type_above_fog) };
            (id, custom != 0, label != 0, above_fog != 0)
        })
        .collect();
    let chosen = pick(&types);
    let listed: Vec<String> = types
        .iter()
        .map(|(id, custom, label, fog)| {
            format!(
                "{id:08x}{}{}{}",
                flag(*custom, "c"),
                flag(*label, "l"),
                flag(*fog, "f")
            )
        })
        .collect();
    crate::log!(
        INFO,
        "map markers: {} marker types ({}); mod markers use {}",
        types.len(),
        listed.join(" "),
        chosen.map_or_else(|| "none".to_owned(), |id| format!("{id:08x}"))
    );
    *MARKERS.marker_type.get_or_init(|| chosen)
}

fn flag(on: bool, letter: &'static str) -> &'static str {
    if on { letter } else { "" }
}

/// From `(id, custom, shows a label, shows above the fog)`.
fn pick(types: &[(u32, bool, bool, bool)]) -> Option<u32> {
    types
        .iter()
        .find(|(_, custom, label, _)| *custom && *label)
        .or_else(|| types.iter().find(|(_, _, label, fog)| *label && *fog))
        .map(|(id, ..)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout of client 426, from its table and reflection.
    fn client_426() -> Layout {
        Layout {
            list: 0x36_ea00,
            list_count: 0x8,
            list_capacity: 0x10,
            entry_size: 0xa0,
            entry_position: 0x10,
            entry_type: 0x30,
            entry_visibility: 0x34,
            entry_label: 0x50,
            entry_color: 0x80,
            visible: 2,
            registry: 0x46_67f0,
            registry_blob: 0x20,
            blob_types: 0x10,
            type_size: 0x50,
            type_id: 0x10,
            type_custom: 0x19,
            type_above_fog: 0x40,
            type_label: 0x41,
            context_state: 0xd0,
            state_play_data: 0x38,
        }
    }

    #[test]
    fn a_layout_whose_entries_hold_every_field_fits() {
        assert!(client_426().fits());
    }

    #[test]
    fn an_entry_or_field_past_the_room_turns_markers_off() {
        let too_big = Layout {
            entry_size: ENTRY_ROOM + 0x10,
            ..client_426()
        };
        assert!(!too_big.fits());
        let label_past = Layout {
            entry_label: 0x90,
            ..client_426()
        };
        assert!(!label_past.fits(), "the label's three words end past 0xa0");
        let flag_past = Layout {
            type_label: 0x50,
            ..client_426()
        };
        assert!(!flag_past.fits(), "a marker type flag past the type");
        let overflowing = Layout {
            entry_label: usize::MAX,
            ..client_426()
        };
        assert!(!overflowing.fits(), "a field whose end overflows");
    }

    #[test]
    fn a_custom_type_with_a_label_wins() {
        let types = [
            (1, false, true, true),
            (2, true, false, true),
            (3, true, true, false),
        ];
        assert_eq!(pick(&types), Some(3));
        assert_eq!(pick(&types[..2]), Some(1));
        assert_eq!(pick(&[(4, true, false, false)]), None);
    }

    #[test]
    fn a_replaced_set_lives_two_frames_however_often_mods_replace_theirs() {
        let mut retired = VecDeque::new();
        for _ in 0..10 {
            retired.push_back((5, Arc::new(Vec::new())));
            retire(&mut retired, 5);
        }
        assert_eq!(retired.len(), 10);
        retire(&mut retired, 6);
        assert_eq!(retired.len(), 10);
        retire(&mut retired, 7);
        assert!(retired.is_empty());
        for _ in 0..MAX_RETIRED + 5 {
            retired.push_back((9, Arc::new(Vec::new())));
            retire(&mut retired, 9);
        }
        assert_eq!(retired.len(), MAX_RETIRED);
    }
}
