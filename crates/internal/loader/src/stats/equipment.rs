//! The items an entity wears: the stacks of its inventories of the
//! equipment category, read through its `InventorySetup` as the game links
//! them.

use std::sync::OnceLock;

use crate::host::Loader;

/// The equipment category of an inventory link.
const EQUIPMENT: u8 = 2;
/// The virtual category, whose link leads to more links.
const VIRTUAL: u8 = 5;
/// Links in an `InventorySetup`, and stacks in an `Inventory`.
const LINKS: usize = 16;
const SLOTS: usize = 8;
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
        let reflection = &loader.reflection;
        let ty = |name: &str| {
            let info = reflection.type_info(name).ok()?;
            crate::world::type_hash(loader, info.va())
        };
        let offset = |ty: &str, field: &str| {
            reflection
                .field_offset(ty, field)
                .ok()
                .and_then(|v| usize::try_from(v).ok())
        };
        let data = offset("keen::ItemStack", "data")?;
        Some(Layout {
            setup: ty("keen::ecs::InventorySetup")?,
            inventory: ty("keen::ecs::Inventory")?,
            links: offset("keen::ecs::InventorySetup", "linksEntities")?,
            categories: offset("keen::ecs::InventorySetup", "linksCategories")?,
            initialized: offset("keen::ecs::InventorySetup", "isInitialized")?,
            stack: usize::try_from(reflection.type_info("keen::ItemStack").ok()?.size()).ok()?,
            count: data,
            pide: data + size_of::<u32>(),
        })
    })
}

/// Reads a `T` at `base + offset`.
///
/// # Safety
///
/// The address holds a readable `T`.
unsafe fn read<T: Copy>(base: *const core::ffi::c_void, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe {
        base.cast::<u8>()
            .wrapping_add(offset)
            .cast::<T>()
            .read_unaligned()
    }
}

/// The `(entity, item)` of every item `entity` wears that has an entity of
/// its own; empty for an entity without inventories. On the game thread.
pub(super) fn worn(loader: &Loader, entity: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    if let Some(layout) = layout(loader) {
        walk(loader, &layout, entity, 0, &mut out);
    }
    out
}

fn walk(loader: &Loader, layout: &Layout, root: u32, depth: u32, out: &mut Vec<(u32, u32)>) {
    let Ok(setup) = crate::world::component(loader, root, layout.setup) else {
        return;
    };
    // SAFETY: a live InventorySetup, on the game thread, holds its flag.
    if unsafe { read::<u8>(setup, layout.initialized) } == 0 {
        return;
    }
    for i in 0..LINKS {
        // SAFETY: the setup holds 16 links and 16 categories.
        let link: u32 = unsafe { read(setup, layout.links + i * size_of::<u32>()) };
        if link == 0 {
            break;
        }
        // SAFETY: as above.
        let category: u8 = unsafe { read(setup, layout.categories + i) };
        if category == VIRTUAL && depth < MAX_DEPTH {
            walk(loader, layout, link, depth + 1, out);
            continue;
        }
        if category != EQUIPMENT {
            continue;
        }
        let Ok(slots) = crate::world::component(loader, link, layout.inventory) else {
            continue;
        };
        for index in 0..SLOTS {
            let at = index * layout.stack;
            // SAFETY: a live Inventory holds 8 stacks of the reflected size.
            let item: u32 = unsafe { read(slots, at) };
            // SAFETY: as above.
            let count: u32 = unsafe { read(slots, at + layout.count) };
            // SAFETY: as above.
            let pide: u32 = unsafe { read(slots, at + layout.pide) };
            if item != 0 && count != 0 && pide != 0 {
                out.push((pide, item));
            }
        }
    }
}
