//! The client's logic entity sets: each base's set of logic graph entities
//! in its zones, rebuilt when the zone list changes.

use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::{CODE, Code, Game, GrowFn, ObjWorldFn, object, point, rec, record, windowed};
use crate::base_groups::lookup::{self, Bases};
use crate::base_groups::window;
use crate::chains::Next;

// SAFETY: `ObjWorldFn` is the signature of the rebuild `REBUILD` finds, and
// `link` uses this slot only as its next.
pub(super) static REBUILD_NEXT: Next<ObjWorldFn> = unsafe { Next::new() };

/// The rebuild's entity list: the game's container header over room of
/// ours, which our callback grows; the game's query touches only the header.
#[repr(C)]
pub(super) struct Found {
    data: *mut Element,
    len: u64,
    cap: u64,
    grow: GrowFn,
    room: Vec<Element>,
}

impl Found {
    pub(super) fn new() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
            cap: 0,
            grow: grow_found,
            room: Vec::new(),
        }
    }

    /// The elements the query appended.
    fn elements(&self) -> &[Element] {
        let len = usize::try_from(self.len).unwrap_or(0).min(self.room.len());
        &self.room[..len]
    }
}

/// One entity the query found: its id, then its component's bytes.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub(super) struct Element([u8; 0x40]);

/// Our grow callback for `Found`, in the game's contract: a negative
/// `wanted` asks for at least `-wanted` elements, else the capacity becomes
/// `wanted`.
unsafe extern "system" fn grow_found(vec: *mut u8, wanted: i64, _keep: u64) -> u8 {
    let result = std::panic::catch_unwind(|| {
        #[expect(
            clippy::cast_ptr_alignment,
            reason = "the game hands back our own `Found`"
        )]
        let found = vec.cast::<Found>();
        // SAFETY: the query hands back the `Found` the rebuild gave it, live
        // and borrowed by nothing else during the call.
        let found = unsafe { &mut *found };
        let need = usize::try_from(wanted.unsigned_abs()).unwrap_or(usize::MAX);
        let len = usize::try_from(found.len)
            .unwrap_or(0)
            .min(found.room.len());
        let cap = if wanted < 0 {
            let mut cap = usize::try_from(found.cap).unwrap_or(1).max(1);
            while cap < need {
                cap = cap.saturating_mul(2);
            }
            cap
        } else {
            need
        };
        if cap <= found.room.len() && wanted < 0 {
            return 1;
        }
        let mut room = vec![Element([0; 0x40]); cap.max(len)];
        room[..len].copy_from_slice(&found.room[..len]);
        found.room = room;
        found.data = found.room.as_mut_ptr();
        found.cap = found.room.len() as u64;
        found.len = found.len.min(found.cap);
        1
    });
    result.unwrap_or(0)
}

pub(super) unsafe extern "system" fn rebuild(obj: *mut u8, world: *mut u8) {
    // SAFETY: the client's object in its serial bases update.
    if let (Some(game), Some(code)) = (unsafe { windowed(obj) }, CODE.get()) {
        // SAFETY: the rebuild's own object and world.
        let done = crate::bases::BASES_FUSE.run("the logic entity sets", || unsafe {
            rebuild_sets(obj, world, &game, code);
        });
        if done.is_some() {
            return;
        }
    }
    // SAFETY: this detour runs only as the rebuild's link, so the slot holds
    // the next function of that signature.
    unsafe { REBUILD_NEXT.get()(obj, world) };
}

/// Empties the set at `set`, keeping its room, as the game does.
///
/// # Safety
///
/// - `set` is a live set of the game's layout, made by the client's new
///   base code.
unsafe fn empty_set(set: *mut u8) {
    // SAFETY: the caller's live set; its u32 count.
    if unsafe { read_at::<u32>(set, record::SET_COUNT - record::SET) } == 0 {
        return;
    }
    // SAFETY: the caller's live set; its bitmap pointer.
    let bitmap = unsafe { read_ptr_at::<u8>(set, record::SET_BITMAP - record::SET) };
    // SAFETY: the caller's live set; its u64 bucket count.
    let buckets = unsafe { read_at::<u64>(set, record::SET_BUCKETS - record::SET) };
    let bytes = usize::try_from(((buckets + 63) >> 3) & !7).unwrap_or(0);
    if !bitmap.is_null() {
        // SAFETY: the set's bitmap, a bit per bucket, as many bytes as the
        // game clears.
        unsafe { bitmap.write_bytes(0, bytes) };
    }
    // SAFETY: the caller's live set; its u32 count.
    unsafe { write_at(set, record::SET_COUNT - record::SET, 0u32) };
}

/// The rebuild over every base of `game`.
///
/// # Safety
///
/// - `obj` is the live client object `game` views and `world` its world,
///   in the serial bases update.
pub(super) unsafe fn rebuild_sets(obj: *mut u8, world: *mut u8, game: &Game, code: &Code) {
    // SAFETY: the caller's live object; the u32 version its grouping saw.
    let grouped = unsafe { read_at::<u32>(obj, object::GROUPED) };
    // SAFETY: the caller's live object; the u32 version the rebuild saw.
    if unsafe { read_at::<u32>(obj, object::REBUILT) } == grouped {
        return;
    }
    // SAFETY: the caller's live object; the u32 version the rebuild saw.
    unsafe { write_at(obj, object::REBUILT, grouped) };
    let indices = || window::indices(game.count());
    for i in indices() {
        // SAFETY: record `i` holds a base the client's new base code started,
        // which made its set.
        unsafe { empty_set(rec(obj, i).wrapping_add(record::SET)) };
    }
    let mut found = Found::new();
    // SAFETY: the game's entity query on its world, appending only through
    // our callback into our own list.
    let _complete = unsafe { (code.query)((&raw mut found).cast(), world, code.hash) };
    for element in found.elements() {
        let at = element.0.as_ptr();
        // SAFETY: the element's fixed-point position, three i64 at 8.
        let p = unsafe { point(at.wrapping_add(8)) };
        let Some(i) = indices().find(|&i| lookup::in_base(game, i, p)) else {
            continue;
        };
        let mut result = [0u8; 16];
        // SAFETY: the element's u32 id comes first.
        let id = unsafe { read_at::<u32>(at, 0) };
        let set = rec(obj, i).wrapping_add(record::SET);
        // SAFETY: the game's insert into record `i`'s set, which the client's
        // new base code made with room, as the game's rebuild calls it.
        let _same = unsafe { (code.insert)(set, result.as_mut_ptr(), &raw const id) };
        if result[0] == 0 {
            break;
        }
    }
    // SAFETY: the caller's live object; its u32 change counter.
    let changes = unsafe { read_at::<u32>(obj, object::CHANGES) };
    // SAFETY: the caller's live object; its u32 change counter.
    unsafe { write_at(obj, object::CHANGES, changes.wrapping_add(1)) };
}
