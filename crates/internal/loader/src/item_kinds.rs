//! The kind of the recipes that make each item, from the equipment slot and
//! the tags in the item's static data, recorded as the game resolves its `ItemInfo`
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

/// Which kind an item's equipment slot and tags make.
#[derive(Debug, Default)]
pub struct Classes {
    /// The slot values with a kind of their own.
    pub slots: Box<[(u64, RecipeKind)]>,
    /// The kind an item in a slot of no kind of its own takes from one tag:
    /// slot value, tag, kind.
    pub tags: Box<[(u64, [u8; 16], RecipeKind)]>,
}

impl Classes {
    /// The kind of an item in `slot` that carries `tags`; the slot's own kind
    /// wins, then the earliest row of [`Classes::tags`] for the slot whose
    /// tag the item carries, whatever the order of its tags.
    pub fn classify(&self, slot: u8, tags: impl IntoIterator<Item = [u8; 16]>) -> RecipeKind {
        let slot = u64::from(slot);
        if let Some(&(_, kind)) = self.slots.iter().find(|&&(value, _)| value == slot) {
            return kind;
        }
        tags.into_iter()
            .filter_map(|carried| {
                self.tags
                    .iter()
                    .position(|&(value, tag, _)| value == slot && tag == carried)
            })
            .min()
            .and_then(|row| self.tags.get(row))
            .map_or(RecipeKind::OTHER, |&(_, _, kind)| kind)
    }

    /// Whether some kind depends on an item's tags, so they are worth
    /// reading.
    #[must_use]
    pub fn by_tags(&self, slot: u8) -> bool {
        self.tags
            .iter()
            .any(|&(value, _, _)| value == u64::from(slot))
    }
}

/// Where an `ItemInfo` holds its tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagLayout {
    /// The blob array header of the tags: an `i32` offset from itself, then
    /// a `u32` count.
    pub at: usize,
    /// The size of one tag entry.
    pub stride: usize,
    /// The 16-byte tag GUID in an entry.
    pub tag_at: usize,
}

impl TagLayout {
    /// The bytes of the tag entries in an `ItemInfo` of `len` bytes whose
    /// header holds `offset` and `count`, from the item's start; `None` when
    /// the header or an entry falls outside the item.
    #[must_use]
    pub fn span(&self, len: usize, offset: i32, count: u32) -> Option<core::ops::Range<usize>> {
        if self.at.checked_add(8)? > len || self.tag_at.checked_add(16)? > self.stride {
            return None;
        }
        if offset == 0 {
            return (count == 0).then_some(0..0);
        }
        let start = self.at.checked_add_signed(isize::try_from(offset).ok()?)?;
        let end = start.checked_add(self.stride.checked_mul(usize::try_from(count).ok()?)?)?;
        (end <= len).then_some(start..end)
    }
}

/// What [`ItemKinds`] reads from an `ItemInfo` and how it classes it.
#[derive(Debug)]
pub struct ItemKindLayout {
    /// Where `ItemInfo` holds its u32 item id.
    pub id_at: usize,
    /// Where `ItemInfo` holds its one-byte equipment slot.
    pub slot_at: usize,
    /// Where `ItemInfo` holds its tags; `None` when reflection lacks them,
    /// and no kind by tag is recorded.
    pub tags: Option<TagLayout>,
    /// Which kind slot and tags make.
    pub classes: Classes,
}

impl ItemKindLayout {
    /// Where `ItemInfo` holds what makes a kind in `reflection`, and which
    /// kind it makes; `None` when it does not reflect the item id or a
    /// one-byte equipment slot. Without the tags, [`ItemKindLayout::tags`] is `None`.
    #[must_use]
    pub fn from_reflection(
        reflection: &enshwick_gamedata::reflect::Reflection<'_>,
    ) -> Option<Self> {
        use enshwick_gamedata::catalogue;

        let one_byte = reflection
            .type_info(catalogue::type_name(catalogue::ids::ITEM_SLOT_TYPE))
            .is_ok_and(|info| info.size() == 1);
        let slots = catalogue::recipe_kind_slots(reflection);
        let at = |id: &str| {
            catalogue::constant(id, reflection).and_then(|offset| usize::try_from(offset).ok())
        };
        let (id_at, slot_at) = at(catalogue::ids::ITEM_INFO_ID)
            .zip(at(catalogue::ids::ITEM_INFO_EQUIPMENT_SLOT))
            .filter(|_| one_byte && !slots.is_empty())?;
        let stride = reflection
            .type_info(catalogue::type_name(catalogue::ids::ITEM_TAG_WEIGHT))
            .ok()
            .and_then(|info| usize::try_from(info.size()).ok());
        let tag_kinds = catalogue::recipe_kind_tags(reflection);
        let tags = at(catalogue::ids::ITEM_INFO_TAGS)
            .zip(stride)
            .zip(at(catalogue::ids::ITEM_TAG_WEIGHT_TAG))
            .map(|((at, stride), tag_at)| TagLayout { at, stride, tag_at })
            .filter(|_| tag_kinds.len() == catalogue::RECIPE_KIND_TAGS.len());
        Some(Self {
            id_at,
            slot_at,
            tags,
            classes: Classes {
                slots: slots
                    .into_iter()
                    .map(|(value, kind)| (value, kind.into()))
                    .collect(),
                tags: tag_kinds
                    .into_iter()
                    .map(|(value, tag, kind)| (value, tag.0, kind.into()))
                    .collect(),
            },
        })
    }

    /// The item id and recipe kind of an `ItemInfo` of `len` bytes, which
    /// `read(at, out)` copies from; it is called only for bytes within
    /// `len`. `None` when the id or the slot lies past `len`; tags past it
    /// count as none.
    pub fn item(&self, len: usize, read: impl Fn(usize, &mut [u8])) -> Option<(u32, RecipeKind)> {
        if self.id_at.checked_add(4)? > len || self.slot_at >= len {
            return None;
        }
        let mut id = [0u8; 4];
        read(self.id_at, &mut id);
        let mut slot = [0u8; 1];
        read(self.slot_at, &mut slot);
        let [slot] = slot;
        let span = self
            .tags
            .filter(|layout| {
                self.classes.by_tags(slot) && layout.at.checked_add(8).is_some_and(|end| end <= len)
            })
            .and_then(|layout| {
                let mut header = [0u8; 8];
                read(layout.at, &mut header);
                let [o0, o1, o2, o3, c0, c1, c2, c3] = header;
                let (offset, count) = (
                    i32::from_le_bytes([o0, o1, o2, o3]),
                    u32::from_le_bytes([c0, c1, c2, c3]),
                );
                Some((layout, layout.span(len, offset, count)?))
            });
        let tags = span.into_iter().flat_map(|(layout, span)| {
            let read = &read;
            span.step_by(layout.stride).map(move |entry| {
                let mut tag = [0u8; 16];
                read(entry + layout.tag_at, &mut tag);
                tag
            })
        });
        Some((u32::from_le_bytes(id), self.classes.classify(slot, tags)))
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
    /// Each loaded item's kind code, by item id.
    kinds: Table<N>,
    /// The layout and classes; unset until sealed, and for good when
    /// reflection lacks the item id or slot.
    sealed: OnceLock<ItemKindLayout>,
    warned: AtomicBool,
}

impl<const N: usize> ItemKinds<N> {
    /// Nothing known yet, for a `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            opened: Table::new(),
            pending: AtomicUsize::new(0),
            kinds: Table::new(),
            sealed: OnceLock::new(),
            warned: AtomicBool::new(false),
        }
    }

    /// Starts recording with `sealed`. Only the first call counts.
    pub fn seal(&self, sealed: ItemKindLayout) {
        let _first = self.sealed.set(sealed);
    }

    /// The layout and classes, once sealed.
    #[must_use]
    pub fn sealed(&self) -> Option<&ItemKindLayout> {
        self.sealed.get()
    }

    /// Notes that the game opened an `ItemInfo` resource as `handle`; a
    /// failed open, handle zero, is not noted.
    pub fn opened(&self, handle: u32) {
        if handle == 0 || self.sealed().is_none() {
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

    /// Records that the recipes making the item `item` are of `kind`.
    pub fn record(&self, item: u32, kind: RecipeKind) {
        if self.kinds.insert(item, kind.0).is_err() {
            self.full();
        }
    }

    /// The kind of the recipes that make the item `item`; `None` until the
    /// game has resolved the item.
    #[must_use]
    pub fn kind(&self, item: u32) -> Option<RecipeKind> {
        self.kinds.get(item).map(RecipeKind)
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

/// Learns from `reflection` where `ItemInfo` holds its id, its slot and its
/// tags, and which of them make which kind; without id and slot no kind is
/// recorded, and without the tags no kind by tag.
#[cfg(windows)]
pub(crate) fn seal(reflection: &enshwick_gamedata::reflect::Reflection<'_>) {
    let Some(sealed) = ItemKindLayout::from_reflection(reflection) else {
        crate::log!(
            WARN,
            "recipe kinds are off: this build does not reflect the items' equipment slots"
        );
        return;
    };
    if sealed.tags.is_none() {
        crate::log!(
            WARN,
            "recipe kinds by item tag are off: this build does not reflect the items' tags"
        );
    }
    ITEMS.seal(sealed);
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

    const BUILD_TOOL: u8 = 3;
    const DECOR: [u8; 16] = [0xd0; 16];
    const COMFORT: [u8; 16] = [0xc0; 16];
    const UNKNOWN_TAG: [u8; 16] = [0xe0; 16];

    fn classes() -> Classes {
        Classes {
            slots: Box::new([(10, RecipeKind::BUILDING_BLOCK)]),
            tags: Box::new([
                (u64::from(BUILD_TOOL), DECOR, RecipeKind::DECORATIVE),
                (
                    u64::from(BUILD_TOOL),
                    COMFORT,
                    RecipeKind::COMFORT_FURNITURE,
                ),
            ]),
        }
    }

    fn sealed() -> ItemKinds<SMALL> {
        let items = ItemKinds::new();
        items.seal(ItemKindLayout {
            id_at: 0,
            slot_at: 8,
            tags: Some(TagLayout {
                at: 0x10,
                stride: 0x18,
                tag_at: 0,
            }),
            classes: classes(),
        });
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
        assert!(unsealed.sealed().is_none());
    }

    #[test]
    fn an_item_has_the_kind_it_was_recorded_with_and_none_before() {
        let items = sealed();
        assert_eq!(items.sealed().map(|s| s.slot_at), Some(8));
        items.record(0x5e01, RecipeKind::BUILDING_BLOCK);
        items.record(0x5e02, RecipeKind::OTHER);
        assert_eq!(items.kind(0x5e01), Some(RecipeKind::BUILDING_BLOCK));
        assert_eq!(items.kind(0x5e02), Some(RecipeKind::OTHER));
        assert_eq!(items.kind(0x5e03), None);
    }

    #[test]
    fn a_slot_kind_wins_and_other_slots_are_other() {
        let classes = classes();
        assert_eq!(classes.classify(10, [DECOR]), RecipeKind::BUILDING_BLOCK);
        assert_eq!(classes.classify(4, []), RecipeKind::OTHER);
        assert_eq!(classes.classify(4, [DECOR]), RecipeKind::OTHER);
    }

    #[test]
    fn a_prop_takes_the_kind_of_its_tag_and_without_one_is_other() {
        let classes = classes();
        assert_eq!(
            classes.classify(BUILD_TOOL, [UNKNOWN_TAG, DECOR]),
            RecipeKind::DECORATIVE
        );
        assert_eq!(
            classes.classify(BUILD_TOOL, [COMFORT]),
            RecipeKind::COMFORT_FURNITURE
        );
        assert_eq!(
            classes.classify(BUILD_TOOL, [UNKNOWN_TAG]),
            RecipeKind::OTHER
        );
        assert_eq!(classes.classify(BUILD_TOOL, []), RecipeKind::OTHER);
    }

    #[test]
    fn a_prop_with_two_kind_tags_takes_the_earlier_row_in_any_tag_order() {
        let classes = classes();
        for tags in [[COMFORT, DECOR], [DECOR, COMFORT]] {
            assert_eq!(classes.classify(BUILD_TOOL, tags), RecipeKind::DECORATIVE);
        }
    }

    /// An `ItemInfo` of the `sealed` layout: id, slot and the tags in a blob
    /// array right after the header.
    fn item_info(id: u32, slot: u8, tags: &[[u8; 16]]) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x18 + 0x18 * tags.len()];
        bytes[..4].copy_from_slice(&id.to_le_bytes());
        bytes[8] = slot;
        if !tags.is_empty() {
            bytes[0x10..0x14].copy_from_slice(&8i32.to_le_bytes());
            let count = u32::try_from(tags.len()).unwrap();
            bytes[0x14..0x18].copy_from_slice(&count.to_le_bytes());
        }
        for (i, tag) in tags.iter().enumerate() {
            let at = 0x18 + 0x18 * i;
            bytes[at..at + 16].copy_from_slice(tag);
        }
        bytes
    }

    fn read(bytes: &[u8]) -> impl Fn(usize, &mut [u8]) + '_ {
        |at, out| out.copy_from_slice(&bytes[at..at + out.len()])
    }

    #[test]
    fn an_item_reads_as_its_id_and_the_kind_of_its_slot_and_tags() {
        let items = sealed();
        let sealed = items.sealed().unwrap();
        let decor = item_info(0x5e01, BUILD_TOOL, &[UNKNOWN_TAG, DECOR]);
        assert_eq!(
            sealed.item(decor.len(), read(&decor)),
            Some((0x5e01, RecipeKind::DECORATIVE))
        );
        let lamp = item_info(0x5e02, BUILD_TOOL, &[UNKNOWN_TAG]);
        assert_eq!(
            sealed.item(lamp.len(), read(&lamp)),
            Some((0x5e02, RecipeKind::OTHER))
        );
        let block = item_info(0x5e03, 10, &[DECOR]);
        assert_eq!(
            sealed.item(block.len(), read(&block)),
            Some((0x5e03, RecipeKind::BUILDING_BLOCK))
        );
        assert_eq!(
            sealed.item(8, read(&block)),
            None,
            "the slot is past the end"
        );
    }

    #[test]
    fn tags_past_the_item_read_as_none() {
        let items = sealed();
        let sealed = items.sealed().unwrap();
        let mut decor = item_info(0x5e01, BUILD_TOOL, &[DECOR]);
        decor[0x14..0x18].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            sealed.item(decor.len(), read(&decor)),
            Some((0x5e01, RecipeKind::OTHER))
        );
        assert_eq!(
            sealed.item(0x14, read(&decor)),
            Some((0x5e01, RecipeKind::OTHER)),
            "the header is cut"
        );
    }

    #[test]
    fn tags_inside_the_item_are_found_and_any_outside_are_refused() {
        let layout = TagLayout {
            at: 0x10,
            stride: 0x18,
            tag_at: 0,
        };
        // Two entries right after the header, which sits at 0x10.
        assert_eq!(layout.span(0x100, 8, 2), Some(0x18..0x48));
        assert_eq!(layout.span(0x100, 0, 0), Some(0..0));
        assert_eq!(layout.span(0x100, 0, 3), None);
        assert_eq!(layout.span(0x48, 8, 2), Some(0x18..0x48));
        assert_eq!(layout.span(0x47, 8, 2), None);
        assert_eq!(layout.span(0x100, -0x11, 1), None);
        assert_eq!(layout.span(0x100, 8, u32::MAX), None);
        assert_eq!(layout.span(0x17, 8, 0), None);
        let wide = TagLayout {
            tag_at: 0x10,
            ..layout
        };
        assert_eq!(wide.span(0x100, 8, 1), None);
    }
}
