//! Where the local player's crafting, item transfer, equipment and item
//! requests lie in its `ClientPlayerInput`, and how a client command writes
//! one. The game's own interface raises each as an event the input builder
//! copies into its request with the step count as its version, and the
//! server runs a request when its version changes: crafting in
//! `player_crafting`, which pushes the `CraftRecipeEvent` the crafting
//! system checks, the rest in `inventory_actions` (client 26430625 UI
//! command 0x37 and builder `0x1403af670`; server 26430253 `0x1400d2250`,
//! read in the decompiler, 2026-11-26).

use enshwick_core::raw::{read_at, write_at};

use super::{fits, reflected_size};
use crate::engine::catalogue::{self, ids};
use crate::engine::loader::Loader;

/// The bit every version the loader writes sets, as the waypoint's does:
/// the game versions its requests by the world's step count, which stays
/// below it, so the server takes the game's and the loader's alike.
const LOADER_VERSION: u32 = 1 << 31;

/// A request a client command writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    /// Crafts `count` times at `station`, or by hand, as the player's own
    /// entity, for 0.
    Craft {
        station: u32,
        recipe: u32,
        count: u16,
    },
    /// Moves `count` items, 0 for the whole stack, between two slots, each
    /// packed as the game packs one, entity then index.
    Move { from: u64, to: u64, count: u16 },
    /// Moves every stack from the inventory `from` into `to`, 0 for the
    /// player's own, as the storage window's take all and deposit all do.
    MoveAll { from: u32, to: u32 },
    /// Equips the slot's item into the `EquipmentSlot` member `equipment`.
    Equip { slot: u64, equipment: u8 },
    /// Destroys the slot's stack.
    Trash { slot: u64 },
}

/// One request's place in the input and its version's.
#[derive(Debug)]
struct Record {
    base: usize,
    version: usize,
}

impl Record {
    /// The request `id` inside the input, its `size` bytes holding a `u32`
    /// version at `version`.
    fn resolve(
        loader: &Loader,
        input: usize,
        id: &'static str,
        ty: &'static str,
        version: &'static str,
    ) -> Result<(Self, usize), &'static str> {
        let size = reflected_size(loader, ty)?;
        let base = catalogue::offset(loader, id)
            .filter(|&at| fits(at, size, input))
            .ok_or(id)?;
        let version = field(loader, version, size_of::<u32>(), size)?;
        Ok((Self { base, version }, size))
    }

    /// Moves the request's version on, so the server takes it once.
    ///
    /// # Safety
    ///
    /// - `input` is valid for reads and writes of the build's
    ///   `ClientPlayerInput`, which nothing else touches during the call.
    unsafe fn bump(&self, input: *mut u8) {
        let at = self.base + self.version;
        // SAFETY: the caller's record, which `resolve` checked holds the
        // request's `u32` version at `at`.
        let old: u32 = unsafe { read_at(input, at) };
        // SAFETY: the same record and version.
        unsafe { write_at(input, at, next_version(old)) };
    }
}

/// The offset of the field `id` of a request of `size` bytes, when its
/// `bytes` lie inside it.
fn field(
    loader: &Loader,
    id: &'static str,
    bytes: usize,
    size: usize,
) -> Result<usize, &'static str> {
    catalogue::offset(loader, id)
        .filter(|&at| fits(at, bytes, size))
        .ok_or(id)
}

/// The enum value `id` names, as the byte the requests hold.
fn value(loader: &Loader, id: &'static str) -> Result<u8, &'static str> {
    catalogue::constant(id, &loader.reflection)
        .and_then(|v| u8::try_from(v).ok())
        .ok_or(id)
}

/// The requests' fields, each relative to its request, and the enum values
/// they take.
#[derive(Debug)]
pub(crate) struct ActLayout {
    craft: Record,
    craft_station: usize,
    craft_recipe: usize,
    craft_item: usize,
    craft_amount: usize,
    craft_selections: (usize, usize),
    craft_type: usize,
    craft_recipe_type: u8,
    transfer: Record,
    transfer_source: usize,
    transfer_target: usize,
    transfer_source_slot: usize,
    transfer_target_slot: usize,
    transfer_type: usize,
    transfer_flags: usize,
    transfer_amount: usize,
    /// `InventoryActionType` All and CustomAmount.
    all: u8,
    custom: u8,
    /// The flags byte that moves every stack.
    move_all_stacks: u8,
    equipment: Record,
    equipment_slot: usize,
    equipment_member: usize,
    equipment_type: usize,
    equip: u8,
    item: Record,
    item_slot: usize,
    item_type: usize,
    trash: u8,
}

impl ActLayout {
    /// The layout, when the build reflects every request with each field
    /// inside it and each request inside the input.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let input = reflected_size(loader, ids::CLIENT_PLAYER_INPUT)?;
        let (craft, craft_size) = Record::resolve(
            loader,
            input,
            ids::CLIENT_PLAYER_INPUT_CRAFTING,
            ids::CRAFTING_ACTION,
            ids::CRAFTING_ACTION_VERSION,
        )?;
        let (transfer, transfer_size) = Record::resolve(
            loader,
            input,
            ids::CLIENT_PLAYER_INPUT_TRANSFER,
            ids::TRANSFER_ACTION,
            ids::TRANSFER_ACTION_VERSION,
        )?;
        let (equipment, equipment_size) = Record::resolve(
            loader,
            input,
            ids::CLIENT_PLAYER_INPUT_EQUIPMENT,
            ids::EQUIPMENT_ACTION,
            ids::EQUIPMENT_ACTION_VERSION,
        )?;
        let (item, item_size) = Record::resolve(
            loader,
            input,
            ids::CLIENT_PLAYER_INPUT_ITEM,
            ids::ITEM_ACTION,
            ids::ITEM_ACTION_VERSION,
        )?;
        let selections = reflected_size(loader, ids::RECIPE_INPUT_SELECTIONS)?;
        let craft_field = |id, bytes| field(loader, id, bytes, craft_size);
        let transfer_field = |id, bytes| field(loader, id, bytes, transfer_size);
        let flag = catalogue::constant(ids::TRANSFER_FLAG_MOVE_ALL_STACKS, &loader.reflection)
            .filter(|&bit| bit < 8)
            .ok_or(ids::TRANSFER_FLAG_MOVE_ALL_STACKS)?;
        Ok(Self {
            craft_station: craft_field(ids::CRAFTING_ACTION_STATION, 4)?,
            craft_recipe: craft_field(ids::CRAFTING_ACTION_RECIPE, 4)?,
            craft_item: craft_field(ids::CRAFTING_ACTION_ITEM, 4)?,
            craft_amount: craft_field(ids::CRAFTING_ACTION_AMOUNT, 2)?,
            craft_selections: (
                craft_field(ids::CRAFTING_ACTION_SELECTIONS, selections)?,
                selections,
            ),
            craft_type: craft_field(ids::CRAFTING_ACTION_TYPE, 1)?,
            craft_recipe_type: value(loader, ids::CRAFTING_ACTION_CRAFT_RECIPE)?,
            craft,
            transfer_source: transfer_field(ids::TRANSFER_ACTION_SOURCE, 4)?,
            transfer_target: transfer_field(ids::TRANSFER_ACTION_TARGET, 4)?,
            transfer_source_slot: transfer_field(ids::TRANSFER_ACTION_SOURCE_SLOT, 8)?,
            transfer_target_slot: transfer_field(ids::TRANSFER_ACTION_TARGET_SLOT, 8)?,
            transfer_type: transfer_field(ids::TRANSFER_ACTION_TYPE, 1)?,
            transfer_flags: transfer_field(ids::TRANSFER_ACTION_FLAGS, 1)?,
            transfer_amount: transfer_field(ids::TRANSFER_ACTION_AMOUNT, 2)?,
            all: value(loader, ids::INVENTORY_ACTION_ALL)?,
            custom: value(loader, ids::INVENTORY_ACTION_CUSTOM_AMOUNT)?,
            move_all_stacks: 1 << flag,
            transfer,
            equipment_slot: field(loader, ids::EQUIPMENT_ACTION_SLOT, 8, equipment_size)?,
            equipment_member: field(
                loader,
                ids::EQUIPMENT_ACTION_EQUIPMENT_SLOT,
                1,
                equipment_size,
            )?,
            equipment_type: field(loader, ids::EQUIPMENT_ACTION_TYPE, 1, equipment_size)?,
            equip: value(loader, ids::CHANGE_EQUIPMENT_EQUIP)?,
            equipment,
            item_slot: field(loader, ids::ITEM_ACTION_SLOT, 8, item_size)?,
            item_type: field(loader, ids::ITEM_ACTION_TYPE, 1, item_size)?,
            trash: value(loader, ids::ITEM_ACTION_TRASH)?,
            item,
        })
    }

    /// Writes `act` as a new request of the local player `player`, its
    /// version moved on from the one the record holds, so the server takes
    /// it once.
    ///
    /// # Safety
    ///
    /// - `input` is valid for reads and writes of the build's
    ///   `ClientPlayerInput`, which nothing else touches during the call.
    pub(crate) unsafe fn write(&self, input: *mut u8, player: u32, act: Act) {
        let put = |record: &Record, at: usize, bytes: &[u8]| {
            for (i, byte) in bytes.iter().enumerate() {
                // SAFETY: the caller's record; `resolve` checked each field
                // of `bytes` lies inside its request, inside the input.
                unsafe { write_at(input, record.base + at + i, *byte) };
            }
        };
        let own = |entity: u32| if entity == 0 { player } else { entity };
        match act {
            Act::Craft {
                station,
                recipe,
                count,
            } => {
                let r = &self.craft;
                put(r, self.craft_station, &own(station).to_ne_bytes());
                put(r, self.craft_recipe, &recipe.to_ne_bytes());
                put(r, self.craft_item, &0u32.to_ne_bytes());
                put(r, self.craft_amount, &count.to_ne_bytes());
                let (at, size) = self.craft_selections;
                for i in 0..size {
                    put(r, at + i, &[0]);
                }
                put(r, self.craft_type, &[self.craft_recipe_type]);
                // SAFETY: the caller's record.
                unsafe { r.bump(input) };
            }
            Act::Move { from, to, count } => {
                let r = &self.transfer;
                let entity = |slot: u64| u32::try_from(slot & 0xffff_ffff).unwrap_or(0);
                put(r, self.transfer_source, &entity(from).to_ne_bytes());
                put(r, self.transfer_target, &entity(to).to_ne_bytes());
                put(r, self.transfer_source_slot, &from.to_ne_bytes());
                put(r, self.transfer_target_slot, &to.to_ne_bytes());
                let kind = if count == 0 { self.all } else { self.custom };
                put(r, self.transfer_type, &[kind]);
                put(r, self.transfer_flags, &[0]);
                put(r, self.transfer_amount, &count.to_ne_bytes());
                // SAFETY: the caller's record.
                unsafe { r.bump(input) };
            }
            Act::MoveAll { from, to } => {
                let r = &self.transfer;
                put(r, self.transfer_source, &own(from).to_ne_bytes());
                put(r, self.transfer_target, &own(to).to_ne_bytes());
                put(r, self.transfer_source_slot, &0u64.to_ne_bytes());
                put(r, self.transfer_target_slot, &0u64.to_ne_bytes());
                put(r, self.transfer_type, &[self.all]);
                put(r, self.transfer_flags, &[self.move_all_stacks]);
                put(r, self.transfer_amount, &0u16.to_ne_bytes());
                // SAFETY: the caller's record.
                unsafe { r.bump(input) };
            }
            Act::Equip { slot, equipment } => {
                let r = &self.equipment;
                put(r, self.equipment_slot, &slot.to_ne_bytes());
                put(r, self.equipment_member, &[equipment]);
                put(r, self.equipment_type, &[self.equip]);
                // SAFETY: the caller's record.
                unsafe { r.bump(input) };
            }
            Act::Trash { slot } => {
                let r = &self.item;
                put(r, self.item_slot, &slot.to_ne_bytes());
                put(r, self.item_type, &[self.trash]);
                // SAFETY: the caller's record.
                unsafe { r.bump(input) };
            }
        }
    }
}

/// The version after `old` that no request of the game's carries.
const fn next_version(old: u32) -> u32 {
    old.wrapping_add(1) | LOADER_VERSION
}

/// The layout of client 26430625, for tests over records of their own.
#[cfg(test)]
pub(crate) const TEST_LAYOUT: ActLayout = ActLayout {
    craft: Record {
        base: 0xb4,
        version: 0x3c,
    },
    craft_station: 0,
    craft_recipe: 4,
    craft_item: 8,
    craft_amount: 12,
    craft_selections: (24, 32),
    craft_type: 64,
    craft_recipe_type: 0,
    transfer: Record {
        base: 0,
        version: 0,
    },
    transfer_source: 4,
    transfer_target: 8,
    transfer_source_slot: 12,
    transfer_target_slot: 20,
    transfer_type: 28,
    transfer_flags: 29,
    transfer_amount: 30,
    all: 2,
    custom: 3,
    move_all_stacks: 1 << 2,
    equipment: Record {
        base: 0x1c0,
        version: 8,
    },
    equipment_slot: 0,
    equipment_member: 12,
    equipment_type: 13,
    equip: 0,
    item: Record {
        base: 0x1dc,
        version: 16,
    },
    item_slot: 0,
    item_type: 20,
    trash: 2,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn at<T: bytemuck::AnyBitPattern>(input: &[u8], from: usize) -> T {
        bytemuck::pod_read_unaligned(&input[from..from + size_of::<T>()])
    }

    fn write(input: &mut [u8], act: Act) {
        // SAFETY: a record of the build's size, not shared.
        unsafe { TEST_LAYOUT.write(input.as_mut_ptr(), 77, act) };
    }

    #[test]
    fn a_hand_craft_names_the_player_as_its_station_and_moves_the_version_on() {
        let mut input = vec![0xaau8; 0x600];
        input[0xf0..0xf4].copy_from_slice(&5u32.to_ne_bytes());
        write(
            &mut input,
            Act::Craft {
                station: 0,
                recipe: 0x7cd8_0a04,
                count: 3,
            },
        );
        assert_eq!(at::<u32>(&input, 0xb4), 77, "the player's own");
        assert_eq!(at::<u32>(&input, 0xb8), 0x7cd8_0a04);
        assert_eq!(at::<u32>(&input, 0xbc), 0, "the recipe's own item");
        assert_eq!(at::<u16>(&input, 0xc0), 3);
        assert!(input[0xcc..0xec].iter().all(|b| *b == 0), "no choices");
        assert_eq!(input[0xf4], 0, "CraftRecipe");
        assert_eq!(at::<u32>(&input, 0xf0), 6 | LOADER_VERSION);
        write(
            &mut input,
            Act::Craft {
                station: 900,
                recipe: 1,
                count: 1,
            },
        );
        assert_eq!(at::<u32>(&input, 0xb4), 900, "a station");
        assert_eq!(at::<u32>(&input, 0xf0), 7 | LOADER_VERSION);
    }

    #[test]
    fn a_move_names_both_slots_and_a_count_or_the_whole_stack() {
        let mut input = vec![0u8; 0x600];
        let (from, to) = (0x41 | 2 << 32, 0x42 | 5 << 32);
        write(&mut input, Act::Move { from, to, count: 4 });
        assert_eq!((at::<u32>(&input, 4), at::<u32>(&input, 8)), (0x41, 0x42));
        assert_eq!((at::<u64>(&input, 12), at::<u64>(&input, 20)), (from, to));
        assert_eq!((input[28], at::<u16>(&input, 30)), (3, 4), "custom amount");
        write(&mut input, Act::Move { from, to, count: 0 });
        assert_eq!(input[28], 2, "all");
        assert_eq!(at::<u32>(&input, 0), 2 | LOADER_VERSION);
    }

    #[test]
    fn take_all_moves_every_stack_into_the_player_s_own_inventory() {
        let mut input = vec![0xffu8; 0x600];
        write(&mut input, Act::MoveAll { from: 500, to: 0 });
        assert_eq!((at::<u32>(&input, 4), at::<u32>(&input, 8)), (500, 77));
        assert_eq!(
            (at::<u64>(&input, 12), at::<u64>(&input, 20)),
            (0, 0),
            "no slots"
        );
        assert_eq!((input[28], input[29]), (2, 1 << 2));
    }

    #[test]
    fn equip_and_trash_write_their_own_requests() {
        let mut input = vec![0u8; 0x600];
        write(
            &mut input,
            Act::Equip {
                slot: 0x41 | 1 << 32,
                equipment: 12,
            },
        );
        assert_eq!(at::<u64>(&input, 0x1c0), 0x41 | 1 << 32);
        assert_eq!((input[0x1cc], input[0x1cd]), (12, 0));
        assert_eq!(at::<u32>(&input, 0x1c8), 1 | LOADER_VERSION);
        write(&mut input, Act::Trash { slot: 0x43 });
        assert_eq!((at::<u64>(&input, 0x1dc), input[0x1dc + 20]), (0x43, 2));
        assert_eq!(at::<u32>(&input, 0x1dc + 16), 1 | LOADER_VERSION);
    }
}
