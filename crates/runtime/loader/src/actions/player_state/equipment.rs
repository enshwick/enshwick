//! The slot `SlotSelection` names for each of the local player's
//! equipment slots, and the equipment slot it last named with an item in
//! it, so an equip of an item moved out of the equipment inventories puts
//! it back.

use std::sync::atomic::Ordering;

use enshwick_core::inventory::InventoryCategory;
use enshwick_gamedata::catalogue;

use super::{MAX_SLOTS, SLOT_ID, STATE};
use crate::engine::loader::Loader;

/// The most `EquipmentSlotData` slots kept; client 26430625 reflects 20.
pub(super) const EQUIPMENT_SLOTS: usize = 32;

/// The words of the equipment copy: the slot read's time, then per
/// `EquipmentSlotData` slot the slot it names, the equipment slot it last
/// named with an item in it, and that item.
pub(super) const EQUIPMENT_WORDS: usize = 1 + 3 * EQUIPMENT_SLOTS;

/// Where `SlotSelection` keeps the slot of each equipment slot.
#[derive(Debug)]
pub(super) struct EquipmentLayout {
    /// `SlotSelection`'s hash.
    pub(super) selection: u32,
    /// The offset of its `EquipmentSlotData`.
    pub(super) offset: usize,
    /// The size of an `InventorySlotId`, one slot.
    pub(super) stride: usize,
    /// How many slots it holds, at most [`EQUIPMENT_SLOTS`].
    pub(super) count: usize,
}

/// Where `loader`'s build keeps `SlotSelection`'s equipment slots, from
/// their reflected names; `None` for a build that lacks one of them.
pub(super) fn equipment_layout(loader: &Loader) -> Option<EquipmentLayout> {
    let size = |id| crate::engine::catalogue::size(loader, id);
    let stride = size(catalogue::ids::INVENTORY_SLOT_ID).filter(|&stride| stride >= 8)?;
    Some(EquipmentLayout {
        selection: catalogue::type_hash(catalogue::ids::SLOT_SELECTION)?,
        offset: catalogue::constant(catalogue::ids::SLOT_SELECTION_EQUIPMENT, &loader.reflection)
            .and_then(|at| usize::try_from(at).ok())?,
        stride,
        count: (size(catalogue::ids::EQUIPMENT_SLOT_DATA)? / stride).min(EQUIPMENT_SLOTS),
    })
}

/// The equipment copy after a slot read `slots`, packed as
/// [`SLOT_WORDS`](super::SLOT_WORDS) lays them out, at `at`, in which `SlotSelection` named
/// `named`: each equipment slot keeps the slot of an equipment inventory it
/// last named with an item in it, and that item, from `before`.
pub(super) fn equipment_words(
    before: Option<&[u64; EQUIPMENT_WORDS]>,
    named: &[u64; EQUIPMENT_SLOTS],
    slots: &[u64],
    at: u64,
) -> [u64; EQUIPMENT_WORDS] {
    let mut words = before.copied().unwrap_or([0; EQUIPMENT_WORDS]);
    words[0] = at;
    for (entry, &slot) in named.iter().enumerate() {
        words[1 + 3 * entry] = slot;
    }
    let equipment = u64::from(InventoryCategory::EQUIPMENT.0);
    for packed in slots.as_chunks::<3>().0 {
        let [at, stack, _] = *packed;
        let item = stack & 0xffff_ffff;
        if at >> 40 & 0xff != equipment || item == 0 {
            continue;
        }
        let slot = at & SLOT_ID;
        for (entry, _) in named.iter().enumerate().filter(|&(_, &n)| n == slot) {
            words[2 + 3 * entry] = slot;
            words[3 + 3 * entry] = item;
        }
    }
    words
}

/// The equipment copy after a step in which `SlotSelection` named `named`,
/// when the slot named for an equipment slot moved since `before`: it keeps
/// the slot named before as the equipment slot's home and `item_at` the slot
/// named now, the item that moved there. `None` when none moved, so the
/// step writes nothing. The time stays the last slot read's, 0 before one,
/// so [`equip_move`] still takes it only beside that read.
///
/// Every step runs it, outside the ask: an item the player moves out while
/// no mod reads the slots still finds its way back.
pub(super) fn moved_words(
    before: Option<&[u64; EQUIPMENT_WORDS]>,
    named: &[u64; EQUIPMENT_SLOTS],
    item_at: impl Fn(u64) -> u64,
) -> Option<[u64; EQUIPMENT_WORDS]> {
    let same = |words: &[u64; EQUIPMENT_WORDS]| {
        named
            .iter()
            .enumerate()
            .all(|(entry, &slot)| words[1 + 3 * entry] == slot)
    };
    if before.is_some_and(same) {
        return None;
    }
    let mut words = before.copied().unwrap_or([0; EQUIPMENT_WORDS]);
    for (entry, &now) in named.iter().enumerate() {
        let was = core::mem::replace(&mut words[1 + 3 * entry], now);
        if was != now && was != 0 && now != 0 {
            words[2 + 3 * entry] = was;
            words[3 + 3 * entry] = item_at(now);
        }
    }
    Some(words)
}

/// How an equip runs, as [`equip_move`] decides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquipMove {
    /// No fresh slot read exists yet.
    Unread,
    /// The game's own equipment request runs it.
    Game,
    /// A move of the item into this slot runs it.
    To(u64),
}

/// How an equip of the item in `slot` into the equipment slot at `entry`
/// of `SlotSelection`'s `EquipmentSlotData` runs, read within a second of
/// `now`.
///
/// The server moves an equipped item to the slot `SlotSelection` names for
/// its equipment slot (server 26430253 `0x1401b4e70`, seen live with the
/// research probe, 2026-11-27); after a move out of the equipment
/// inventories that is the item's own slot, so the request moves nothing.
/// Such an equip is a move back into the equipment slot it last named, while
/// that is empty and the item is the one that lay there. Each step keeps
/// that slot ([`moved_words`]), the slot reads or not.
pub(crate) fn equip_move(now: u64, entry: u8, slot: u64) -> EquipMove {
    STATE.slots_asked.store(now, Ordering::Relaxed);
    let Some(slots) = STATE
        .slots
        .load()
        .filter(|slots| crate::actions::local_held::fresh(slots[1], now))
    else {
        return EquipMove::Unread;
    };
    let count = usize::try_from(slots[0]).unwrap_or(0).min(MAX_SLOTS);
    STATE
        .equipment
        .load()
        .filter(|words| words[0] == slots[1])
        .and_then(|words| home(&words, &slots[2..2 + 3 * count], entry, slot))
        .map_or(EquipMove::Game, EquipMove::To)
}

/// The empty slot `words` keeps as the equipment slot at `entry`'s, when
/// `SlotSelection` names `slot` for it, `slot` lies outside the equipment
/// inventories of `slots`, and holds the item that lay there.
fn home(words: &[u64; EQUIPMENT_WORDS], slots: &[u64], entry: u8, slot: u64) -> Option<u64> {
    let at = 1 + 3 * usize::from(entry);
    let [named, home, item] = <[u64; 3]>::try_from(words.get(at..at + 3)?).ok()?;
    let find = |wanted: u64| {
        slots
            .as_chunks::<3>()
            .0
            .iter()
            .find(|packed| packed[0] & SLOT_ID == wanted)
            .map(|packed| (packed[0] >> 40 & 0xff, packed[1] & 0xffff_ffff))
    };
    let equipment = u64::from(InventoryCategory::EQUIPMENT.0);
    let (category, held) = find(slot)?;
    (named == slot
        && home != 0
        && home != slot
        && category != equipment
        && held == item
        && find(home) == Some((equipment, 0)))
    .then_some(home)
}

#[cfg(test)]
mod tests {
    use super::super::pack_slot;
    use super::*;
    use enshwick_core::abi::ClientSlot;

    fn packed(slots: &[(u64, InventoryCategory, u32)]) -> Vec<u64> {
        slots
            .iter()
            .flat_map(|&(slot, category, item)| {
                pack_slot(&enshwick_abi::headed!(ClientSlot {
                    slot,
                    item,
                    count: u32::from(item != 0),
                    category: category.0,
                    ..ClientSlot::EMPTY
                }))
            })
            .collect()
    }

    #[test]
    fn an_equip_after_a_move_out_goes_back_only_to_its_own_empty_equipment_slot() {
        use InventoryCategory as C;
        let (pants, glider, bag, ring) = (1, 1 | 1 << 32, 6858, 2 | 3 << 32);
        let (pants_item, glider_item, armour) = (0xecc1_c60c, 0x534b_a901, 7);
        let mut named = [0; EQUIPMENT_SLOTS];
        named[18] = glider;
        let worn = packed(&[
            (pants, C::EQUIPMENT, pants_item),
            (glider, C::EQUIPMENT, glider_item),
            (ring, C::EQUIPMENT, 0),
            (bag, C::GENERIC, 0),
        ]);
        let before = equipment_words(None, &named, &worn, 1);
        named[18] = bag;
        let moved = |pants_item, bag_item| {
            let slots = packed(&[
                (pants, C::EQUIPMENT, pants_item),
                (glider, C::EQUIPMENT, 0),
                (ring, C::EQUIPMENT, 0),
                (bag, C::GENERIC, bag_item),
            ]);
            (equipment_words(Some(&before), &named, &slots, 2), slots)
        };
        let (words, slots) = moved(pants_item, glider_item);
        assert_eq!(
            home(&words, &slots, 18, bag),
            Some(glider),
            "the glider goes back to the glider slot, not the empty ring slot"
        );
        assert_eq!(home(&words, &slots, 0, bag), None, "another equipment slot");
        assert_eq!(home(&words, &slots, 18, pants), None, "not the named slot");
        let mut taken = slots.clone();
        taken[4] = u64::from(glider_item) | 1 << 32;
        let (other, other_slots) = moved(pants_item, armour);
        assert_eq!(
            home(&other, &other_slots, 18, bag),
            None,
            "another item in the bag slot keeps the game's kind check"
        );
        assert_eq!(
            home(&words, &taken, 18, bag),
            None,
            "the glider slot is no longer empty"
        );
        assert_eq!(
            home(&equipment_words(None, &named, &slots, 2), &slots, 18, bag),
            None,
            "no equipment slot named before the move"
        );
    }

    #[test]
    fn an_equip_after_a_move_out_no_slot_read_saw_goes_back_to_its_equipment_slot() {
        use InventoryCategory as C;
        let (glider, bag, ring) = (1 | 1 << 32, 6858, 2 | 3 << 32);
        let glider_item = 0x534b_a901;
        let mut named = [0; EQUIPMENT_SLOTS];
        named[18] = glider;
        named[3] = ring;
        let worn = packed(&[(glider, C::EQUIPMENT, glider_item), (bag, C::GENERIC, 0)]);
        let read = equipment_words(None, &named, &worn, 1);
        assert_eq!(
            moved_words(Some(&read), &named, |_| unreachable!()),
            None,
            "a step with nothing moved writes nothing"
        );
        named[18] = bag;
        let item_at = |slot| {
            if slot == bag {
                u64::from(glider_item)
            } else {
                0
            }
        };
        let stepped = moved_words(Some(&read), &named, item_at).unwrap();
        assert_eq!(stepped[0], 1, "the time stays the slot read's");
        let slots = packed(&[
            (glider, C::EQUIPMENT, 0),
            (ring, C::EQUIPMENT, 0),
            (bag, C::GENERIC, glider_item),
        ]);
        let after = equipment_words(Some(&stepped), &named, &slots, 2);
        assert_eq!(
            home(&after, &slots, 18, bag),
            Some(glider),
            "the glider goes back though no read saw it in the slot"
        );
        let first = moved_words(None, &named, item_at).unwrap();
        assert_eq!(first[0], 0, "no slot read yet");
        assert_eq!(
            home(
                &equipment_words(Some(&first), &named, &slots, 2),
                &slots,
                18,
                bag
            ),
            None,
            "the first step knows no slot named before"
        );
    }
}
