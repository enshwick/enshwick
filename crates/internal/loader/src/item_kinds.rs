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
pub const CAPACITY: usize = 1 << 14;
const EMPTY: u64 = 0;
const UNKNOWN: usize = usize::MAX;
/// An opened `ItemInfo` handle the game has not resolved yet.
const PENDING: u32 = 1;
/// An opened `ItemInfo` handle whose item is recorded.
const DONE: u32 = 2;

/// A map of non-zero `u32` keys to `u32` values over fixed atomics, which
/// the resource detours use without a lock or an allocation.
///
/// Keys are never removed: the slots in use are bounded by the distinct
/// keys, and a probe for a missing key ends at the first empty slot.
#[derive(Debug)]
pub struct Table<const N: usize> {
    slots: [AtomicU64; N],
}

/// [`Table::insert`] found no free slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

impl<const N: usize> Table<N> {
    /// An empty table, for a `static`.
    #[must_use]
    pub const fn new() -> Self {
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

    /// The slots to probe for `key`, starting at its hash.
    fn probe(key: u32) -> impl Iterator<Item = usize> {
        let start = key.wrapping_mul(0x9e37_79b9) as usize % N;
        (0..N).map(move |i| (start + i) % N)
    }

    /// The slot holding `key`, or the empty slot that ends its probe.
    fn slot(&self, key: u32) -> Option<&AtomicU64> {
        Self::probe(key)
            .map(|i| &self.slots[i])
            .find(|slot| match slot.load(Ordering::Acquire) {
                EMPTY => true,
                entry => Self::split(entry).0 == key,
            })
    }

    /// Sets `key` to `value` and answers the value it had.
    ///
    /// # Errors
    ///
    /// [`Full`] when `key` is new and no slot is free, or `key` is zero.
    pub fn insert(&self, key: u32, value: u32) -> Result<Option<u32>, Full> {
        if key == 0 {
            return Err(Full);
        }
        let entry = Self::pack(key, value);
        loop {
            let slot = self.slot(key).ok_or(Full)?;
            match slot.compare_exchange(EMPTY, entry, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Ok(None),
                Err(now) if Self::split(now).0 == key => {
                    return Ok(Some(Self::split(slot.swap(entry, Ordering::AcqRel)).1));
                }
                // Another key took the empty slot first: probe again.
                Err(_) => {}
            }
        }
    }

    /// The value of `key`.
    #[must_use]
    pub fn get(&self, key: u32) -> Option<u32> {
        let entry = self.slot(key)?.load(Ordering::Acquire);
        let (found, value) = Self::split(entry);
        (key != 0 && found == key).then_some(value)
    }

    /// Sets `key` from `from` to `to`; `false` when it held another value
    /// or was not there.
    pub fn replace(&self, key: u32, from: u32, to: u32) -> bool {
        key != 0
            && self.slot(key).is_some_and(|slot| {
                slot.compare_exchange(
                    Self::pack(key, from),
                    Self::pack(key, to),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            })
    }

    #[cfg(test)]
    fn used(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.load(Ordering::Relaxed) != EMPTY)
            .count()
    }
}

impl<const N: usize> Default for Table<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// The items' kinds as the game loads them, with `N` slots per table.
///
/// The `ItemInfo` handles noted are taken to stay `ItemInfo` until the game
/// resolves them. Should the database give a handle opened and never
/// resolved to another resource later, that resource's resolve is read as an
/// item: the length checks keep the read inside it, but the item recorded
/// may take a wrong kind.
#[derive(Debug)]
pub struct ItemKinds<const N: usize> {
    /// The `ItemInfo` handles the game opened: [`PENDING`] until resolved,
    /// then [`DONE`].
    opened: Table<N>,
    /// How many handles in `opened` are [`PENDING`]: a resolve reads
    /// nothing while none is.
    pending: AtomicUsize,
    /// Each loaded item's equipment slot, by item id.
    slots: Table<N>,
    /// Where `ItemInfo` holds its item id and its equipment slot;
    /// [`UNKNOWN`] until sealed, and for good when reflection lacks either.
    id_at: AtomicUsize,
    slot_at: AtomicUsize,
    /// The slot values that make a kind other than [`RecipeKind::OTHER`].
    kinds: OnceLock<Box<[(u64, RecipeKind)]>>,
    warned: AtomicBool,
}

impl<const N: usize> ItemKinds<N> {
    /// Nothing known yet, for a `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            opened: Table::new(),
            pending: AtomicUsize::new(0),
            slots: Table::new(),
            id_at: AtomicUsize::new(UNKNOWN),
            slot_at: AtomicUsize::new(UNKNOWN),
            kinds: OnceLock::new(),
            warned: AtomicBool::new(false),
        }
    }

    /// Starts recording: `ItemInfo` holds its item id at `id_at` and its
    /// one-byte slot at `slot_at`, and `kinds` names the slot values with a
    /// kind of their own. Only the first call counts.
    pub fn seal(&self, id_at: usize, slot_at: usize, kinds: Box<[(u64, RecipeKind)]>) {
        if self.kinds.set(kinds).is_ok() {
            self.id_at.store(id_at, Ordering::Relaxed);
            self.slot_at.store(slot_at, Ordering::Release);
        }
    }

    /// Where `ItemInfo` holds its item id and its equipment slot, once
    /// sealed.
    #[must_use]
    pub fn offsets(&self) -> Option<(usize, usize)> {
        let slot_at = self.slot_at.load(Ordering::Acquire);
        (slot_at != UNKNOWN).then(|| (self.id_at.load(Ordering::Relaxed), slot_at))
    }

    /// Notes that the game opened an `ItemInfo` resource as `handle`; a
    /// failed open, handle zero, is not noted.
    pub fn opened(&self, handle: u32) {
        if handle == 0 || self.offsets().is_none() {
            return;
        }
        match self.opened.insert(handle, PENDING) {
            Ok(Some(PENDING)) => {}
            Ok(_) => {
                self.pending.fetch_add(1, Ordering::AcqRel);
            }
            Err(Full) => self.full(),
        }
    }

    /// Whether any noted handle waits for its resolve; the one check on a
    /// resolve once the items are in.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) != 0
    }

    /// Whether `handle` is an `ItemInfo` the game opened and has not
    /// resolved yet; marks it resolved, so each is read once.
    pub fn take_opened(&self, handle: u32) -> bool {
        let taken = self.opened.replace(handle, PENDING, DONE);
        if taken {
            self.pending.fetch_sub(1, Ordering::AcqRel);
        }
        taken
    }

    /// Records that the item `item` has the equipment slot `slot`.
    pub fn record(&self, item: u32, slot: u8) {
        if self.slots.insert(item, u32::from(slot)).is_err() {
            self.full();
        }
    }

    /// The kind of the recipes that make the item `item`; `None` until the
    /// game has resolved the item.
    #[must_use]
    pub fn kind(&self, item: u32) -> Option<RecipeKind> {
        let slot = u64::from(self.slots.get(item)?);
        let kinds = self.kinds.get()?;
        Some(
            kinds
                .iter()
                .find(|&&(value, _)| value == slot)
                .map_or(RecipeKind::OTHER, |&(_, kind)| kind),
        )
    }

    #[cold]
    fn full(&self) {
        if !self.warned.swap(true, Ordering::Relaxed) {
            crate::log!(
                WARN,
                "more items than recipe kinds can hold: the rest have no kind"
            );
        }
    }
}

impl<const N: usize> Default for ItemKinds<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// The game's items, as the resource detours record them.
#[cfg(windows)]
pub(crate) static ITEMS: ItemKinds<CAPACITY> = ItemKinds::new();

/// Learns from `reflection` where `ItemInfo` holds its id and its slot and
/// which slot values make which kind; without them no kind is recorded.
#[cfg(windows)]
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
    ITEMS.seal(id_at, slot_at, kinds);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: usize = 16;

    #[test]
    fn a_table_keeps_the_last_value_of_each_key() {
        let table = Table::<SMALL>::new();
        assert_eq!(table.insert(7, 1), Ok(None));
        assert_eq!(table.insert(7, 2), Ok(Some(1)));
        assert_eq!(table.get(7), Some(2));
        assert_eq!(table.get(8), None);
        assert_eq!(table.insert(0, 1), Err(Full));
        assert_eq!(table.get(0), None);
        assert!(table.replace(7, 2, 3));
        assert!(!table.replace(7, 2, 4));
        assert!(!table.replace(8, 0, 1));
        assert_eq!(table.get(7), Some(3));
    }

    #[test]
    fn keys_that_share_a_start_slot_probe_past_each_other() {
        let table = Table::<SMALL>::new();
        let start = |key| Table::<SMALL>::probe(key).next();
        let keys: Vec<u32> = (1..u32::MAX)
            .filter(|&k| start(k) == start(1))
            .take(3)
            .collect();
        for &key in &keys {
            assert_eq!(table.insert(key, key), Ok(None));
        }
        for &key in &keys {
            assert_eq!(table.get(key), Some(key));
        }
    }

    #[test]
    fn a_full_table_refuses_new_keys_and_keeps_the_old() {
        let table = Table::<SMALL>::new();
        let capacity = u32::try_from(SMALL).unwrap();
        for key in 1..=capacity {
            assert_eq!(table.insert(key, key), Ok(None));
        }
        assert_eq!(table.insert(capacity + 1, 1), Err(Full));
        assert_eq!(table.get(capacity + 1), None);
        assert_eq!(table.insert(1, 9), Ok(Some(1)));
    }

    fn sealed() -> ItemKinds<SMALL> {
        let items = ItemKinds::new();
        items.seal(0, 8, Box::new([(10, RecipeKind::BUILDING_BLOCK)]));
        items
    }

    #[test]
    fn opens_and_resolves_of_the_same_handles_never_fill_the_table() {
        let items = sealed();
        for round in 0..1000 {
            let handle = round % 4 + 1;
            items.opened(handle);
            items.opened(handle);
            assert!(items.pending());
            assert!(items.take_opened(handle));
            assert!(!items.take_opened(handle));
            assert!(!items.pending());
        }
        assert_eq!(items.opened.used(), 4);
        assert!(!items.take_opened(99));
    }

    #[test]
    fn a_failed_open_and_an_unsealed_build_note_nothing() {
        let items = sealed();
        items.opened(0);
        assert!(!items.pending());
        assert!(!items.warned.load(Ordering::Relaxed));

        let unsealed = ItemKinds::<SMALL>::new();
        unsealed.opened(5);
        assert!(!unsealed.pending());
        assert_eq!(unsealed.offsets(), None);
    }

    #[test]
    fn an_item_has_the_kind_of_its_slot_and_other_slots_are_other() {
        let items = sealed();
        assert_eq!(items.offsets(), Some((0, 8)));
        items.record(0x5e01, 10);
        items.record(0x5e02, 3);
        assert_eq!(items.kind(0x5e01), Some(RecipeKind::BUILDING_BLOCK));
        assert_eq!(items.kind(0x5e02), Some(RecipeKind::OTHER));
        assert_eq!(items.kind(0x5e03), None);
    }
}
