//! The items an entity wears: the stacks of its inventories of the
//! equipment category, read through its `InventorySetup` as the game links
//! them.

use std::ops::ControlFlow;
use std::sync::OnceLock;

use enshwick_core::__private::inventory_links::{self, SetupLayout};
use enshwick_core::Entity;
use enshwick_core::inventory::InventoryCategory;
use enshwick_core::raw::read_at;

use crate::engine::catalogue::{self, ids};
use crate::engine::loader::Loader;

use crate::world::transactions::props::SLOTS;

/// How deep virtual links are followed.
const MAX_DEPTH: u32 = 2;

/// Where the walk reads, from the game's reflection.
#[derive(Debug, Clone, Copy)]
struct Layout {
    setup: u32,
    inventory: u32,
    links: SetupLayout,
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
            links: SetupLayout {
                links: catalogue::offset(loader, ids::INVENTORY_SETUP_LINKS)?,
                categories: catalogue::offset(loader, ids::INVENTORY_SETUP_CATEGORIES)?,
                initialized: catalogue::setup_initialized_at(loader)?,
            },
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
    let Some(layout) = layout(loader) else {
        return out;
    };
    let mut setup = |entity| {
        crate::engine::world::component(loader, entity, layout.setup)
            .ok()
            .map(<*const _>::cast)
    };
    let mut visit = |link, category| {
        if category == InventoryCategory::EQUIPMENT {
            worn_in(loader, &layout, link, &mut out);
        }
        ControlFlow::<()>::Continue(())
    };
    // SAFETY: `world::component` returns only the entity's live
    // `InventorySetup`, read on the game thread that owns it, laid out as
    // the reflected `layout.links` says.
    let _walked = unsafe {
        inventory_links::walk(
            entity,
            MAX_DEPTH,
            Entity::NONE,
            &layout.links,
            &mut setup,
            &mut visit,
        )
    };
    out
}

/// Adds to `out` the `(entity, item)` of each item with an entity of its
/// own in the equipment inventory `link`.
fn worn_in(loader: &Loader, layout: &Layout, link: Entity, out: &mut Vec<(u32, u32)>) {
    let Ok(slots) = crate::engine::world::component(loader, link, layout.inventory) else {
        return;
    };
    for index in 0..SLOTS {
        let at = index * layout.stack;
        // SAFETY: the live `Inventory` from `world::component`, on the game
        // thread, holds 8 stacks of the reflected `ItemStack` size, `index`
        // is below 8, and a stack starts with its `u32` item id.
        let item: u32 = unsafe { read_at(slots.cast(), at) };
        // SAFETY: the stack at `at` lies in the live `Inventory` (8 stacks,
        // `index` below 8) and holds its `u32` count at the reflected `data`
        // offset.
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
