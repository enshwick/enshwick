//! The world's side of an anchor, read on the server's game thread: a placed
//! thing's key, where an item entity sits, and which item entity a slot
//! holds. Layouts come from reflection (server 399, client 426).

use std::sync::OnceLock;

use enshwick::Entity;
use enshwick::raw::read_at;
use enshwick_gamedata::catalogue as names;

use super::anchor::{Anchor, Placed, TRANSFORM_KEY};
use crate::host::Loader;

/// `link` of an item in its root's own inventory rather than a linked one.
const OWN: u8 = u8::MAX;
/// Linked inventories an `InventorySetup` holds.
const LINKS: u8 = 16;
/// Stacks an `Inventory` holds.
const SLOTS: u8 = 8;

/// The reflected types and offsets an anchor reads.
#[derive(Clone, Copy, Debug)]
struct Layout {
    transform: u32,
    transform_at: usize,
    item_state: u32,
    slot_entity: usize,
    slot_index: usize,
    item: usize,
    owner: u32,
    owner_entity: usize,
    setup: u32,
    links: usize,
    inventory: u32,
    stack: usize,
    stack_pide: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let ty = |id: &str| {
                let info = loader.reflection.type_info(names::type_name(id)).ok()?;
                crate::world::type_hash(info.va())
            };
            let at = |id: &str| usize::try_from(names::constant(id, &loader.reflection)?).ok();
            let transform_type = loader
                .reflection
                .type_info(names::type_name(names::CURRENT_TRANSFORM))
                .ok()?;
            let stack = loader
                .reflection
                .type_info(names::type_name(names::ITEM_STACK))
                .ok()?;
            let layout = Layout {
                transform: ty(names::CURRENT_TRANSFORM)?,
                transform_at: at(names::CURRENT_TRANSFORM_TRANSFORM)?,
                item_state: ty(names::ITEM_STATE)?,
                slot_entity: at(names::ITEM_STATE_SLOT)? + at(names::INVENTORY_SLOT_ENTITY)?,
                slot_index: at(names::ITEM_STATE_SLOT)? + at(names::INVENTORY_SLOT_INDEX)?,
                item: at(names::ITEM_STATE_ITEM)?,
                owner: ty(names::OWNER)?,
                owner_entity: at(names::OWNER_ENTITY)?,
                setup: ty(names::INVENTORY_SETUP)?,
                links: at(names::INVENTORY_SETUP_LINKS)?,
                inventory: ty(names::INVENTORY)?,
                stack: usize::try_from(stack.size()).ok()?,
                stack_pide: at(names::ITEM_STACK_PIDE)?,
            };
            // The key's bytes lie inside the transform.
            let whole = usize::try_from(transform_type.size()).ok()?;
            (layout.transform_at + TRANSFORM_KEY <= whole && layout.stack_pide + 4 <= layout.stack)
                .then_some(layout)
        })
        .as_ref()
}

/// The `CurrentTransform` hash, for the walk over placed things.
pub(crate) fn transform_type(loader: &Loader) -> Option<u32> {
    layout(loader).map(|l| l.transform)
}

/// The data of `entity`'s component `ty`.
fn component(loader: &Loader, entity: u32, ty: u32) -> Option<*const u8> {
    crate::world::component(loader, Entity(entity), ty)
        .ok()
        .map(<*const core::ffi::c_void>::cast)
}

/// The key of a placed thing: `None` for a player, whose character the
/// world save does not hold, and for an entity with no template or place.
pub(crate) fn placed(loader: &Loader, entity: u32) -> Option<Placed> {
    let layout = layout(loader)?;
    if crate::world::player_entities(loader)
        .ok()?
        .contains(&entity)
    {
        return None;
    }
    let template = crate::world::template(loader, Entity(entity)).ok()?;
    let data = component(loader, entity, layout.transform)?;
    // SAFETY: `data` is the entity's live `CurrentTransform`, read on the game
    // thread (checked by `world::component`); `layout` checked that its
    // reflected transform holds `TRANSFORM_KEY` bytes from `transform_at`.
    let transform = unsafe { read_at::<[u8; TRANSFORM_KEY]>(data, layout.transform_at) };
    Some(Placed::new(template, transform))
}

/// The `link` of inventory `inventory` among `root`'s linked inventories, or
/// [`OWN`] when it is the root's own.
fn link_of(loader: &Loader, layout: &Layout, root: u32, inventory: u32) -> Option<u8> {
    if root == inventory {
        return Some(OWN);
    }
    let setup = component(loader, root, layout.setup)?;
    (0..LINKS).find(|&link| {
        // SAFETY: `setup` is the root's live `InventorySetup`, whose
        // reflected `linksEntities` holds `LINKS` `u32` entities from
        // `links`; read on the game thread (checked by `world::component`).
        let linked: u32 = unsafe { read_at(setup, layout.links + usize::from(link) * 4) };
        linked == inventory
    })
}

/// Where the item entity `pide` sits, as an anchor, and the item it holds;
/// `None` for an entity the world does not hold, and no anchor for an item
/// in no placed thing's inventory, such as a player's.
pub(crate) fn held(loader: &Loader, pide: u32) -> Option<(Option<Anchor>, u32)> {
    let layout = layout(loader)?;
    let state = component(loader, pide, layout.item_state)?;
    // SAFETY: `state` is the entity's live `ItemState`; its reflected slot id
    // holds the `u32` inventory entity and the slot byte at these offsets, its
    // `itemId` a `u32`; read on the game thread (checked by `world::component`).
    let (inventory, slot, item) = unsafe {
        (
            read_at::<u32>(state, layout.slot_entity),
            read_at::<u8>(state, layout.slot_index),
            read_at::<u32>(state, layout.item),
        )
    };
    let root = match component(loader, inventory, layout.owner) {
        // SAFETY: `owner` is the inventory's live `OwnerRelationship`, its
        // reflected owner a `u32` entity; read on the game thread.
        Some(owner) => unsafe { read_at::<u32>(owner, layout.owner_entity) },
        None => inventory,
    };
    let anchor = link_of(loader, layout, root, inventory)
        .zip(placed(loader, root))
        .map(|(link, root)| Anchor::Held { root, link, slot });
    Some((anchor, item))
}

/// The item entity in slot `slot` of the inventory at `link` of `root`, and
/// the item it holds.
pub(crate) fn in_slot(loader: &Loader, root: u32, link: u8, slot: u8) -> Option<(u32, u32)> {
    let layout = layout(loader)?;
    if slot >= SLOTS {
        return None;
    }
    let inventory = if link == OWN {
        root
    } else if link < LINKS {
        let setup = component(loader, root, layout.setup)?;
        // SAFETY: `setup` is the root's live `InventorySetup`; `link < LINKS`
        // indexes its `u32` `linksEntities`; read on the game thread.
        unsafe { read_at::<u32>(setup, layout.links + usize::from(link) * 4) }
    } else {
        return None;
    };
    let stacks = component(loader, inventory, layout.inventory)?;
    let at = usize::from(slot) * layout.stack;
    // SAFETY: `stacks` is the inventory's live `Inventory`, `SLOTS` stacks of
    // the reflected `ItemStack` size from offset 0 and `slot < SLOTS`; a
    // stack starts with its `u32` item and holds its `u32` PIDE at
    // `stack_pide`; read on the game thread.
    let (item, pide) = unsafe {
        (
            read_at::<u32>(stacks, at),
            read_at::<u32>(stacks, at + layout.stack_pide),
        )
    };
    (pide != 0).then_some((pide, item))
}
