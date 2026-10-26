//! Mod markers on the client's map.
//!
//! Engine facts from client build 1269579: every frame the ECS system
//! `replicate_map_markers_for_ui` fills a list of the map's markers in the
//! interface's play data, which the clear system empties first. The loader
//! appends the mods' markers to that list right after the game fills it, on
//! the same thread, so a marker a mod drops is gone the next frame. A marker
//! shows only with a marker type the game's registry knows; the loader picks
//! a custom marker type with a label from the registry once it loads.
//!
//! The layouts below are pinned for that build alone and no pattern finds
//! them on another, so map markers stay off on a build without its own table.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwap;
use enshwick::abi::{HookFlags, Status, UiMarker};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;

use crate::builds::Kind;
use crate::chains::Next;
use crate::host::Loader;
use crate::locks::Relock;

use super::engine::{Buf, UiText};

/// The most markers one mod shows.
const MAX_MARKERS: usize = 256;
/// Frames an old marker set stays alive after it was replaced, since the map
/// may still read a label it was handed the frame before.
const RETIRED_FRAMES: u64 = 2;
/// Old marker sets kept at most while no frame runs to retire them.
const MAX_RETIRED: usize = 64;

/// The play data's main marker list: entries, count, capacity.
const LIST_DATA: usize = 0x36_ea00;
const LIST_COUNT: usize = 0x36_ea08;
const LIST_CAPACITY: usize = 0x36_ea10;
/// A marker entry and its fields.
const ENTRY_SIZE: usize = 0xa0;
const ENTRY_POSITION: usize = 0x10;
const ENTRY_TYPE: usize = 0x30;
const ENTRY_VISIBILITY: usize = 0x34;
const ENTRY_LABEL: usize = 0x50;
const ENTRY_COLOR: usize = 0x80;
const VISIBLE: u8 = 2;
/// The marker type registry, its blob, and the blob's type array:
/// `{offset from the array, count}`, `MapMarkerType` entries of 0x50 bytes.
const REGISTRY: usize = 0x46_67f0;
const REGISTRY_BLOB: usize = 0x20;
const BLOB_TYPES: usize = 0x10;
const TYPE_SIZE: usize = 0x50;
const TYPE_ID: usize = 0x10;
const TYPE_ABOVE_FOG: usize = 0x40;
const TYPE_LABEL: usize = 0x41;
const TYPE_CUSTOM: usize = 0x19;
/// The interface context's G38 state and the play data pointer in it.
const CONTEXT_STATE: usize = 0xd0;
const STATE_PLAY_DATA: usize = 0x38;

/// One marker, owned by the loader.
struct Owned {
    owner: usize,
    position: [i64; 3],
    label: Box<[u8]>,
    color: u8,
}

type SystemFn = unsafe extern "system" fn(context: *mut u8);

// SAFETY: `SystemFn` is the exact signature of the reflected ECS system
// `replicate_map_markers_for_ui`, and the slot is used only as the `next` of
// the `detour` link on that system's entry.
static NEXT: Next<SystemFn> = unsafe { Next::new() };
/// Each mod's markers, by owner.
static SETS: Mutex<Vec<(usize, Arc<[Owned]>)>> = Mutex::new(Vec::new());
/// Every mod's markers, swapped whole for the system's thread.
static SNAPSHOT: OnceLock<ArcSwap<Sets>> = OnceLock::new();
/// Every mod's markers, one shared set each.
type Sets = Vec<Arc<[Owned]>>;
/// Replaced marker sets, with the frame they were replaced in.
static RETIRED_SETS: Mutex<VecDeque<(u64, Arc<Sets>)>> = Mutex::new(VecDeque::new());
/// Frames the replicate system has run.
static FRAME: AtomicU64 = AtomicU64::new(0);
/// The interface's play data, as the last frame found it; 0 outside a world.
static PLAY_DATA: AtomicUsize = AtomicUsize::new(0);
/// When a frame last found it, in milliseconds since the system started.
static NOTED_AT: AtomicU64 = AtomicU64::new(0);
/// How old the play data may be: frames that stop noting it, such as under a
/// loading screen between two worlds, leave it to a world that may be gone.
const FRESH_MS: u64 = 500;
/// More marker types than any build has: a count past it is a misread.
const MAX_TYPES: u32 = 4096;
/// The marker type mod markers show with, once the registry is read.
static MARKER_TYPE: OnceLock<Option<u32>> = OnceLock::new();

fn snapshot() -> &'static ArcSwap<Sets> {
    SNAPSHOT.get_or_init(|| ArcSwap::from_pointee(Vec::new()))
}

/// Replaces mod `owner`'s markers with `markers`.
pub(crate) fn set(owner: usize, given: &[UiMarker]) -> Status {
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
    let mut sets = SETS.relock();
    sets.retain(|(o, _)| *o != owner);
    if !copied.is_empty() {
        sets.push((owner, copied.into()));
    }
    let next: Sets = sets.iter().map(|(_, list)| Arc::clone(list)).collect();
    drop(sets);
    let old = snapshot().swap(Arc::new(next));
    let mut retired = RETIRED_SETS.relock();
    retired.push_back((FRAME.load(Ordering::Acquire), old));
    retire(&mut retired, FRAME.load(Ordering::Acquire));
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
    if NEXT.wired().is_none() {
        return;
    }
    // SAFETY: the one caller passes the live interface context of the frame
    // it draws, on the interface thread; client build 1269579 keeps its G38
    // state pointer at `CONTEXT_STATE`.
    let state = unsafe { crate::memory::read_ptr::<u8>(context, CONTEXT_STATE) };
    let play = if state.is_null() {
        core::ptr::null_mut()
    } else {
        // SAFETY: `state` is the context's non-null G38 state (checked
        // earlier), whose first word is the play state pointer or null on
        // client build 1269579.
        unsafe { crate::memory::read_ptr::<u8>(state.expose_provenance(), 0) }
    };
    let data = if play.is_null() {
        0
    } else {
        // SAFETY: `play` is the non-null play state (checked earlier), which
        // holds its play data pointer at `STATE_PLAY_DATA` on client build
        // 1269579.
        unsafe { crate::memory::read_ptr::<u8>(play.expose_provenance(), STATE_PLAY_DATA) }
            .expose_provenance()
    };
    PLAY_DATA.store(data, Ordering::Release);
    // SAFETY: `GetTickCount64` takes no arguments and has no preconditions.
    NOTED_AT.store(unsafe { GetTickCount64() }, Ordering::Release);
}

/// Whether the layouts this module writes through hold on a build whose
/// table came about as `kind`.
fn layout_pinned(kind: Kind) -> bool {
    kind == Kind::Exact
}

/// Links the replicate system on the client, on a build with its own table.
pub(crate) fn link(loader: &Loader) {
    if !layout_pinned(loader.build.kind) {
        crate::log!(
            WARN,
            "map markers off: their layout is known only on a build with its own table"
        );
        return;
    }
    let target = loader
        .reflection
        .system("replicate_map_markers_for_ui")
        .ok()
        .map(enshwick_reflect::System::entry);
    loader.link_feature(
        "map markers",
        target,
        &NEXT,
        detour as SystemFn,
        HookFlags::NONE,
    );
}

unsafe extern "system" fn detour(context: *mut u8) {
    // SAFETY: the game's own argument, passed on unchanged to the rest of the
    // chain, which has the replicate system's signature.
    unsafe { NEXT.get()(context) };
    let appended = std::panic::catch_unwind(append);
    if appended.is_err() {
        crate::log!(ERROR, "map markers: the loader panicked while adding them");
    }
    let now = FRAME.fetch_add(1, Ordering::AcqRel) + 1;
    if let Ok(mut retired) = RETIRED_SETS.try_lock() {
        retire(&mut retired, now);
    }
}

fn append() {
    let sets = snapshot().load();
    if sets.is_empty() {
        return;
    }
    let data = PLAY_DATA.load(Ordering::Acquire);
    // SAFETY: `GetTickCount64` takes no arguments and has no preconditions.
    let age = unsafe { GetTickCount64() }.saturating_sub(NOTED_AT.load(Ordering::Acquire));
    if data == 0 || age > FRESH_MS {
        return;
    }
    // SAFETY: `data` is the non-zero play data `note` found under 500 ms ago
    // (checked earlier), and `append` runs inside the replicate system's
    // detour, on its thread while it runs.
    let Some(kind) = (unsafe { marker_type(data) }) else {
        return;
    };
    // SAFETY: `data` is the non-zero play data `note` found under 500 ms ago
    // (checked earlier), read on the replicate system's thread, which owns it
    // while the system runs; client build 1269579 keeps the marker list's
    // entry pointer at `LIST_DATA`.
    let entries = unsafe { crate::memory::read_ptr::<u8>(data, LIST_DATA) };
    // SAFETY: the live play data, checked fresh and non-zero earlier, on the
    // replicate system's thread; client build 1269579 keeps the list's `u64`
    // count at `LIST_COUNT`.
    let mut count: u64 = unsafe { crate::memory::read(data, LIST_COUNT) };
    // SAFETY: the live play data, checked fresh and non-zero earlier, on the
    // replicate system's thread; client build 1269579 keeps the list's `u64`
    // capacity at `LIST_CAPACITY`.
    let capacity: u64 = unsafe { crate::memory::read(data, LIST_CAPACITY) };
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
        let mut entry = Buf::<ENTRY_SIZE>::zeroed();
        for (i, axis) in marker.position.iter().enumerate() {
            entry.u64(ENTRY_POSITION + 8 * i, axis.cast_unsigned());
        }
        entry.u32(ENTRY_TYPE, kind);
        entry.u8(ENTRY_VISIBILITY, VISIBLE);
        // The label lives in the snapshot, kept for `RETIRED` sets after it
        // is replaced.
        entry.text(ENTRY_LABEL, &UiText::new(&marker.label));
        entry.u8(ENTRY_COLOR, marker.color);
        let slot = entries.wrapping_add(index * ENTRY_SIZE);
        // SAFETY: `index` is the list's count, below its capacity by the
        // earlier check, so `slot` is a free entry of `ENTRY_SIZE` bytes in the
        // list's non-null storage; the replicate system's thread owns the list
        // while the system runs, and `entry` is the loader's own buffer.
        unsafe { core::ptr::copy_nonoverlapping(entry.ptr(), slot, ENTRY_SIZE) };
        count += 1;
    }
    // SAFETY: the live play data, checked fresh and non-zero earlier, on the
    // replicate system's thread, which owns it while the system runs; client
    // build 1269579 keeps the list's `u64` count at `LIST_COUNT`.
    unsafe { crate::memory::write(data, LIST_COUNT, count) };
}

/// The marker type for mod markers: the registry's first custom marker type
/// with a label, else its first type with a label shown above the fog;
/// `None` until the registry has loaded, and for good when it has none.
///
/// # Safety
///
/// - `data` is the address of the interface's live play data of client build
///   1269579.
/// - The call runs on the replicate system's thread while the system runs,
///   so nothing writes the play data or its registry during the call.
unsafe fn marker_type(data: usize) -> Option<u32> {
    if let Some(found) = MARKER_TYPE.get() {
        return *found;
    }
    // SAFETY: the caller's contract makes `data` the live play data, read on
    // the replicate system's thread; client build 1269579 keeps the marker
    // type registry pointer at `REGISTRY`.
    let registry = unsafe { crate::memory::read_ptr::<u8>(data, REGISTRY) };
    if registry.is_null() {
        return None;
    }
    // SAFETY: `registry` is the non-null marker type registry (checked
    // earlier), which holds its blob pointer at `REGISTRY_BLOB` on client build
    // 1269579.
    let blob =
        unsafe { crate::memory::read_ptr::<u8>(registry.expose_provenance(), REGISTRY_BLOB) };
    if blob.is_null() {
        return None;
    }
    let array = blob.expose_provenance() + BLOB_TYPES;
    // SAFETY: the non-null blob (checked earlier) holds its type array header
    // at `BLOB_TYPES` on client build 1269579: an `i32` offset first.
    let offset: i32 = unsafe { crate::memory::read(array, 0) };
    // SAFETY: the same type array header in the non-null blob, whose `u32`
    // count follows the `i32` offset on client build 1269579.
    let count: u32 = unsafe { crate::memory::read(array, 4) };
    if offset == 0 || count == 0 || count > MAX_TYPES {
        return None;
    }
    let first = array.wrapping_add_signed(offset as isize);
    let types: Vec<(u32, bool, bool, bool)> = (0..count as usize)
        .map(|i| {
            let at = first + i * TYPE_SIZE;
            // SAFETY: the blob's type array holds `count` `MapMarkerType`
            // entries of `TYPE_SIZE` bytes from `first` (count bounded by
            // `MAX_TYPES` earlier) and `i` is below `count`; each keeps a `u32`
            // id at `TYPE_ID` on client build 1269579.
            let id: u32 = unsafe { crate::memory::read(at, TYPE_ID) };
            // SAFETY: entry `i` of the blob's type array, `i` below `count`;
            // its custom flag is a byte at `TYPE_CUSTOM` on client build
            // 1269579, read as a byte and compared.
            let custom: u8 = unsafe { crate::memory::read(at, TYPE_CUSTOM) };
            // SAFETY: entry `i` of the blob's type array, `i` below `count`;
            // its label flag is a byte at `TYPE_LABEL` on client build
            // 1269579, read as a byte and compared.
            let label: u8 = unsafe { crate::memory::read(at, TYPE_LABEL) };
            // SAFETY: entry `i` of the blob's type array, `i` below `count`;
            // its above-fog flag is a byte at `TYPE_ABOVE_FOG` on client
            // build 1269579, read as a byte and compared.
            let above_fog: u8 = unsafe { crate::memory::read(at, TYPE_ABOVE_FOG) };
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
    *MARKER_TYPE.get_or_init(|| chosen)
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

    #[test]
    fn markers_need_a_build_with_its_own_table() {
        assert!(layout_pinned(Kind::Exact));
        assert!(!layout_pinned(Kind::Fallback));
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
