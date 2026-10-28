//! The items an entity wears: the stacks of its inventories of the
//! equipment category, read through its `InventorySetup` as the game links
//! them.

use std::sync::OnceLock;

use enshwick::Entity;
use enshwick::inventory::InventoryCategory;
use enshwick::raw::read_at;

use crate::catalogue::{self, ids};
use crate::host::Loader;

use crate::transactions::props::{LINKS, SLOTS};

/// The equipment category of an inventory link.
const EQUIPMENT: u8 = InventoryCategory::EQUIPMENT.0;
/// The virtual category, whose link leads to more links.
const VIRTUAL: u8 = InventoryCategory::VIRTUAL.0;
/// How deep virtual links are followed.
const MAX_DEPTH: u32 = 2;

/// Where the walk reads, from the game's reflection.
#[derive(Debug, Clone, Copy)]
struct Layout {
    setup: u32,
    inventory: u32,
    links: usize,
    categories: usize,
    initialized: usize,
    stack: usize,
    count: usize,
    pide: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<Layout> {
    *LAYOUT.get_or_init(|| {
        let count = catalogue::offset(loader, ids::ITEM_STACK_COUNT)?;
        Some(Layout {
            setup: catalogue::component_hash(loader, ids::INVENTORY_SETUP)?,
            inventory: catalogue::component_hash(loader, ids::INVENTORY)?,
            links: catalogue::offset(loader, ids::INVENTORY_SETUP_LINKS)?,
            categories: catalogue::offset(loader, ids::INVENTORY_SETUP_CATEGORIES)?,
            initialized: catalogue::offset(loader, ids::INVENTORY_SETUP_INITIALIZED)?,
            stack: catalogue::size(loader, ids::ITEM_STACK)?,
            count,
            pide: count + size_of::<u32>(),
        })
    })
}

/// The `(entity, item)` of every item `entity` wears that has an entity of
/// its own; empty for an entity without inventories. On the game thread.
pub(super) fn worn(loader: &Loader, entity: Entity) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    if let Some(layout) = layout(loader) {
        walk(loader, &layout, entity.0, 0, &mut out);
    }
    out
}

fn walk(loader: &Loader, layout: &Layout, root: u32, depth: u32, out: &mut Vec<(u32, u32)>) {
    let Ok(setup) = crate::world::component(loader, Entity(root), layout.setup) else {
        return;
    };
    // SAFETY: `world::component` returned the entity's live `InventorySetup`,
    // read on the game thread that owns it; `isInitialized` is a one-byte
    // field at the offset reflection gave.
    if unsafe { read_at::<u8>(setup.cast(), layout.initialized) } == 0 {
        return;
    }
    for i in 0..LINKS {
        // SAFETY: the live `InventorySetup` from `world::component`, on the
        // game thread, holds 16 `u32` links at the reflected `linksEntities`
        // offset, and `i` is below 16.
        let link: u32 = unsafe { read_at(setup.cast(), layout.links + i * size_of::<u32>()) };
        if link == 0 {
            break;
        }
        // SAFETY: the live `InventorySetup` from `world::component`, on the
        // game thread, holds 16 one-byte categories at the reflected
        // `linksCategories` offset, and `i` is below 16.
        let category: u8 = unsafe { read_at(setup.cast(), layout.categories + i) };
        if category == VIRTUAL && depth < MAX_DEPTH {
            walk(loader, layout, link, depth + 1, out);
            continue;
        }
        if category != EQUIPMENT {
            continue;
        }
        let Ok(slots) = crate::world::component(loader, Entity(link), layout.inventory) else {
            continue;
        };
        for index in 0..SLOTS {
            let at = index * layout.stack;
            // SAFETY: the live `Inventory` from `world::component`, on the
            // game thread, holds 8 stacks of the reflected `ItemStack` size,
            // `index` is below 8, and a stack starts with its `u32` item id.
            let item: u32 = unsafe { read_at(slots.cast(), at) };
            // SAFETY: the stack at `at` lies in the live `Inventory` (8 stacks,
            // `index` below 8) and holds its `u32` count at the reflected
            // `data` offset.
            let count: u32 = unsafe { read_at(slots.cast(), at + layout.count) };
            // SAFETY: the stack at `at` lies in the live `Inventory` (8 stacks,
            // `index` below 8) and holds its `u32` entity id right after the
            // count, inside the reflected `ItemStack` size.
            let pide: u32 = unsafe { read_at(slots.cast(), at + layout.pide) };
            if item != 0 && count != 0 && pide != 0 {
                out.push((pide, item));
            }
        }
    }
}
