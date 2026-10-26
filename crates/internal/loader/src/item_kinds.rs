//! The kind of the recipes that make each item, from the equipment slot in
//! the item's static data, recorded as the game resolves its `ItemInfo`
//! resources through the served-resource detours.
//!
//! A dedicated server resolves its items after it first builds the recipe
//! registry, so a recipe patch sees kinds only in a rebuild there.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;

use enshwick::abi::RecipeKind;

/// Slots of each table: more than the items of the game and its mods.
const CAPACITY: usize = 1 << 14;
const EMPTY: u64 = 0;
/// A slot whose key was taken; probes pass over it.
const TAKEN: u64 = u64::MAX;
const UNKNOWN: usize = usize::MAX;

/// A map of non-zero `u32` keys to `u32` values over a fixed array of
/// atomics, so the resource detours write and read it without a lock or
/// an allocation.
struct Table<const N: usize> {
    slots: [AtomicU64; N],
}

/// What [`Table::set`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Set {
    Added,
    Replaced,
    Full,
}

impl<const N: usize> Table<N> {
    const fn new() -> Self {
        Self {
            slots: [const { AtomicU64::new(EMPTY) }; N],
        }
    }

    fn pack(key: u32, value: u32) -> u64 {
        u64::from(key) << 32 | u64::from(value)
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "an entry is a key in the high half and a value in the low"
    )]
    fn split(entry: u64) -> (u32, u32) {
        ((entry >> 32) as u32, entry as u32)
    }

    fn key(entry: u64) -> u32 {
        Self::split(entry).0
    }

    /// The slots to probe for `key`, starting at its hash.
    fn probe(key: u32) -> impl Iterator<Item = usize> {
        let start = key.wrapping_mul(0x9e37_79b9) as usize % N;
        (0..N).map(move |i| (start + i) % N)
    }

    fn set(&self, key: u32, value: u32) -> Set {
        if key == 0 {
            return Set::Full;
        }
        let entry = Self::pack(key, value);
        for i in Self::probe(key) {
            let slot = &self.slots[i];
            let mut current = slot.load(Ordering::Acquire);
            if current == EMPTY {
                match slot.compare_exchange(EMPTY, entry, Ordering::AcqRel, Ordering::Acquire) {
                    Ok(_) => return Set::Added,
                    Err(now) => current = now,
                }
            }
            if current != TAKEN && Self::key(current) == key {
                slot.store(entry, Ordering::Release);
                return Set::Replaced;
            }
        }
        Set::Full
    }

    fn find(&self, key: u32) -> Option<&AtomicU64> {
        if key == 0 {
            return None;
        }
        for i in Self::probe(key) {
            let slot = &self.slots[i];
            match slot.load(Ordering::Acquire) {
                EMPTY => return None,
                TAKEN => {}
                entry if Self::key(entry) == key => return Some(slot),
                _ => {}
            }
        }
        None
    }

    fn get(&self, key: u32) -> Option<u32> {
        let entry = self.find(key)?.load(Ordering::Acquire);
        (entry != TAKEN && Self::key(entry) == key).then_some(Self::split(entry).1)
    }

    /// Takes `key` out; `None` when it was not there or another thread
    /// took it first.
    fn take(&self, key: u32) -> Option<u32> {
        let slot = self.find(key)?;
        let entry = slot.load(Ordering::Acquire);
        if entry == TAKEN || Self::key(entry) != key {
            return None;
        }
        slot.compare_exchange(entry, TAKEN, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|entry| Self::split(entry).1)
    }
}

/// Handles of the `ItemInfo` resources the game opened and has not
/// resolved yet.
static OPENED: Table<CAPACITY> = Table::new();
/// Each loaded item's equipment slot, by item id.
static SLOTS: Table<CAPACITY> = Table::new();
/// Where `ItemInfo` holds its item id and its equipment slot; [`UNKNOWN`]
/// until sealed, and for good when reflection lacks either.
static ID_AT: AtomicUsize = AtomicUsize::new(UNKNOWN);
static SLOT_AT: AtomicUsize = AtomicUsize::new(UNKNOWN);
/// The slot values that make a kind other than [`RecipeKind::OTHER`].
static KINDS: OnceLock<Box<[(u64, RecipeKind)]>> = OnceLock::new();

/// Learns from `reflection` where `ItemInfo` holds its id and its slot and
/// which slot values make which kind; without them no kind is recorded.
pub(crate) fn seal(reflection: &enshwick_gamedata::reflect::Reflection<'_>) {
    use enshwick_gamedata::catalogue;

    let slot_type = catalogue::type_name(catalogue::SLOT_TYPE);
    let one_byte = reflection
        .type_info(slot_type)
        .is_ok_and(|info| info.size() == 1);
    let kinds = catalogue::recipe_kind_slots(reflection);
    let at = |id: &str| {
        catalogue::constant(id, reflection).and_then(|offset| usize::try_from(offset).ok())
    };
    let offsets = at(catalogue::ITEM_ID)
        .zip(at(catalogue::ITEM_SLOT))
        .filter(|_| one_byte && !kinds.is_empty());
    let Some((id_at, slot_at)) = offsets else {
        crate::log!(
            WARN,
            "recipe kinds are off: this build does not reflect the items' equipment slots"
        );
        return;
    };
    let kinds = kinds
        .into_iter()
        .map(|(value, kind)| (value, kind.into()))
        .collect();
    if KINDS.set(kinds).is_err() {
        return;
    }
    ID_AT.store(id_at, Ordering::Relaxed);
    SLOT_AT.store(slot_at, Ordering::Release);
}

/// Where `ItemInfo` holds its item id and its equipment slot, once sealed.
pub(crate) fn offsets() -> Option<(usize, usize)> {
    let slot_at = SLOT_AT.load(Ordering::Acquire);
    (slot_at != UNKNOWN).then(|| (ID_AT.load(Ordering::Relaxed), slot_at))
}

/// Notes that the game opened an `ItemInfo` resource as `handle`.
pub(crate) fn opened(handle: u32) {
    if offsets().is_some() && OPENED.set(handle, 0) == Set::Full {
        full();
    }
}

/// Whether `handle` is an `ItemInfo` the game opened and has not resolved
/// yet; takes it, so each is read once.
pub(crate) fn take_opened(handle: u32) -> bool {
    offsets().is_some() && OPENED.take(handle).is_some()
}

/// Records that the item `item` has the equipment slot `slot`.
pub(crate) fn record(item: u32, slot: u8) {
    if SLOTS.set(item, u32::from(slot)) == Set::Full {
        full();
    }
}

#[cold]
fn full() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::log!(
            WARN,
            "more items than recipe kinds can hold: the rest have no kind"
        );
    }
}

/// The kind of the recipes that make the item `item`; `None` until the
/// game has resolved the item.
pub(crate) fn kind(item: u32) -> Option<RecipeKind> {
    let slot = u64::from(SLOTS.get(item)?);
    let kinds = KINDS.get()?;
    Some(
        kinds
            .iter()
            .find(|&&(value, _)| value == slot)
            .map_or(RecipeKind::OTHER, |&(_, kind)| kind),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: usize = 16;

    #[test]
    fn a_table_keeps_the_last_value_of_each_key() {
        let table = Table::<SMALL>::new();
        assert_eq!(table.set(7, 1), Set::Added);
        assert_eq!(table.set(7, 2), Set::Replaced);
        assert_eq!(table.get(7), Some(2));
        assert_eq!(table.get(8), None);
        assert_eq!(table.set(0, 1), Set::Full);
        assert_eq!(table.get(0), None);
    }

    #[test]
    fn a_taken_key_is_gone_once_and_probes_pass_over_it() {
        let table = Table::<SMALL>::new();
        // Keys that share a start slot probe past each other.
        let start = |key| Table::<SMALL>::probe(key).next();
        let keys: Vec<u32> = (1..u32::MAX)
            .filter(|&k| start(k) == start(1))
            .take(3)
            .collect();
        for &key in &keys {
            assert_eq!(table.set(key, key), Set::Added);
        }
        assert_eq!(table.take(keys[0]), Some(keys[0]));
        assert_eq!(table.take(keys[0]), None);
        assert_eq!(table.get(keys[2]), Some(keys[2]));
        assert_eq!(table.set(keys[0], 5), Set::Added);
        assert_eq!(table.get(keys[0]), Some(5));
    }

    #[test]
    fn a_full_table_refuses_new_keys_and_keeps_the_old() {
        let table = Table::<SMALL>::new();
        let capacity = u32::try_from(SMALL).unwrap();
        for key in 1..=capacity {
            assert_eq!(table.set(key, key), Set::Added);
        }
        assert_eq!(table.set(capacity + 1, 1), Set::Full);
        assert_eq!(table.get(capacity), Some(capacity));
        assert_eq!(table.set(1, 9), Set::Replaced);
    }
}
