//! Props in the game: resolving a placement, reading what a prop holds, spawning, finding and removing it.

use enshwick::abi::{Header, PropPlace, SpawnRequest, Status};
use enshwick::ecs::Entity;
use enshwick_gamedata::catalogue;

use crate::host::Loader;
use crate::transactions::props::{FactoryRead, Prop, StateReads};

/// Metres to the game's 32.32 fixed point.
const FIXED_ONE: f64 = 4_294_967_296.0;
/// Links an `InventorySetup` holds, stacks a sub-inventory holds, how deep
/// virtual links nest, and the virtual link category, as the game keeps
/// inventories (server 399, client 426; the API's inventory reader walks
/// them the same way).
const LINKS: usize = 16;
const SLOTS: usize = 8;
const MAX_DEPTH: u32 = 4;
const VIRTUAL: u8 = 5;

/// The offset the catalogue id `id` names on this build.
fn offset(loader: &Loader, id: &str) -> Result<usize, Status> {
    catalogue::constant(id, &loader.reflection)
        .and_then(|at| usize::try_from(at).ok())
        .ok_or(Status::MISSING)
}

/// The reflected size of the catalogue type `id`.
fn type_size(loader: &Loader, id: &str) -> Result<usize, Status> {
    loader
        .reflection
        .type_info(catalogue::type_name(id))
        .ok()
        .and_then(|info| usize::try_from(info.size()).ok())
        .ok_or(Status::MISSING)
}

/// The data of `entity`'s component of the catalogue type `id`, when it
/// has one; on the game thread.
fn component(loader: &Loader, entity: u32, id: &str) -> Option<*const u8> {
    let hash = catalogue::type_hash(id)?;
    crate::world::component(loader, Entity(entity), hash)
        .ok()
        .map(<*const core::ffi::c_void>::cast)
}

/// A placement box's lowest and highest corner, before it turns.
type LocalBox = ([f32; 3], [f32; 3]);
/// What an item places: its template GUID, placement box and logic flag.
type ItemProp = ([u8; 16], Option<LocalBox>, bool);

/// What an item places as a prop: its template, its placement box and
/// whether it is a logic prop; [`Status::BAD_ARGS`] for an item the game
/// does not have or that places nothing, [`Status::MISSING`] while the
/// loader knows no item registry.
fn item_prop(loader: &Loader, item: u32) -> Result<ItemProp, Status> {
    let info = crate::inventory::item_info_at(loader, item)?.ok_or(Status::BAD_ARGS)?;
    let template_at = offset(loader, catalogue::ITEM_PLACED_ENTITY)?;
    // SAFETY: `info` is the item's live `ItemInfo` from the game's lookup,
    // with its placed template's 16-byte GUID at the reflected path.
    let template: [u8; 16] = unsafe { enshwick::raw::read_at(info, template_at) };
    if template == [0; 16] {
        return Err(Status::BAD_ARGS);
    }
    let [min_at, max_at] = catalogue::ITEM_PLACEMENT_BOX;
    let local_box = match (offset(loader, min_at), offset(loader, max_at)) {
        (Ok(min_at), Ok(max_at)) => {
            // SAFETY: `info` is the item's live `ItemInfo`, with its
            // placement box's lowest corner, three `f32`, at the reflected
            // path.
            let min: [f32; 3] = unsafe { enshwick::raw::read_at(info, min_at) };
            // SAFETY: `info` is the item's live `ItemInfo`, with its
            // placement box's highest corner, three `f32`, at the reflected
            // path.
            let max: [f32; 3] = unsafe { enshwick::raw::read_at(info, max_at) };
            let finite = min.iter().chain(&max).all(|v| v.is_finite());
            (finite && (0..3).any(|axis| max[axis] > min[axis])).then_some((min, max))
        }
        _ => None,
    };
    let logic = match (
        offset(loader, catalogue::ITEM_FLAGS),
        catalogue::constant(catalogue::ITEM_FLAG_IS_LOGIC_ENTITY, &loader.reflection),
    ) {
        (Ok(flags_at), Some(bit)) if bit < 32 => {
            // SAFETY: `info` is the item's live `ItemInfo`, with its `u32`
            // flags at the reflected offset.
            let flags: u32 = unsafe { enshwick::raw::read_at(info, flags_at) };
            flags & (1 << bit) != 0
        }
        _ => false,
    };
    Ok((template, local_box, logic))
}

/// A position in metres as the game's 32.32 fixed point; the caller keeps
/// it finite, at 0 or more and within the world.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a position below 2^30 metres fits 32.32 fixed point in an i64"
)]
fn fixed(metres: f64) -> i64 {
    (metres * FIXED_ONE) as i64
}

/// The prop `request` asks to place, checked: its item's template, box
/// and logic flag. [`Status::BAD_ARGS`] for a position or orientation out
/// of range or an item that places nothing. Any thread: it reads only the
/// item registry, which the world sets at its start.
pub(crate) fn resolve(loader: &Loader, request: &PropPlace) -> Result<Prop, Status> {
    let position_ok = request.position.iter().all(|axis| {
        axis.is_finite() && *axis >= 0.0 && *axis < f64::from(enshwick::abi::Voxel::END)
    });
    let norm: f32 = request.orientation.iter().map(|v| v * v).sum();
    if !position_ok || !norm.is_finite() || norm < 1e-6 || request.reserved != 0 {
        return Err(Status::BAD_ARGS);
    }
    let (template, local_box, logic) = item_prop(loader, request.item)?;
    Ok(Prop {
        template,
        item: request.item,
        position: request.position.map(fixed),
        orientation: request.orientation,
        local_box,
        logic,
    })
}

/// Spawns `prop` as the game's place-prop op does: its template, standing
/// where it says, with the item it came from and without random loot when
/// destroyed; the new entity. On the game thread at a quiet moment.
pub(super) fn spawn(loader: &Loader, prop: &Prop) -> Result<u32, Status> {
    let used_item = catalogue::type_hash(catalogue::USED_ITEM).ok_or(Status::MISSING)?;
    let no_loot =
        catalogue::type_hash(catalogue::DISABLE_RANDOM_LOOT_ON_DESTROY).ok_or(Status::MISSING)?;
    let mut item = vec![0u8; type_size(loader, catalogue::USED_ITEM)?];
    let bytes = prop.item.to_le_bytes();
    item.get_mut(..bytes.len())
        .ok_or(Status::MISSING)?
        .copy_from_slice(&bytes);
    let tag = vec![0u8; type_size(loader, catalogue::DISABLE_RANDOM_LOOT_ON_DESTROY)?];
    let request = SpawnRequest {
        header: Header::of::<SpawnRequest>(),
        template: prop.template,
        position: prop.position,
        orientation: prop.orientation,
        level: 0,
        reserved: 0,
        lifetime_ns: 0,
    };
    crate::spawns::spawn_with(loader, &request, vec![(used_item, item), (no_loot, tag)])
}

/// The prop `entity` is and the state it holds, as `HOLDS_` bits;
/// [`Status::MISSING`] when it is not a prop placed from an item. On the
/// game thread.
pub(super) fn read(loader: &Loader, entity: u32) -> Result<(Prop, u32), Status> {
    let template = crate::world::template(loader, Entity(entity)).map_err(|_| Status::MISSING)?;
    let item = crate::world::used_item(loader, Entity(entity)).ok_or(Status::MISSING)?;
    let (placed, local_box, logic) = item_prop(loader, item).map_err(|_| Status::MISSING)?;
    if placed != template {
        return Err(Status::MISSING);
    }
    let position = crate::world::fixed_position(loader, Entity(entity)).ok_or(Status::MISSING)?;
    let orientation = orientation(loader, entity).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let prop = Prop {
        template,
        item,
        position,
        orientation,
        local_box,
        logic,
    };
    Ok((prop, state(loader, entity).holds()))
}

/// Which way `entity` faces, from its `CurrentTransform`.
fn orientation(loader: &Loader, entity: u32) -> Option<[f32; 4]> {
    let data = component(loader, entity, catalogue::CURRENT_TRANSFORM)?;
    let at = offset(loader, catalogue::CURRENT_TRANSFORM_TRANSFORM).ok()?
        + offset(loader, catalogue::WORLD_TRANSFORM_ORIENTATION).ok()?;
    // SAFETY: `data` is the entity's live `CurrentTransform`, whose
    // transform holds its orientation, four `f32`, at the reflected offsets.
    Some(unsafe { enshwick::raw::read_at(data, at) })
}

/// Whether a prop like `prop` stands where it stood: an entity of its
/// item with its template at its position.
pub(super) fn stands(loader: &Loader, prop: &Prop) -> Result<bool, Status> {
    let used_item = catalogue::type_hash(catalogue::USED_ITEM).ok_or(Status::MISSING)?;
    let placed = crate::entities::with_component(loader, used_item)?;
    Ok(placed.iter().any(|place| {
        place.position == prop.position
            && crate::world::used_item(loader, Entity(place.entity)) == Some(prop.item)
            && crate::world::template(loader, Entity(place.entity)).ok() == Some(prop.template)
    }))
}

/// Asks the game to remove the prop `entity` after the next step, as its
/// tear-down does, without loot.
pub(super) fn remove(loader: &Loader, entity: u32) -> Result<(), Status> {
    crate::entities::destroy(loader, Entity(entity))
}

/// What `entity` holds that a respawn from its template would lose
/// (server 399, client 426): items in its inventory, a plant's level above
/// 0, a production station's recipes, assigned survivors and written text.
fn state(loader: &Loader, entity: u32) -> StateReads {
    let read_u32 = |id: &str, field: &str| -> Option<u32> {
        let data = component(loader, entity, id)?;
        let at = offset(loader, field).ok()?;
        // SAFETY: `data` is the entity's live component of the catalogue
        // type `id`, with the `u32` field `field` at its reflected offset.
        Some(unsafe { enshwick::raw::read_at(data, at) })
    };
    let factory = component(loader, entity, catalogue::FACTORY_STATION).and_then(|data| {
        let (state_at, running_at, next_at) = (
            offset(loader, catalogue::FACTORY_STATE).ok()?,
            offset(loader, catalogue::FACTORY_RUNNING).ok()?,
            offset(loader, catalogue::FACTORY_NEXT).ok()?,
        );
        // SAFETY: `data` is the entity's live `FactoryStation`, with its
        // `u8` state at the reflected offset.
        let state: u8 = unsafe { enshwick::raw::read_at(data, state_at) };
        // SAFETY: `data` is the entity's live `FactoryStation`, with its
        // `u32` running recipe at the reflected offset.
        let running: u32 = unsafe { enshwick::raw::read_at(data, running_at) };
        // SAFETY: `data` is the entity's live `FactoryStation`, with its
        // `u32` next recipe at the reflected offset.
        let next: u32 = unsafe { enshwick::raw::read_at(data, next_at) };
        Some(FactoryRead {
            state,
            running: running != 0,
            next: next != 0,
        })
    });
    let store_npcs = component(loader, entity, catalogue::STORE_NPCS).is_some_and(|data| {
        offset(loader, catalogue::STORE_NPCS_IDS).is_ok_and(|at| {
            // SAFETY: `data` is the entity's live `StoreAssociatedNpcIds`,
            // with its two `u32` survivor ids at the reflected offset.
            let ids: [u32; 2] = unsafe { enshwick::raw::read_at(data, at) };
            ids != [0, 0]
        })
    });
    let text = component(loader, entity, catalogue::CUSTOM_PLAYER_STRING).is_some_and(|data| {
        offset(loader, catalogue::CUSTOM_PLAYER_STRING_ID).is_ok_and(|at| {
            // SAFETY: `data` is the entity's live `CustomPlayerString`, with
            // its `u64` string id at the reflected offset.
            let id: u64 = unsafe { enshwick::raw::read_at(data, at) };
            id != 0
        })
    });
    StateReads {
        items: holds_items(loader, entity, 0),
        grown: component(loader, entity, catalogue::GROWTH).is_some()
            && read_u32(catalogue::LEVEL, catalogue::LEVEL_DATA).is_some_and(|level| level > 0),
        factory,
        bed_npc: read_u32(catalogue::NPC_BED, catalogue::NPC_BED_NPC).is_some_and(|npc| npc != 0),
        store_npcs,
        text,
        fuel: fuel(loader, entity),
        logic: wired(loader, entity),
        trigger: component(loader, entity, catalogue::LOGIC_TRIGGER).is_some()
            || component(loader, entity, catalogue::LOGIC_TRIGGER_SHAPE).is_some(),
    }
}

/// The settings of the logic trigger `entity`, as bytes to compare: the
/// settings its `LogicGraphTrigger` uses and its `LogicTriggerShape`
/// whole, leaving out what the game changes on its own (its query and its
/// new-settings flag); `None` for a prop that is no trigger,
/// [`Status::MISSING`] while it is not in the world. On the game thread.
pub(super) fn trigger_settings(loader: &Loader, entity: u32) -> Result<Option<Vec<u8>>, Status> {
    crate::world::template(loader, Entity(entity)).map_err(|_| Status::MISSING)?;
    let trigger = component(loader, entity, catalogue::LOGIC_TRIGGER);
    let shape = component(loader, entity, catalogue::LOGIC_TRIGGER_SHAPE);
    if trigger.is_none() && shape.is_none() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    if let Some(data) = trigger {
        let at = offset(loader, catalogue::LOGIC_TRIGGER_CONFIG)?;
        // SAFETY: `data` is the entity's live `LogicGraphTrigger`, with its
        // `u32` settings id at the reflected offset.
        let config: u32 = unsafe { enshwick::raw::read_at(data, at) };
        bytes.extend_from_slice(&config.to_le_bytes());
    }
    if let Some(data) = shape {
        let size = type_size(loader, catalogue::LOGIC_TRIGGER_SHAPE)?;
        // SAFETY: `data` is the entity's live `LogicTriggerShape`, which
        // reflection sizes at `size` bytes; read on the game thread, which
        // alone writes it, and copied before the call returns.
        let shape = unsafe { core::slice::from_raw_parts(data, size) };
        bytes.extend_from_slice(shape);
    }
    Ok(Some(bytes))
}

/// Whether `entity` holds more `Fuel` charges than a fresh prop of its
/// kind starts with (server 399, client 426).
fn fuel(loader: &Loader, entity: u32) -> bool {
    let Some(data) = component(loader, entity, catalogue::FUEL_CHARGES) else {
        return false;
    };
    let Ok(charges_at) = offset(loader, catalogue::FUEL_CHARGES_DATA) else {
        // Unreadable on this build: the safe side is to keep it.
        return true;
    };
    // SAFETY: `data` is the entity's live `Fuel`, whose charges now, an
    // `i32`, start its values at the reflected offset.
    let charges: i32 = unsafe { enshwick::raw::read_at(data, charges_at) };
    let start = component(loader, entity, catalogue::FUEL_SETUP).and_then(|setup| {
        let most_at = offset(loader, catalogue::FUEL_SETUP_MAX).ok()?;
        let share_at = offset(loader, catalogue::FUEL_SETUP_INITIAL).ok()?;
        // SAFETY: `setup` is the entity's live `FuelSetup`, with its most
        // charges, an `i32`, at the reflected offset.
        let most: i32 = unsafe { enshwick::raw::read_at(setup, most_at) };
        // SAFETY: `setup` is the entity's live `FuelSetup`, with the share
        // a fresh prop starts with, an `f32`, at the reflected offset.
        let share: f32 = unsafe { enshwick::raw::read_at(setup, share_at) };
        Some((most, share))
    });
    crate::transactions::props::fuel_above_start(charges, start)
}

/// Whether `entity` is a logic prop wired to other logic (server 399,
/// client 426): its `LogicGraphNodeConnected` writes to a graph or is read.
fn wired(loader: &Loader, entity: u32) -> bool {
    let Some(data) = component(loader, entity, catalogue::LOGIC_CONNECTED) else {
        return false;
    };
    let (Ok(writer_at), Ok(readers_at)) = (
        offset(loader, catalogue::LOGIC_CONNECTED_WRITER),
        offset(loader, catalogue::LOGIC_CONNECTED_READERS),
    ) else {
        // Unreadable on this build: the safe side is to keep it.
        return true;
    };
    // SAFETY: `data` is the entity's live `LogicGraphNodeConnected`, with
    // the `u32` graph it writes to at the reflected offset.
    let writer: u32 = unsafe { enshwick::raw::read_at(data, writer_at) };
    // SAFETY: `data` is the entity's live `LogicGraphNodeConnected`, with
    // its readers byte at the reflected offset.
    let readers: u8 = unsafe { enshwick::raw::read_at(data, readers_at) };
    crate::transactions::props::wired(writer, readers)
}

/// Whether any slot of `root`'s inventory, its linked and virtual
/// inventories included, holds an item.
fn holds_items(loader: &Loader, root: u32, depth: u32) -> bool {
    let Some(setup) = component(loader, root, catalogue::INVENTORY_SETUP) else {
        return false;
    };
    let (Ok(initialized_at), Ok(links_at), Ok(categories_at), Ok(stack)) = (
        offset(loader, catalogue::INVENTORY_SETUP_INITIALIZED),
        offset(loader, catalogue::INVENTORY_SETUP_LINKS),
        offset(loader, catalogue::INVENTORY_SETUP_CATEGORIES),
        type_size(loader, catalogue::ITEM_STACK),
    ) else {
        return false;
    };
    // SAFETY: `setup` is the entity's live `InventorySetup`, with its
    // `isInitialized` byte at the reflected offset.
    if unsafe { enshwick::raw::read_at::<u8>(setup, initialized_at) } == 0 {
        return false;
    }
    (0..LINKS).any(|i| {
        // SAFETY: `setup` is the live `InventorySetup`, whose `LINKS` `u32`
        // linked entities start at the reflected offset, and `i < LINKS`.
        let link: u32 = unsafe { enshwick::raw::read_at(setup, links_at + 4 * i) };
        if link == 0 {
            return false;
        }
        // SAFETY: `setup` is the live `InventorySetup`, whose `LINKS`
        // category bytes start at the reflected offset, and `i < LINKS`.
        let category: u8 = unsafe { enshwick::raw::read_at(setup, categories_at + i) };
        if category == VIRTUAL {
            return depth < MAX_DEPTH && holds_items(loader, link, depth + 1);
        }
        component(loader, link, catalogue::INVENTORY).is_some_and(|slots| {
            (0..SLOTS).any(|slot| {
                // SAFETY: `slots` is the link's live `Inventory`, which holds
                // `SLOTS` stacks of the reflected `ItemStack` size, each
                // starting with its `u32` item, and `slot < SLOTS`.
                let item: u32 = unsafe { enshwick::raw::read_at(slots, slot * stack) };
                item != 0
            })
        })
    })
}
