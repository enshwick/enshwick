//! Mod markers on the client's map.
//!
//! Engine facts from client build 1269579: every frame the ECS system
//! `replicate_map_markers_for_ui` fills a list of the map's markers in the
//! interface's play data, which the clear system empties first. The loader
//! appends the mods' markers to that list right after the game fills it, on
//! the same thread, so a marker a mod drops is gone the next frame. A marker
//! shows only with a marker type the game's registry knows; the loader picks
//! a custom marker type with a label from the registry once it loads.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use arc_swap::ArcSwap;
use enshwick_loader_api::abi::{HookFlags, Status, UiMarker};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;

use super::engine::{Buf, UiText};
use crate::chains::Next;
use crate::host::Loader;

/// The most markers one mod shows.
const MAX_MARKERS: usize = 256;
/// Old marker sets kept alive, since the map may still read a label it
/// was handed a frame before.
const RETIRED: usize = 4;

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

// SAFETY: the slot joins the chain of the replicate system, typed `SystemFn`.
static NEXT: Next<SystemFn> = unsafe { Next::new() };
/// Each mod's markers, by owner.
static SETS: Mutex<Vec<(usize, Arc<[Owned]>)>> = Mutex::new(Vec::new());
/// Every mod's markers, swapped whole for the system's thread.
static SNAPSHOT: OnceLock<ArcSwap<Sets>> = OnceLock::new();
/// Every mod's markers, one shared set each.
type Sets = Vec<Arc<[Owned]>>;
static RETIRED_SETS: Mutex<VecDeque<Arc<Sets>>> = Mutex::new(VecDeque::new());
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
            // SAFETY: the label is borrowed for the call, and copied here.
            label: unsafe { m.label.as_str() }.unwrap_or("").as_bytes().into(),
            color: u8::try_from(m.color.min(UiMarker::BLACK)).unwrap_or(0),
        })
        .collect();
    let mut sets = SETS.lock().unwrap_or_else(PoisonError::into_inner);
    sets.retain(|(o, _)| *o != owner);
    if !copied.is_empty() {
        sets.push((owner, copied.into()));
    }
    let next: Sets = sets.iter().map(|(_, list)| Arc::clone(list)).collect();
    drop(sets);
    let old = snapshot().swap(Arc::new(next));
    let mut retired = RETIRED_SETS.lock().unwrap_or_else(PoisonError::into_inner);
    retired.push_back(old);
    while retired.len() > RETIRED {
        retired.pop_front();
    }
    Status::OK
}

/// Notes the play data of the interface context `context`, for the
/// system's thread.
pub(super) fn note(context: usize) {
    // SAFETY: a live context holds its G38 state pointer.
    let state = unsafe { crate::world::read::<usize>(context, CONTEXT_STATE) };
    let play = if state == 0 {
        0
    } else {
        // SAFETY: the state's first word points at the play state, or is 0.
        unsafe { crate::world::read::<usize>(state, 0) }
    };
    let data = if play == 0 {
        0
    } else {
        // SAFETY: the play state holds its play data pointer.
        unsafe { crate::world::read::<usize>(play, STATE_PLAY_DATA) }
    };
    PLAY_DATA.store(data, Ordering::Release);
    // SAFETY: no preconditions.
    NOTED_AT.store(unsafe { GetTickCount64() }, Ordering::Release);
}

/// Links the replicate system on the client.
pub(crate) fn link(loader: &Loader) {
    let target = loader
        .reflection
        .system("replicate_map_markers_for_ui")
        .ok()
        .map(enshwick_loader_reflect::System::entry);
    loader.link_feature(
        "map markers",
        target,
        &NEXT,
        detour as SystemFn,
        HookFlags::NONE,
    );
}

unsafe extern "system" fn detour(context: *mut u8) {
    // SAFETY: forwarded unchanged.
    unsafe { NEXT.get()(context) };
    let appended = std::panic::catch_unwind(append);
    if appended.is_err() {
        crate::log!(ERROR, "map markers: the loader panicked while adding them");
    }
}

/// Reads a value of the game's at `base + offset`.
fn read<T: Copy>(base: usize, offset: usize) -> T {
    // SAFETY: every caller reads a field of the play data, its marker list
    // or its marker type registry, live while the replicate system runs.
    unsafe { crate::world::read::<T>(base, offset) }
}

fn append() {
    let sets = snapshot().load();
    if sets.is_empty() {
        return;
    }
    let data = PLAY_DATA.load(Ordering::Acquire);
    // SAFETY: no preconditions.
    let age = unsafe { GetTickCount64() }.saturating_sub(NOTED_AT.load(Ordering::Acquire));
    if data == 0 || age > FRESH_MS {
        return;
    }
    let Some(kind) = marker_type(data) else {
        return;
    };
    let entries: usize = read(data, LIST_DATA);
    let mut count: u64 = read(data, LIST_COUNT);
    let capacity: u64 = read(data, LIST_CAPACITY);
    if entries == 0 {
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
        let slot = core::ptr::with_exposed_provenance_mut::<u8>(entries + index * ENTRY_SIZE);
        // SAFETY: a free slot of the list, below its capacity, on the
        // system's thread, which owns the list while it runs.
        unsafe { core::ptr::copy_nonoverlapping(entry.ptr(), slot, ENTRY_SIZE) };
        count += 1;
    }
    let slot = core::ptr::with_exposed_provenance_mut::<u64>(data + LIST_COUNT);
    // SAFETY: the list's count, on the system's thread.
    unsafe { slot.write_unaligned(count) };
}

/// The marker type for mod markers: the registry's first custom marker type
/// with a label, else its first type with a label shown above the fog;
/// `None` until the registry has loaded, and for good when it has none.
fn marker_type(data: usize) -> Option<u32> {
    if let Some(found) = MARKER_TYPE.get() {
        return *found;
    }
    let registry: usize = read(data, REGISTRY);
    if registry == 0 {
        return None;
    }
    let blob: usize = read(registry, REGISTRY_BLOB);
    if blob == 0 {
        return None;
    }
    let array = blob + BLOB_TYPES;
    let offset: i32 = read(array, 0);
    let count: u32 = read(array, 4);
    if offset == 0 || count == 0 || count > MAX_TYPES {
        return None;
    }
    let first = array.wrapping_add_signed(offset as isize);
    let types: Vec<(u32, bool, bool, bool)> = (0..count as usize)
        .map(|i| {
            let at = first + i * TYPE_SIZE;
            (
                read::<u32>(at, TYPE_ID),
                read::<u8>(at, TYPE_CUSTOM) != 0,
                read::<u8>(at, TYPE_LABEL) != 0,
                read::<u8>(at, TYPE_ABOVE_FOG) != 0,
            )
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
}
