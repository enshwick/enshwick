//! The stacks an inventory's slots hold, read from inside a running system:
//! the item filters' uses and the interaction filters' moves read them.

use enshwick_core::raw::read_at;

use crate::engine::catalogue::{self, ids};
use crate::engine::ecs::Components;
use crate::engine::loader::Loader;

/// Where an `Inventory` component keeps its stacks, from the reflected
/// layouts of the running build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stacks {
    /// The `Inventory` component's type hash.
    inventory: u32,
    /// One stack's size.
    stack: usize,
    /// Where a stack holds its `u32` item id.
    item: usize,
    /// Where a stack holds its `u32` count.
    count: usize,
    /// How many stacks one inventory holds.
    slots: usize,
}

impl Stacks {
    /// The layout on the running build; `None` when reflection lacks a part
    /// of it or its reads would leave the stack.
    pub(crate) fn resolve(loader: &Loader) -> Option<Self> {
        let stack = catalogue::size(loader, ids::ITEM_STACK)?;
        let item = loader
            .reflection
            .field_offset(
                enshwick_gamedata::catalogue::type_name(ids::ITEM_STACK),
                "id",
            )
            .ok()
            .and_then(|v| usize::try_from(v).ok())?;
        let count = catalogue::offset(loader, ids::ITEM_STACK_COUNT)?;
        let inventory = catalogue::size(loader, ids::INVENTORY)?;
        Self::new(
            catalogue::component_hash(loader, ids::INVENTORY)?,
            stack,
            item,
            count,
            inventory.checked_div(stack)?,
        )
    }

    /// The layout of these parts; `None` when a stack's `u32` reads would
    /// leave it.
    pub(crate) const fn new(
        inventory: u32,
        stack: usize,
        item: usize,
        count: usize,
        slots: usize,
    ) -> Option<Self> {
        let last = if item > count { item } else { count };
        if last + 4 > stack {
            return None;
        }
        Some(Self {
            inventory,
            stack,
            item,
            count,
            slots,
        })
    }

    /// The item and count in slot `index` of the inventory `inventory`,
    /// both 0 for an empty slot; `None` for an index past the inventory or
    /// no inventory.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live context of a running system, used on its thread.
    pub(crate) unsafe fn holding(
        &self,
        components: &Components,
        ctx: *mut u64,
        inventory: u32,
        index: u8,
    ) -> Option<(u32, u32)> {
        let index = usize::from(index);
        if index >= self.slots {
            return None;
        }
        // SAFETY: the caller guarantees the system's live context on its thread.
        let slots = unsafe { components.component(ctx, inventory, self.inventory) }?;
        let at = index * self.stack;
        // SAFETY: `slots` is the live `Inventory` component, `self.slots`
        // stacks of the reflected `ItemStack` size, and `index` is below it;
        // the stack's u32 item id sits at the reflected `self.item`, inside
        // it (checked by `new`).
        let item: u32 = unsafe { read_at(slots, at + self.item) };
        // SAFETY: `slots` is the live `Inventory` component and `at` the
        // start of stack `index`, below `self.slots`; the stack's u32 count
        // sits at the catalogue's `item_stack.count` offset, inside it
        // (checked by `new`).
        let count: u32 = unsafe { read_at(slots, at + self.count) };
        Some((item, count))
    }

    /// The item and count in `slot`, an `InventorySlotId` read as its
    /// entity in the low 32 bits and its index above them; `None` for an
    /// empty slot, slot 0 or a slot past its inventory.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live context of a running system, used on its thread.
    pub(crate) unsafe fn in_slot(
        &self,
        components: &Components,
        ctx: *mut u64,
        slot: u64,
    ) -> Option<(u32, u32)> {
        let (inventory, index) = split(slot)?;
        // SAFETY: the caller's contract is `holding`'s.
        unsafe { self.holding(components, ctx, inventory, index) }
            .filter(|&(item, count)| item != 0 && count != 0)
    }
}

/// `slot`'s inventory entity and index: the low 32 bits and the byte above
/// them; `None` for no inventory.
pub(crate) const fn split(slot: u64) -> Option<(u32, u8)> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the entity is the low half, the index the next byte"
    )]
    let (inventory, index) = (slot as u32, (slot >> 32) as u8);
    if inventory == 0 {
        None
    } else {
        Some((inventory, index))
    }
}

/// `slot` as the API hands it on: its entity and index, without the padding
/// the game leaves above them; 0 for no slot.
pub(crate) fn slot_id(slot: u64) -> u64 {
    split(slot).map_or(0, |(inventory, index)| {
        u64::from(inventory) | u64::from(index) << 32
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_id_splits_into_its_inventory_and_index_and_ignores_the_padding() {
        assert_eq!(split(0x7700_0003_0000_1234), Some((0x1234, 3)));
        assert_eq!(split(0x0000_0005_0000_0000), None);
    }

    #[test]
    fn a_slot_handed_on_keeps_its_inventory_and_index_and_drops_the_padding() {
        assert_eq!(slot_id(0x7700_0003_0000_1234), 0x0000_0003_0000_1234);
        assert_eq!(slot_id(0x0000_0005_0000_0000), 0, "no inventory, no slot");
    }

    #[test]
    fn a_layout_whose_reads_leave_the_stack_is_refused() {
        assert!(Stacks::new(3, 12, 0, 4, 8).is_some());
        assert!(Stacks::new(3, 12, 0, 9, 8).is_none());
    }
}
