//! The bases past 32 altar zones: the zones of each base in a side table,
//! and the game's member lookups replaced to read it.
//!
//! The game names a base's zones in a 32-bit mask, and copies the world's
//! zone list into room for 16 that cannot grow; given an allocator, it grows
//! to every listed zone. Its grouping still runs, to release the old bases
//! and copy the world's zone list, but it may start
//! only the bases our grouping plans; after it, each base takes our box and
//! zones, the mask its first 32. Bases past the game's 16 start at window
//! indices ([`window`]) with the game's own new base code, the count set to
//! the index for the call; the count stays at most 16 and the side table
//! knows the rest. The four lookups that read the mask answer from the side
//! table instead. Facts from server 399 and client 426, where both bodies
//! are the same.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::{Answer, Container, Fields};
use crate::base_groups::grouping::{self, Grouped, MAX_LISTED, ZoneSet};
use crate::base_groups::limit::Aabb;
use crate::base_groups::lookup::{self, Bases, Holder};
use crate::base_groups::window::{self, Kind};

/// The kind of bases object `obj` as an index of the per-kind state: the
/// client's object 0, the server's 1.
///
/// # Safety
///
/// - `obj` is a live, non-null bases object laid out as `fields`.
pub(super) unsafe fn kind(obj: *const u8, fields: &Fields) -> usize {
    // SAFETY: the caller guarantees a live object; its kind byte sits at the
    // build table's `bases.kind`.
    usize::from(unsafe { read_at::<u8>(obj, fields.kind) } & 1)
}

/// The side a bases object serves.
///
/// # Safety
///
/// - `obj` is a live, non-null bases object laid out as `fields`.
pub(super) unsafe fn side_of(obj: *const u8, fields: &Fields) -> Kind {
    // SAFETY: the caller's guarantee, passed on.
    if unsafe { kind(obj, fields) } == 1 {
        Kind::Server
    } else {
        Kind::Client
    }
}
/// The error byte for a point outside every base.
const NOT_IN_A_BASE: u8 = 0x1f;
/// The bit the game sets in the world allocator when it hands it to a
/// container.
const HANDED: usize = 1;

/// The callback that grows a zone box container to hold `-wanted` boxes.
type GrowFn = unsafe extern "system" fn(vec: *mut u8, wanted: i64, exact: u64) -> u8;

/// The bases of one bases object and the zones of each, by record index,
/// written only by the grouping, which the game never runs beside a lookup.
#[derive(Debug)]
pub(super) struct Side {
    /// The object the sets belong to; another object reads its own masks.
    owner: AtomicUsize,
    /// How many bases there are; their indices are [`window::indices`] of
    /// it.
    bases: AtomicUsize,
    sets: [[AtomicU64; 4]; window::INDICES],
}

impl Side {
    const fn new() -> Self {
        Self {
            owner: AtomicUsize::new(0),
            bases: AtomicUsize::new(0),
            sets: [const { [const { AtomicU64::new(0) }; 4] }; window::INDICES],
        }
    }

    /// The side table of a `kind` object.
    pub(super) fn of(kind: Kind) -> &'static Self {
        &SIDES[usize::from(kind == Kind::Server)]
    }

    fn set(&self, i: usize) -> ZoneSet {
        self.sets.get(i).map_or_else(ZoneSet::default, |words| {
            ZoneSet::from_words(words.each_ref().map(|w| w.load(Ordering::Relaxed)))
        })
    }

    /// Fills the sets for `owner`. No reader runs meanwhile: the game runs
    /// its grouping in the world's serial update, beside no system (server
    /// 399 and client 426, medium confidence), so the owner is cleared and
    /// set only to keep a lookup on another object off a half-filled table;
    /// x86 keeps the stores in order.
    fn store(&self, owner: *const u8, bases: &Grouped) {
        self.owner.store(0, Ordering::Relaxed);
        for (i, words) in self.sets.iter().enumerate() {
            let set = window::position(i)
                .and_then(|n| bases.get(n))
                .map_or_else(ZoneSet::default, |base| base.zones);
            for (word, value) in words.iter().zip(set.words()) {
                word.store(value, Ordering::Relaxed);
            }
        }
        self.bases.store(bases.len(), Ordering::Relaxed);
        self.owner.store(owner.addr(), Ordering::Release);
    }

    /// Forgets the table's object, which then reads its own masks.
    pub(super) fn forget(&self) {
        self.owner.store(0, Ordering::Release);
    }
}

/// Forgets the side table of `obj`, which the game is about to free, so an
/// object made later at the same address reads its own masks until its
/// first grouping.
pub(super) fn forget_object(obj: *const u8) {
    for side in &SIDES {
        let _gone = side
            .owner
            .compare_exchange(obj.addr(), 0, Ordering::AcqRel, Ordering::Relaxed);
    }
}

/// The side tables of the client's object and the server's, which a
/// hosting client runs both of.
static SIDES: [Side; 2] = [Side::new(), Side::new()];

/// One bases object of the game as the lookups read it.
pub(super) struct Game {
    obj: *const u8,
    fields: Fields,
    side: Option<&'static Side>,
}

impl Game {
    /// The bases object `obj` with its side table, if the grouping filled
    /// one for it.
    ///
    /// # Safety
    ///
    /// - `obj` is a live bases object of the build table's layout, for as
    ///   long as the view is used, with no grouping running meanwhile.
    pub(super) unsafe fn new(obj: *const u8, fields: Fields) -> Self {
        // SAFETY: the caller guarantees a live bases object.
        let kind = unsafe { kind(obj, &fields) };
        let side = SIDES
            .get(kind)
            .filter(|side| side.owner.load(Ordering::Acquire) == obj.addr());
        Self { obj, fields, side }
    }

    /// The build table's fields the view reads by.
    pub(super) const fn fields(&self) -> &Fields {
        &self.fields
    }

    fn record(&self, i: usize) -> usize {
        i * self.fields.stride
    }

    fn zone_entry(&self, z: usize) -> *const u8 {
        // SAFETY: `Game::new`'s caller guarantees a live object; its zone
        // list pointer sits at the build table's `bases.zone_list`.
        let list = unsafe { read_ptr_at::<u8>(self.obj, self.fields.zone_list) };
        list.cast_const().wrapping_add(z * self.fields.zone_size)
    }

    fn read_box(&self, at: *const u8) -> Aabb {
        // SAFETY: callers pass a zone entry or a record's box, both in the
        // zone layout: the low corner first, the high one at `zone_high`.
        let min = unsafe { read_at::<[f32; 3]>(at, 0) };
        // SAFETY: the same box, whose high corner sits at the build table's
        // `bases.zone_high`.
        let max = unsafe { read_at::<[f32; 3]>(at, self.fields.zone_high) };
        Aabb { min, max }
    }

    fn bounds_at(&self, i: usize) -> *const u8 {
        self.obj
            .wrapping_add(self.record(i) + self.fields.members - self.fields.zone_size)
    }
}

impl Game {
    /// The game's own count, which never passes the 16 of its array.
    fn game_count(&self) -> usize {
        // SAFETY: `Game::new`'s caller guarantees a live object; its u32 base
        // count sits at the build table's `bases.count`.
        let count = unsafe { read_at::<u32>(self.obj, self.fields.count) };
        usize::try_from(count).map_or(window::VANILLA, |n| n.min(window::VANILLA))
    }

    /// Whether `i` is the record index of one of the bases.
    pub(super) fn holds_index(&self, i: usize) -> bool {
        window::position(i).is_some_and(|n| n < self.count())
    }
}

impl Bases for Game {
    fn count(&self) -> usize {
        self.side.map_or_else(
            || self.game_count(),
            |side| side.bases.load(Ordering::Relaxed),
        )
    }

    fn bounds(&self, i: usize) -> Aabb {
        self.read_box(self.bounds_at(i))
    }

    fn members(&self, i: usize) -> ZoneSet {
        match self.side {
            Some(side) => side.set(i),
            None if i < window::VANILLA => {
                // SAFETY: `i` is below the array's 16, so record `i`'s u32
                // mask lies in the live object at the build table's
                // `bases.members`.
                let mask =
                    unsafe { read_at::<u32>(self.obj, self.record(i) + self.fields.members) };
                ZoneSet::from_mask(mask)
            }
            None => ZoneSet::default(),
        }
    }

    fn listed(&self) -> usize {
        // SAFETY: `Game::new`'s caller guarantees a live object; the zone
        // list's length sits at the build table's `bases.zone_list_len`.
        let len = unsafe { read_at::<usize>(self.obj, self.fields.zone_list_len) };
        len.min(MAX_LISTED)
    }

    fn zone(&self, z: usize) -> Aabb {
        self.read_box(self.zone_entry(z))
    }
}

/// Lets the zone list of `obj` grow past its inline room of 16 zones, which
/// the game leaves without an allocator, so its copy of the world's list
/// stops at 16: hands it the object's allocator as the game hands it to each
/// base's lists. Every destroy of the object frees the list through it.
/// Grown, a regroup may move the list and free its old block, where the game
/// left stale inline entries; that is safe because the game runs no reader
/// beside its grouping (server 399 and client 426, medium confidence).
///
/// # Safety
///
/// - `obj` is a live bases object, on the grouping's thread before the game
///   copies the world's list, with no reader meanwhile.
pub(super) unsafe fn let_zone_list_grow(obj: *mut u8, fields: Fields) {
    let allocator_at = fields.zone_list + fields.container.allocator;
    // SAFETY: the caller guarantees a live object; its zone list container
    // starts at the build table's `bases.zone_list`, its allocator field at
    // `bases.zone_list_allocator`.
    let given = unsafe { read_at::<usize>(obj, allocator_at) };
    // SAFETY: the caller guarantees a live object; its world allocator sits
    // at the build table's `bases.allocator`.
    let allocator = unsafe { read_at::<usize>(obj, fields.allocator) };
    if given == 0 && allocator != 0 {
        // SAFETY: the caller guarantees a live object and no reader before
        // the copy; the list's allocator field sits at `allocator_at`.
        unsafe { write_at(obj, allocator_at, allocator | HANDED) };
    }
}

/// How many bases the grouping running on object `obj` may start: what our
/// grouping plans, at most `cap`, read from the zone list the game has just
/// copied, before its first new base.
///
/// # Safety
///
/// - `obj` is the live bases object of a grouping that has just refilled its
///   zone list, on the grouping's thread.
pub(super) unsafe fn planned(obj: *const u8, fields: Fields, cap: usize) -> usize {
    // SAFETY: the caller guarantees a live object whose zone list was just
    // refilled, on the grouping's thread with no reader beside it.
    let game = unsafe { Game::new(obj, fields) };
    plan(&game, cap).len()
}

/// Our grouping of `game`'s listed zones into at most `cap` bases.
///
/// Allocates the zone boxes and the answer, which would take some 17 KB of
/// a game thread's stack; it runs only when the zone list changes.
fn plan(game: &Game, cap: usize) -> Grouped {
    let listed = game.listed().min(MAX_LISTED);
    let boxes: Vec<Aabb> = (0..listed).map(|z| game.zone(z)).collect();
    grouping::group(&boxes, cap)
}

/// Set while the grouping lends the count to the game's new base code for a
/// window record, and how often a whole-array function was entered
/// meanwhile, which must stay 0.
static COUNT_LENT: AtomicBool = AtomicBool::new(false);
static ENTERED_WHILE_LENT: AtomicU32 = AtomicU32::new(0);

/// Counts an entry of a whole-array function, when the count is lent.
pub(super) fn note_entry() {
    if COUNT_LENT.load(Ordering::Relaxed) {
        ENTERED_WHILE_LENT.fetch_add(1, Ordering::Relaxed);
    }
}

/// How many whole-array functions were entered while the count was lent,
/// since the last call.
pub(super) fn take_entries_while_lent() -> u32 {
    ENTERED_WHILE_LENT.swap(0, Ordering::Relaxed)
}

/// Gives the bases the grouping on `obj` left our boxes and zones, starting
/// those it did not with `new_base`, and returns them. With `window`, the
/// object holds window records released since its last grouping, and bases
/// past 16 start there: `window` readies index `i` first, then the game's
/// new base code runs with the count set to `i`, which it takes as the
/// record to fill.
///
/// # Safety
///
/// - `obj` is the live bases object the game's grouping has just filled, on
///   the grouping's thread, with no reader meanwhile.
/// - `new_base` is the game's own new base code, `fn(obj, zone, index)`.
/// - With `window`, `obj` is a block of [`window::BLOCK`] bytes whose window
///   records are initialised and released.
pub(super) unsafe fn settle(
    obj: *mut u8,
    fields: Fields,
    cap: usize,
    new_base: super::AddBaseFn,
    window: Option<&dyn Fn(usize)>,
) -> Option<Grouped> {
    // SAFETY: the caller guarantees a live object the game's grouping has
    // just filled, on its thread with no reader beside it.
    let game = unsafe { Game::new(obj, fields) };
    let cap = if window.is_some() {
        cap
    } else {
        cap.min(window::VANILLA)
    };
    let bases = plan(&game, cap);
    let in_array = bases.len().min(window::VANILLA);
    for base in bases.iter().take(in_array).skip(game.game_count()) {
        let first = u32::try_from(base.first).ok()?;
        // SAFETY: the game's new base code on its own object, with zone
        // `first`'s entry of the list it has just copied, below its length,
        // and the zone's index as the grouping passes it.
        unsafe { new_base(obj, game.zone_entry(base.first), first) };
    }
    if game.game_count() != in_array {
        return None;
    }
    for (n, base) in bases.iter().enumerate().skip(window::VANILLA) {
        let (Some(ready), Some(i)) = (window, window::index(n)) else {
            return None;
        };
        let first = u32::try_from(base.first).ok()?;
        ready(i);
        let at = u32::try_from(i).ok()?;
        // SAFETY: the caller guarantees a live object; its u32 count sits at
        // the build table's `bases.count`.
        let count = unsafe { read_at::<u32>(obj, fields.count) };
        COUNT_LENT.store(true, Ordering::Relaxed);
        // SAFETY: the live object's u32 count at `bases.count`. No reader
        // sees the lent value: the game runs its grouping in the world's
        // serial update beside no system (server 399 and client 426, medium
        // confidence), and `note_entry` counts any whole-array function
        // entered meanwhile, so a server copy can show it stays 0.
        unsafe { write_at(obj, fields.count, at) };
        // SAFETY: the game's new base code, which takes the count as the
        // record to fill and raises it, on record `i` of the window the
        // caller guarantees initialised and released, with zone `first`'s
        // entry of the list.
        unsafe { new_base(obj, game.zone_entry(base.first), first) };
        // SAFETY: the live object's u32 count at `bases.count`, back to the
        // array's.
        unsafe { write_at(obj, fields.count, count) };
        COUNT_LENT.store(false, Ordering::Relaxed);
    }
    for (n, base) in bases.iter().enumerate() {
        let i = window::index(n)?;
        let bounds = game.bounds_at(i).cast_mut();
        // SAFETY: record `i` is one the new base code has just filled, in
        // the array or the window of the live object; its box starts with
        // the low corner.
        unsafe { write_at(bounds, 0, base.bounds.min) };
        // SAFETY: the same box, whose high corner sits at the build table's
        // `bases.zone_high`.
        unsafe { write_at(bounds, fields.zone_high, base.bounds.max) };
        // SAFETY: the same record of the live object; its u32 mask sits at
        // the build table's `bases.members`.
        unsafe { write_at(obj, game.record(i) + fields.members, base.zones.low_mask()) };
    }
    // SAFETY: the caller guarantees a live object.
    Side::of(unsafe { side_of(obj, &fields) }).store(obj, &bases);
    Some(bases)
}

/// Forgets every side table, when a world closes.
pub(super) fn forget() {
    for side in &SIDES {
        side.owner.store(0, Ordering::Release);
    }
}

/// Writes the lookups' answer at `out`, laid out as `a`: the error byte and
/// the zero word.
///
/// # Safety
///
/// - `out` is the lookup's answer, writable for its `a.size` bytes.
unsafe fn answer(out: *mut u8, a: &Answer, error: u8) {
    // SAFETY: the caller guarantees a writable answer; its error byte lies
    // inside it, as `Fields::read` checked.
    unsafe { write_at(out, a.error, error) };
    // SAFETY: the same answer; its zero word lies inside it, as
    // `Fields::read` checked.
    unsafe { write_at(out, a.zero, 0u32) };
}

/// Point in base: the first base holding `p`, into `out`.
///
/// # Safety
///
/// - `out` is writable for the answer's size the build table gives, `obj`
///   a live bases object, `p` readable for three `f32`; the game's own
///   arguments to the lookup.
pub(super) unsafe fn base_at(out: *mut u8, game: &Game, p: [f32; 3]) {
    let a = &game.fields.answer;
    let found = lookup::base_at(game, p);
    // SAFETY: the caller guarantees a writable answer.
    unsafe { answer(out, a, if found.is_some() { 0 } else { NOT_IN_A_BASE }) };
    let index = found.and_then(|i| u8::try_from(i).ok()).unwrap_or(0);
    // SAFETY: the caller guarantees a writable answer; its index byte lies
    // inside it, as `Fields::read` checked.
    unsafe { write_at(out, a.index, index) };
}

/// Point in base `i`.
pub(super) fn in_base(game: &Game, i: usize, p: [f32; 3]) -> bool {
    lookup::in_base(game, i, p)
}

/// The box holding `p`: a one-zone base's box or a zone, into `out`.
///
/// # Safety
///
/// - As for [`base_at`].
pub(super) unsafe fn holder_at(out: *mut u8, game: &Game, p: [f32; 3]) {
    let a = &game.fields.answer;
    let Some(holder) = lookup::holder_at(game, p) else {
        // SAFETY: the caller guarantees a writable answer.
        unsafe { answer(out, a, NOT_IN_A_BASE) };
        let low = out.wrapping_add(a.boxed);
        // SAFETY: the caller guarantees a writable answer; its box, the low
        // corner first, lies inside it, as `Fields::read` checked.
        unsafe { write_at(low, 0, [f32::MAX; 3]) };
        // SAFETY: the same answer; the box's high corner sits `zone_high`
        // after its low one, inside the box.
        unsafe { write_at(low, game.fields.zone_high, [-f32::MAX; 3]) };
        return;
    };
    let from = match holder {
        Holder::Base(i) => game.bounds_at(i),
        Holder::Zone(z) => game.zone_entry(z),
    };
    // SAFETY: the caller guarantees a writable answer.
    unsafe { answer(out, a, 0) };
    // SAFETY: `from` is a live box of the zone layout, `zone_size` bytes,
    // and the answer holds that many at its box, as `Fields::read` checked;
    // the two never overlap.
    unsafe {
        core::ptr::copy_nonoverlapping(from, out.wrapping_add(a.boxed), game.fields.zone_size);
    }
}

/// How many boxes the game's container `vec`, laid out as `c`, holds.
///
/// # Safety
///
/// - `vec` is the game's live zone box container.
pub(super) unsafe fn list_len(vec: *const u8, c: &Container) -> u64 {
    // SAFETY: the caller guarantees a live container; its u64 length.
    unsafe { read_at::<u64>(vec, c.len) }
}

/// Cuts the game's container `vec`, laid out as `c`, back to `len` boxes,
/// `len` at most what it holds.
///
/// # Safety
///
/// - `vec` is the game's live, writable zone box container holding at least
///   `len` boxes.
pub(super) unsafe fn set_list_len(vec: *mut u8, c: &Container, len: u64) {
    // SAFETY: the caller guarantees a writable container holding `len`
    // boxes; its u64 length.
    unsafe { write_at(vec, c.len, len) };
}

/// Appends the zones of the base holding `p` to the game's container `vec`,
/// through its own grow callback, and answers into `out`.
///
/// # Safety
///
/// - `vec` is the game's zone box container, live and writable, whose grow
///   callback, when set, is the game's; the rest as for [`base_at`].
pub(super) unsafe fn zones_at(out: *mut u8, vec: *mut u8, game: &Game, p: [f32; 3]) {
    let Some(zones) = lookup::zones_at(game, p) else {
        // SAFETY: the caller guarantees a writable answer.
        unsafe { answer(out, &game.fields.answer, NOT_IN_A_BASE) };
        return;
    };
    let size = game.fields.zone_size;
    let c = &game.fields.container;
    for z in zones {
        // SAFETY: the caller guarantees a live container; its u64 length.
        let len = unsafe { read_at::<u64>(vec, c.len) };
        // SAFETY: the same live container; its u64 capacity.
        let cap = unsafe { read_at::<u64>(vec, c.cap) };
        if cap < len + 1 {
            // SAFETY: the caller guarantees a live container; its allocator
            // field.
            if unsafe { read_at::<usize>(vec, c.allocator) } == 0 {
                // SAFETY: the same live container, whose caller never reads
                // the field, and its grow and free reach the loader's heap
                // through it.
                unsafe { write_at(vec, c.allocator, super::heap::handed()) };
            }
            // SAFETY: the same live container; its grow callback, null or the
            // game's own.
            let grow = unsafe { read_ptr_at::<u8>(vec, c.grow) };
            if grow.is_null() {
                continue;
            }
            // SAFETY: a non-null callback the game stored in its own
            // container, of the shape the game calls it with.
            let grow: GrowFn = unsafe { core::mem::transmute::<*mut u8, GrowFn>(grow) };
            let wanted = i64::try_from(len + 1).map_or(i64::MIN, |n| -n);
            // SAFETY: the game's callback on its own container, asked for
            // one more box as the game asks.
            if unsafe { grow(vec, wanted, 1) } == 0 {
                continue;
            }
        }
        // SAFETY: the container now holds room for `len + 1` boxes at its
        // data pointer.
        let data = unsafe { read_ptr_at::<u8>(vec, c.data) };
        let Ok(at) = usize::try_from(len) else {
            return;
        };
        // SAFETY: zone `z` is listed, `size` bytes in the live list, and box
        // `len` of the container is room it owns, its boxes the zone list's
        // `size` bytes, as the game's own lookup copies them; the two never
        // overlap.
        unsafe {
            core::ptr::copy_nonoverlapping(game.zone_entry(z), data.wrapping_add(at * size), size);
        }
        // SAFETY: the same live container; its u64 length.
        unsafe { write_at(vec, c.len, len + 1) };
    }
    // SAFETY: the caller guarantees a writable answer.
    unsafe { answer(out, &game.fields.answer, 0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bases::fake::{Entry, FIELDS, Fake, new_base, square};

    /// 40 zones that touch in an L, then one apart.
    fn l_and_one() -> Vec<Aabb> {
        let mut boxes: Vec<Aabb> = (0u8..20)
            .map(|i| square(f32::from(i) * 10.0, 0.0))
            .collect();
        boxes.extend((1u8..=20).map(|i| square(0.0, f32::from(i) * 10.0)));
        boxes.push(square(1000.0, 1000.0));
        boxes
    }

    #[test]
    fn the_settled_bases_hold_every_zone_with_the_game_s_mask_low() {
        let mut fake = Fake::new(1, &l_and_one());
        let obj = fake.ptr();
        // The game's grouping started one base before ours took over.
        // SAFETY: the fake object and its zone 0.
        unsafe { new_base(obj, fake.zones.as_ptr().cast(), 0) };
        // SAFETY: the fake object, filled as after the game's grouping.
        let bases = unsafe { settle(obj, FIELDS, 16, new_base, None) }.unwrap();
        assert_eq!(bases.len(), 2);
        assert_eq!(fake.u32_at(0x24), 2);
        assert_eq!(fake.u32_at(0x140), u32::MAX, "zones 0 to 31 in the mask");
        assert_eq!(fake.u32_at(0x140 + 0x130), 0, "zone 40 is past the mask");
        assert_eq!(fake.f32s_at(0x30 + 0xf0), [0.0, 0.0, 0.0]);
        assert_eq!(fake.f32s_at(0x30 + 0x100), [200.0, 10.0, 210.0]);
        let game = fake.view();
        assert_eq!(game.members(0).len(), 40);
        assert!(game.members(1).contains(40));
    }

    #[test]
    fn lookups_answer_for_zones_past_32_as_the_game_lays_out_its_answers() {
        let mut fake = Fake::new(0, &l_and_one());
        let obj = fake.ptr();
        // SAFETY: the fake object, with no base started yet.
        let _bases = unsafe { settle(obj, FIELDS, 16, new_base, None) }.unwrap();
        let game = fake.view();
        let mut out = [0xAAu8; 0x30];
        // SAFETY: a writable answer.
        unsafe { base_at(out.as_mut_ptr(), &game, [5.0, 5.0, 205.0]) };
        assert_eq!((out[0], &out[4..8], out[8]), (0, &[0u8; 4][..], 0));
        assert!(in_base(&game, 0, [5.0, 5.0, 205.0]));
        assert!(!in_base(&game, 0, [150.0, 5.0, 150.0]));
        // SAFETY: a writable answer of 0x30 bytes.
        unsafe { base_at(out.as_mut_ptr(), &game, [1005.0, 5.0, 1005.0]) };
        assert_eq!((out[0], out[8]), (0, 1));
        // SAFETY: a writable answer of 0x30 bytes.
        unsafe { base_at(out.as_mut_ptr(), &game, [150.0, 5.0, 150.0]) };
        assert_eq!((out[0], out[8]), (NOT_IN_A_BASE, 0));
        // SAFETY: a writable answer of 0x30 bytes.
        unsafe { holder_at(out.as_mut_ptr(), &game, [5.0, 5.0, 205.0]) };
        let zone_39: [u8; 0x20] = bytemuck::cast(fake.zones[39].0);
        assert_eq!(out[0], 0);
        assert_eq!(out[0x10..0x30], zone_39);
        // SAFETY: a writable answer of 0x30 bytes.
        unsafe { holder_at(out.as_mut_ptr(), &game, [150.0, 5.0, 150.0]) };
        assert_eq!(out[0], NOT_IN_A_BASE);
        assert_eq!(out[0x10..0x14], f32::MAX.to_le_bytes());
        assert_eq!(out[0x20..0x24], (-f32::MAX).to_le_bytes());
    }

    /// The game's zone box container as its one caller builds it: 16 boxes
    /// of inline room at +0x30, the game's grow callback, no allocator.
    #[repr(C, align(16))]
    struct List {
        data: *mut Entry,
        len: u64,
        cap: u64,
        grow: Option<GrowFn>,
        allocator: usize,
        inline: [Entry; 16],
    }

    impl List {
        fn new(grow: Option<GrowFn>) -> Box<Self> {
            let mut list = Box::new(Self {
                data: core::ptr::null_mut(),
                len: 0,
                cap: 16,
                grow,
                allocator: 0,
                inline: [Entry([0.0; 8]); 16],
            });
            list.data = list.inline.as_mut_ptr();
            list
        }

        /// Calls slot `at` of the vtable of the allocator the list names.
        fn vtable_slot(&self, at: usize) -> usize {
            let allocator = (self.allocator & !1) as *const u8;
            // SAFETY: the list names the loader's heap, whose first word is
            // its vtable.
            let vtable = unsafe { read_ptr_at::<u8>(allocator, 0) };
            // SAFETY: the vtable holds three function pointers.
            unsafe { read_at::<usize>(vtable, at) }
        }

        /// The game's realloc as the research reads it: inline room for 16
        /// when the allocator is handed, a heap block past it from the
        /// allocator's vtable, the old block freed only above 16.
        fn realloc(&mut self, new_cap: u64, may_fail: bool) -> bool {
            if new_cap == self.cap {
                return true;
            }
            let inline_room = if self.cap == 0 || self.allocator & 1 == 1 {
                16
            } else {
                self.cap
            };
            let allocator = (self.allocator & !1) as *const u8;
            let data = if new_cap == 0 {
                core::ptr::null_mut()
            } else if new_cap <= inline_room {
                self.inline.as_mut_ptr()
            } else {
                assert!(may_fail || self.allocator & 1 == 1, "the game aborts here");
                if self.allocator & 1 == 0 {
                    return false;
                }
                // SAFETY: slot +0x8 of the loader's heap is its allocate.
                let allocate: unsafe extern "system" fn(
                    *const u8,
                    usize,
                    usize,
                    u32,
                    u8,
                ) -> *mut u8 = unsafe { core::mem::transmute(self.vtable_slot(0x8)) };
                let bytes = usize::try_from(new_cap).unwrap() * 0x20;
                // SAFETY: the heap's allocate, called as the game calls it.
                let block = unsafe { allocate(allocator, bytes, 0x10, 4, 0) };
                if block.is_null() {
                    return false;
                }
                block.cast()
            };
            let kept = self.len.min(new_cap);
            if data != self.data && !data.is_null() && kept > 0 {
                // SAFETY: both blocks hold at least `kept` boxes.
                unsafe { core::ptr::copy(self.data, data, usize::try_from(kept).unwrap()) };
            }
            if !self.data.is_null() && self.cap > 16 && self.data != data {
                // SAFETY: slot +0x10 of the loader's heap is its free.
                let free: unsafe extern "system" fn(*const u8, *mut u8, usize) =
                    unsafe { core::mem::transmute(self.vtable_slot(0x10)) };
                // SAFETY: a block the heap handed this list, freed once.
                unsafe { free(allocator, self.data.cast(), 0) };
            }
            self.data = data;
            self.len = kept;
            self.cap = new_cap;
            true
        }
    }

    /// The game's grow callback: the capacity doubles from at least 1 until
    /// it holds `-wanted` boxes, then the realloc, which may fail.
    unsafe extern "system" fn grow(vec: *mut u8, wanted: i64, _exact: u64) -> u8 {
        let wanted = u64::try_from(-wanted).unwrap();
        #[expect(
            clippy::cast_ptr_alignment,
            reason = "the game hands back the test's own aligned container"
        )]
        let list = vec.cast::<List>();
        // SAFETY: the test's container, a `List` the test aligned.
        let list = unsafe { &mut *list };
        let mut cap = list.cap.max(1);
        while cap < wanted {
            cap *= 2;
        }
        u8::from(list.realloc(cap, true))
    }

    #[test]
    fn the_zones_of_a_base_past_32_are_appended_through_the_game_s_callback() {
        let mut fake = Fake::new(1, &l_and_one());
        let obj = fake.ptr();
        // SAFETY: the fake object, with no base started yet.
        let _bases = unsafe { settle(obj, FIELDS, 16, new_base, None) }.unwrap();
        let game = fake.view();
        let mut list = List::new(Some(grow));
        let mut out = [0xAAu8; 0x10];
        let vec = (&raw mut *list).cast::<u8>();
        // SAFETY: a writable answer and the test's container.
        unsafe { zones_at(out.as_mut_ptr(), vec, &game, [5.0, 5.0, 5.0]) };
        assert_eq!((out[0], &out[4..8]), (0, &[0u8; 4][..]));
        assert_eq!(
            (list.len, list.cap),
            (40, 64),
            "16 inline, then 32, then 64"
        );
        assert_eq!(list.allocator, super::super::heap::handed());
        assert_ne!(list.data, list.inline.as_mut_ptr());
        // SAFETY: the container holds `len` boxes.
        let got = unsafe { core::slice::from_raw_parts(list.data, 40) };
        assert_eq!(got[0].0, fake.zones[0].0, "the inline boxes moved along");
        assert_eq!(got[39].0, fake.zones[39].0);
        assert!(list.realloc(0, false), "the caller's last free");
        assert!(list.data.is_null());
        let mut stuck = List::new(None);
        // SAFETY: a writable answer and the test's container, with no grow
        // callback.
        unsafe {
            zones_at(
                out.as_mut_ptr(),
                (&raw mut *stuck).cast(),
                &game,
                [5.0, 5.0, 5.0],
            );
        };
        assert_eq!(stuck.len, 16, "boxes past the room are skipped");
    }

    #[test]
    fn the_zone_list_takes_the_object_s_allocator_once() {
        let mut fake = Fake::new(1, &[square(0.0, 0.0)]);
        let obj = fake.ptr();
        // SAFETY: the fake object, without an allocator yet.
        unsafe { let_zone_list_grow(obj, FIELDS) };
        assert_eq!(fake.obj.0[0x1350..0x1358], [0; 8], "no allocator to hand");
        fake.obj.0[..8].copy_from_slice(&0x1000usize.to_le_bytes());
        // SAFETY: the fake object, now with an allocator.
        unsafe { let_zone_list_grow(obj, FIELDS) };
        assert_eq!(fake.obj.0[0x1350..0x1358], 0x1001usize.to_le_bytes());
        fake.obj.0[0x1350..0x1358].copy_from_slice(&0x2001usize.to_le_bytes());
        // SAFETY: the fake object, whose list already has an allocator.
        unsafe { let_zone_list_grow(obj, FIELDS) };
        assert_eq!(
            fake.obj.0[0x1350..0x1358],
            0x2001usize.to_le_bytes(),
            "a list that has one keeps it"
        );
    }

    #[test]
    fn more_bases_than_planned_settle_nothing() {
        let mut fake = Fake::new(1, &[square(0.0, 0.0), square(10.0, 0.0)]);
        let obj = fake.ptr();
        for (z, entry) in (0u32..).zip(&fake.zones) {
            // SAFETY: the fake object and zone `z`.
            unsafe { new_base(obj, core::ptr::from_ref(entry).cast(), z) };
        }
        // SAFETY: the fake object, with one base more than our plan.
        assert!(unsafe { settle(obj, FIELDS, 16, new_base, None) }.is_none());
    }

    fn twenty_apart() -> Vec<Aabb> {
        crate::bases::fake::apart(20)
    }

    #[test]
    fn bases_past_16_start_at_window_indices_with_the_count_kept_at_16() {
        let mut fake = Fake::new(1, &twenty_apart());
        let obj = fake.ptr();
        let readied = core::cell::RefCell::new(Vec::new());
        let ready = |i: usize| readied.borrow_mut().push(i);
        // SAFETY: the fake object, with every window record zeroed, which
        // the fake new base code takes as released.
        let bases = unsafe { settle(obj, FIELDS, window::MOST, new_base, Some(&ready)) }.unwrap();
        assert_eq!(bases.len(), 20);
        assert_eq!(fake.u32_at(0x24), 16, "the count stays the array's");
        assert_eq!(*readied.borrow(), [93, 94, 95, 96]);
        for (n, i) in [(16usize, 93usize), (19, 96)] {
            let record = 0x30 + i * 0x130;
            assert_eq!(
                fake.u32_at(record + 0x110),
                1 << (n & 31),
                "base {n}'s mask"
            );
            let x = f32::from(u8::try_from(n).unwrap()) * 100.0;
            assert_eq!(fake.f32s_at(record + 0xf0), [x, 0.0, 0.0]);
            assert_eq!(fake.f32s_at(record + 0x100), [x + 10.0, 10.0, 10.0]);
        }
        let game = fake.view();
        assert_eq!(game.count(), 20);
        assert!(game.holds_index(96) && !game.holds_index(97) && !game.holds_index(16));
        let mut out = [0xAAu8; 0x10];
        // SAFETY: a writable answer.
        unsafe { base_at(out.as_mut_ptr(), &game, [1805.0, 5.0, 5.0]) };
        assert_eq!((out[0], out[8]), (0, 95));
        assert!(in_base(&game, 95, [1805.0, 5.0, 5.0]));
    }

    #[test]
    fn without_a_window_a_grouping_keeps_16_bases() {
        let mut fake = Fake::new(1, &twenty_apart());
        let obj = fake.ptr();
        // SAFETY: the fake object, with no base started yet.
        let bases = unsafe { settle(obj, FIELDS, window::MOST, new_base, None) }.unwrap();
        assert_eq!(bases.len(), 16);
        assert_eq!(fake.view().count(), 16);
        assert_eq!(fake.u32_at(0x30 + 93 * 0x130 + 0x110), 0);
    }

    #[test]
    fn an_object_without_a_side_table_reads_its_masks() {
        let mut settled = Fake::new(1, &l_and_one());
        let obj = settled.ptr();
        // SAFETY: the fake object, with no base started yet.
        let _bases = unsafe { settle(obj, FIELDS, 16, new_base, None) }.unwrap();
        let mut other = Fake::new(1, &l_and_one());
        let obj = other.ptr();
        // SAFETY: the fake object and its zone 0.
        unsafe { new_base(obj, other.zones.as_ptr().cast(), 0) };
        assert_eq!(other.view().members(0), ZoneSet::from_mask(1));
        forget();
        assert_eq!(
            settled.view().members(0).len(),
            32,
            "the mask after a forget"
        );
    }
}
