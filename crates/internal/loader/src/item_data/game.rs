//! The world's side of an anchor, read on the server's game thread: a placed
//! thing's key, where an item entity sits, and which item entity a slot
//! holds. Layouts come from reflection (server 399, client 426).

use std::sync::OnceLock;

use enshwick::Entity;
use enshwick::raw::read_at;
use enshwick_gamedata::catalogue as names;

use super::anchor::{Anchor, CharacterId, Placed};
use crate::host::Loader;

/// `link` of an item in its root's own inventory rather than a linked one.
const OWN: u8 = u8::MAX;
/// Linked inventories an `InventorySetup` holds.
const LINKS: u8 = 16;
/// Stacks an `Inventory` holds.
const SLOTS: u8 = 8;
/// The reflected names of what the API's catalogue does not hold.
const ITEM_STATE: &str = "keen::ecs::ItemState";
const SLOT_ID: &str = "keen::InventorySlotId";

/// The reflected types and offsets an anchor reads.
#[derive(Clone, Copy, Debug)]
struct Layout {
    transform: u32,
    /// Where a `CurrentTransform` holds its position.
    position: usize,
    item_state: u32,
    slot_entity: usize,
    slot_index: usize,
    item: usize,
    owner: u32,
    owner_entity: usize,
    setup: u32,
    links: usize,
    /// Where an `InventorySetup` says its inventories are filled; `None`
    /// turns off only the wait for a joining character's inventories.
    initialized: Option<usize>,
    inventory: u32,
    stack: usize,
    stack_item: usize,
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
            let field = |ty: &str, field: &str| {
                usize::try_from(loader.reflection.field_offset(ty, field).ok()?).ok()
            };
            let transform_type = loader
                .reflection
                .type_info(names::type_name(names::ids::CURRENT_TRANSFORM))
                .ok()?;
            let stack = loader
                .reflection
                .type_info(names::type_name(names::ids::ITEM_STACK))
                .ok()?;
            let layout = Layout {
                transform: ty(names::ids::CURRENT_TRANSFORM)?,
                position: at(names::ids::CURRENT_TRANSFORM_TRANSFORM)?
                    + at(names::ids::WORLD_TRANSFORM_POSITION)?,
                item_state: ty(ITEM_STATE)?,
                slot_entity: field(ITEM_STATE, "containedInSlotId")? + field(SLOT_ID, "entityId")?,
                slot_index: field(ITEM_STATE, "containedInSlotId")? + field(SLOT_ID, "slotIndex")?,
                item: field(ITEM_STATE, "itemId")?,
                owner: ty(names::ids::OWNER)?,
                owner_entity: field(names::type_name(names::ids::OWNER), "ownerEntityId")?,
                setup: ty(names::ids::INVENTORY_SETUP)?,
                links: at(names::ids::INVENTORY_SETUP_LINKS)?,
                initialized: crate::catalogue::setup_initialized_at(loader),
                inventory: ty(names::ids::INVENTORY)?,
                stack: usize::try_from(stack.size()).ok()?,
                stack_item: field(names::type_name(names::ids::ITEM_STACK), "id")?,
                stack_pide: at(names::ids::ITEM_STACK_PIDE)?,
            };
            // Every read lies inside the reflected type it reads.
            let size =
                |name: &str| usize::try_from(loader.reflection.type_info(name).ok()?.size()).ok();
            let setup_size = size(names::type_name(names::ids::INVENTORY_SETUP))?;
            let inside = layout.position + size_of::<[i64; 3]>()
                <= usize::try_from(transform_type.size()).ok()?
                && layout.stack_pide.max(layout.stack_item) + 4 <= layout.stack
                && layout.slot_entity.max(layout.item) + 4 <= size(ITEM_STATE)?
                && layout.slot_index < size(ITEM_STATE)?
                && layout.owner_entity + 4 <= size(names::type_name(names::ids::OWNER))?
                && layout.links + usize::from(LINKS) * 4 <= setup_size
                && usize::from(SLOTS) * layout.stack
                    <= size(names::type_name(names::ids::INVENTORY))?;
            inside.then_some(Layout {
                initialized: layout.initialized.filter(|&at| at < setup_size),
                ..layout
            })
        })
        .as_ref()
}

/// Whether this build gives every layout an anchor reads; without it no
/// anchor is read, and no record may be judged by one.
pub(crate) fn readable(loader: &Loader) -> bool {
    layout(loader).is_some()
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
    if crate::world::character_entities(loader)
        .ok()?
        .contains(&entity)
    {
        return None;
    }
    let template = crate::world::template(loader, Entity(entity)).ok()?;
    let data = component(loader, entity, layout.transform)?;
    // SAFETY: `data` is the entity's live `CurrentTransform`, read on the game
    // thread (checked by `world::component`); `layout` checked that it holds
    // the three `i64` of its reflected position from `position`.
    let position = unsafe { read_at::<[i64; 3]>(data, layout.position) };
    Some(Placed::new(template, position))
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

/// Where the item entity `pide` sits, as an anchor, the item it holds and
/// the root whose inventory holds it; `None` for an entity the world does
/// not hold. An item in no placed thing's inventory is anchored to the
/// character `character_of` names for its root, which runs only then. The
/// anchor is `None` for an item in neither.
pub(crate) fn held(
    loader: &Loader,
    pide: u32,
    character_of: impl FnOnce(Entity) -> Option<CharacterId>,
) -> Option<Held> {
    let layout = layout(loader)?;
    let state = component(loader, pide, layout.item_state)?;
    // SAFETY: `state` is the entity's live `ItemState`, whose reflected slot
    // id holds its `u32` inventory entity at `slot_entity` (in bounds, checked
    // by `layout`); read on the game thread (checked by `world::component`).
    let inventory = unsafe { read_at::<u32>(state, layout.slot_entity) };
    // SAFETY: `state` is the entity's live `ItemState`, whose reflected slot
    // id holds its slot byte at `slot_index` (in bounds, checked by
    // `layout`); read on the game thread (checked by `world::component`).
    let slot = unsafe { read_at::<u8>(state, layout.slot_index) };
    // SAFETY: `state` is the entity's live `ItemState`, which holds its `u32`
    // `itemId` at `item` (in bounds, checked by `layout`); read on the game
    // thread (checked by `world::component`).
    let item = unsafe { read_at::<u32>(state, layout.item) };
    let root = match component(loader, inventory, layout.owner) {
        // SAFETY: `owner` is the inventory's live `OwnerRelationship`, its
        // reflected owner a `u32` entity; read on the game thread.
        Some(owner) => unsafe { read_at::<u32>(owner, layout.owner_entity) },
        None => inventory,
    };
    let anchor = link_of(loader, layout, root, inventory).and_then(|link| {
        placed(loader, root)
            .map(|root| Anchor::Held { root, link, slot })
            .or_else(|| {
                character_of(Entity(root)).map(|character| Anchor::Carried {
                    character,
                    link,
                    slot,
                })
            })
    });
    Some(Held { anchor, item, root })
}

/// An item entity's place: its anchor, the item it holds and its root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    pub(crate) anchor: Option<Anchor>,
    pub(crate) item: u32,
    pub(crate) root: u32,
}

/// Whether the inventories of `root`, a character, are filled: its
/// `InventorySetup` says it is initialized (server 399). `None` when that
/// does not read: a build without the field, or no setup.
pub(crate) fn filled(loader: &Loader, root: Entity) -> Option<bool> {
    let layout = layout(loader)?;
    let initialized = layout.initialized?;
    let setup = component(loader, root.0, layout.setup)?;
    // SAFETY: `setup` is the root's live `InventorySetup`, which holds its
    // `isInitialized` byte at `initialized` (inside the type, checked by
    // `layout`); read on the game thread (checked by `world::component`).
    Some(unsafe { crate::transactions::props::setup_initialized(setup, initialized) })
}

/// Whether a joining character `root` may be looked at for its carried
/// items: its inventories are [`filled`]; on a build without the field,
/// always, since a look only takes back what it finds.
pub(crate) fn ready(loader: &Loader, root: Entity) -> bool {
    match layout(loader) {
        Some(layout) if layout.initialized.is_none() => true,
        Some(_) => filled(loader, root) == Some(true),
        None => false,
    }
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
    // the reflected `ItemStack` size from offset 0 (checked by `layout`) and
    // `slot < SLOTS`; the stack holds its `u32` item id at the reflected
    // `stack_item`, inside the stack (checked by `layout`); read on the game
    // thread (checked by `world::component`).
    let item = unsafe { read_at::<u32>(stacks, at + layout.stack_item) };
    // SAFETY: `stacks` is the inventory's live `Inventory`, `SLOTS` stacks of
    // the reflected `ItemStack` size from offset 0 (checked by `layout`) and
    // `slot < SLOTS`; the stack holds its `u32` PIDE at `stack_pide`, inside
    // the stack (checked by `layout`); read on the game thread (checked by
    // `world::component`).
    let pide = unsafe { read_at::<u32>(stacks, at + layout.stack_pide) };
    (pide != 0).then_some((pide, item))
}
