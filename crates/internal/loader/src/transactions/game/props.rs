//! Props in the game: resolving a placement, reading what a prop holds, spawning, finding and removing it.

use enshwick::abi::{Header, PropPlace, SpawnRequest, Status};
use enshwick::ecs::Entity;
use enshwick_gamedata::catalogue;

use crate::host::Loader;
use crate::transactions::props::{
    FactoryRead, LINKS, Prop, SLOTS, Setup, StateReads, fuel_above_start, holds_items, wired,
};

/// Metres to the game's 32.32 fixed point.
const FIXED_ONE: f64 = 4_294_967_296.0;

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

/// A placement box's lowest and highest corner, before it turns.
type LocalBox = ([f32; 3], [f32; 3]);
/// What an item places: its template GUID, placement box and logic flag.
type ItemProp = ([u8; 16], Option<LocalBox>, bool);

/// What an item places as a prop: its template, its placement box and
/// whether it is a logic prop; [`Status::BAD_ARGS`] for an item the game
/// does not have or that places nothing, [`Status::NOT_A_PROP`] for one
/// that places an animal or a survivor (its `IsAnimal` or `IsNpc` flag) or
/// a flame altar, which founds a base (its `requiresAltarSlot`),
/// [`Status::MISSING`] while the loader knows no item registry or this
/// build lacks those reads.
fn item_prop(loader: &Loader, item: u32) -> Result<ItemProp, Status> {
    let info = crate::inventory::item_info_at(loader, item)?.ok_or(Status::BAD_ARGS)?;
    let flags_at = offset(loader, catalogue::ITEM_FLAGS)?;
    // SAFETY: `info` is the item's live `ItemInfo`, with its `u32` flags at
    // the reflected offset.
    let flags: u32 = unsafe { enshwick::raw::read_at(info, flags_at) };
    let bit = |id: &str| {
        catalogue::constant(id, &loader.reflection)
            .filter(|&bit| bit < 32)
            .map(|bit| 1u32 << bit)
            .ok_or(Status::MISSING)
    };
    let creature = bit(catalogue::ITEM_FLAG_IS_NPC)? | bit(catalogue::ITEM_FLAG_IS_ANIMAL)?;
    let altar_at = offset(loader, catalogue::ITEM_REQUIRES_ALTAR_SLOT)?;
    // SAFETY: `info` is the item's live `ItemInfo`, with its equipment's
    // `requiresAltarSlot` bool at the reflected path.
    let altar: u8 = unsafe { enshwick::raw::read_at(info, altar_at) };
    if flags & creature != 0 || altar != 0 {
        return Err(Status::NOT_A_PROP);
    }
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
    let logic = bit(catalogue::ITEM_FLAG_IS_LOGIC_ENTITY).is_ok_and(|logic| flags & logic != 0);
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
/// of range or an item that places nothing, [`Status::NOT_A_PROP`] for one
/// that places a creature or a flame altar. Any thread: it reads only the
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

/// The reflected types and offsets the state reads need, resolved
/// together: one missing turns removal and undo off on this build.
struct Facts {
    setup: u32,
    setup_initialized: usize,
    setup_links: usize,
    setup_categories: usize,
    inventory: u32,
    stack: usize,
    level: u32,
    level_data: usize,
    growth: u32,
    factory: u32,
    factory_state: usize,
    factory_running: usize,
    factory_next: usize,
    bed: u32,
    bed_npc: usize,
    store: u32,
    store_ids: usize,
    text: u32,
    text_id: usize,
    fuel: u32,
    fuel_charges: usize,
    fuel_setup: u32,
    fuel_most: usize,
    fuel_start: usize,
    connected: u32,
    connected_writer: usize,
    connected_readers: usize,
    trigger: u32,
    trigger_config: usize,
    trigger_shape: u32,
    animal: u32,
    npc: u32,
    altar: u32,
}

/// The hash of the catalogue type `id`, which reflection must know.
fn type_of(loader: &Loader, id: &str) -> Result<u32, Status> {
    loader
        .reflection
        .type_info(catalogue::type_name(id))
        .map_err(|_| Status::MISSING)?;
    catalogue::type_hash(id).ok_or(Status::MISSING)
}

impl Facts {
    /// The facts on this build; [`Status::MISSING`] when one does not
    /// resolve.
    fn resolve(loader: &Loader) -> Result<Self, Status> {
        let at = |id: &str| offset(loader, id);
        let ty = |id: &str| type_of(loader, id);
        Ok(Self {
            setup: ty(catalogue::INVENTORY_SETUP)?,
            setup_initialized: at(catalogue::INVENTORY_SETUP_INITIALIZED)?,
            setup_links: at(catalogue::INVENTORY_SETUP_LINKS)?,
            setup_categories: at(catalogue::INVENTORY_SETUP_CATEGORIES)?,
            inventory: ty(catalogue::INVENTORY)?,
            stack: type_size(loader, catalogue::ITEM_STACK)?,
            level: ty(catalogue::LEVEL)?,
            level_data: at(catalogue::LEVEL_DATA)?,
            growth: ty(catalogue::GROWTH)?,
            factory: ty(catalogue::FACTORY_STATION)?,
            factory_state: at(catalogue::FACTORY_STATE)?,
            factory_running: at(catalogue::FACTORY_RUNNING)?,
            factory_next: at(catalogue::FACTORY_NEXT)?,
            bed: ty(catalogue::NPC_BED)?,
            bed_npc: at(catalogue::NPC_BED_NPC)?,
            store: ty(catalogue::STORE_NPCS)?,
            store_ids: at(catalogue::STORE_NPCS_IDS)?,
            text: ty(catalogue::CUSTOM_PLAYER_STRING)?,
            text_id: at(catalogue::CUSTOM_PLAYER_STRING_ID)?,
            fuel: ty(catalogue::FUEL_CHARGES)?,
            fuel_charges: at(catalogue::FUEL_CHARGES_DATA)?,
            fuel_setup: ty(catalogue::FUEL_SETUP)?,
            fuel_most: at(catalogue::FUEL_SETUP_MAX)?,
            fuel_start: at(catalogue::FUEL_SETUP_INITIAL)?,
            connected: ty(catalogue::LOGIC_CONNECTED)?,
            connected_writer: at(catalogue::LOGIC_CONNECTED_WRITER)?,
            connected_readers: at(catalogue::LOGIC_CONNECTED_READERS)?,
            trigger: ty(catalogue::LOGIC_TRIGGER)?,
            trigger_config: at(catalogue::LOGIC_TRIGGER_CONFIG)?,
            trigger_shape: ty(catalogue::LOGIC_TRIGGER_SHAPE)?,
            animal: ty(catalogue::ANIMAL_SAVED)?,
            npc: ty(catalogue::ACTIVE_NPC_STATE_SAVED)?,
            altar: ty(catalogue::BASE_ID)?,
        })
    }
}

/// The data of `entity`'s component of type `hash`, when it has one; on
/// the game thread.
fn data(loader: &Loader, entity: u32, hash: u32) -> Option<*const u8> {
    crate::world::component(loader, Entity(entity), hash)
        .ok()
        .map(<*const core::ffi::c_void>::cast)
}

/// The value of type `T` at `at` in `entity`'s component of type `hash`,
/// when it has one; on the game thread.
fn field<T: enshwick::raw::AnyBitPattern>(
    loader: &Loader,
    entity: u32,
    hash: u32,
    at: usize,
) -> Option<T> {
    let data = data(loader, entity, hash)?;
    // SAFETY: `data` is the entity's live component of type `hash`, and the
    // caller passes the reflected offset of a `T` field inside it, from
    // `Facts`; read on the game thread, which alone writes it.
    Some(unsafe { enshwick::raw::read_at(data, at) })
}

/// The prop `entity` is and the state it holds, as `HOLDS_` bits;
/// [`Status::NOT_A_PROP`] for an entity not placed from an item, a
/// creature (an animal, a pet or a survivor) or a flame altar, which owns
/// a base, [`Status::MISSING`] while it
/// is not in the world or when this build lacks a fact the state reads
/// need. On the game thread.
pub(super) fn read(loader: &Loader, entity: u32) -> Result<(Prop, u32), Status> {
    let facts = Facts::resolve(loader)?;
    let template = crate::world::template(loader, Entity(entity)).map_err(|_| Status::MISSING)?;
    let not_a_prop = [facts.animal, facts.npc, facts.altar];
    if not_a_prop
        .into_iter()
        .any(|hash| data(loader, entity, hash).is_some())
    {
        return Err(Status::NOT_A_PROP);
    }
    let item = crate::world::used_item(loader, Entity(entity)).ok_or(Status::NOT_A_PROP)?;
    let (placed, local_box, logic) = match item_prop(loader, item) {
        Ok(found) => found,
        Err(Status::BAD_ARGS) => return Err(Status::NOT_A_PROP),
        Err(reason) => return Err(reason),
    };
    if placed != template {
        return Err(Status::NOT_A_PROP);
    }
    let position = crate::world::fixed_position(loader, Entity(entity)).ok_or(Status::MISSING)?;
    let orientation = orientation(loader, entity).ok_or(Status::MISSING)?;
    let prop = Prop {
        template,
        item,
        position,
        orientation,
        local_box,
        logic,
    };
    Ok((prop, state(loader, &facts, entity).holds()))
}

/// Which way `entity` faces, from its `CurrentTransform`.
fn orientation(loader: &Loader, entity: u32) -> Option<[f32; 4]> {
    let hash = type_of(loader, catalogue::CURRENT_TRANSFORM).ok()?;
    let at = offset(loader, catalogue::CURRENT_TRANSFORM_TRANSFORM).ok()?
        + offset(loader, catalogue::WORLD_TRANSFORM_ORIENTATION).ok()?;
    field(loader, entity, hash, at)
}

/// Whether a prop like `prop` stands where it stood: an entity of its
/// item with its template at its position.
pub(super) fn stands(loader: &Loader, prop: &Prop) -> Result<bool, Status> {
    let used_item = type_of(loader, catalogue::USED_ITEM)?;
    let placed = crate::entities::with_component(loader, used_item)?;
    Ok(placed.iter().any(|place| {
        place.position == prop.position
            && crate::world::used_item(loader, Entity(place.entity)) == Some(prop.item)
            && crate::world::template(loader, Entity(place.entity)).ok() == Some(prop.template)
    }))
}

/// Asks the game to remove the prop `entity` at the end of the next step,
/// as its tear-down does, without loot; players stay kept from it until
/// it is gone.
pub(super) fn remove(loader: &Loader, entity: u32) -> Result<(), Status> {
    crate::entities::destroy(loader, Entity(entity))?;
    crate::transactions::holds().destroying(entity);
    Ok(())
}

/// Keeps players from the prop `entity` until its destroy: the loader's
/// interaction filter refuses opening it, moving items in or out, sorting
/// and dismantling it, and its Quick Stash setting is off, so area deposits
/// skip it. [`Status::MISSING`] when a hook the filter needs did not link
/// or its fuse is off, or the setting does not resolve. On the game thread
/// at a quiet moment.
pub(super) fn hold(loader: &Loader, entity: u32) -> Result<(), Status> {
    let stock = type_of(loader, catalogue::CRAFTING_STOCK)?;
    let deposit_at = offset(loader, catalogue::CRAFTING_STOCK_DEPOSIT)?;
    let holds = crate::transactions::holds();
    holds.hold(entity, crate::interactions::filter_off())?;
    if let Some(data) = data(loader, entity, stock) {
        // SAFETY: `data` is the entity's live `InventoryCraftingStock`,
        // with its Quick Stash byte at the reflected offset; read on the
        // game thread at a quiet moment, when no system runs.
        let before: u8 = unsafe { enshwick::raw::read_at(data, deposit_at) };
        holds.note_deposit(entity, before);
        set_deposit(loader, entity, 0);
    }
    Ok(())
}

/// Lets players reach the prop `entity` again, its Quick Stash setting as
/// it was. On the game thread at a quiet moment.
pub(super) fn release(loader: &Loader, entity: u32) {
    if let Some(before) = crate::transactions::holds().release(entity) {
        set_deposit(loader, entity, before);
    }
}

/// Writes `value` into the Quick Stash setting of `entity`, when it has
/// one; nothing off the game thread or inside a step, where systems may
/// read it.
pub(crate) fn set_deposit(loader: &Loader, entity: u32, value: u8) {
    if crate::world::server().is_err() {
        return;
    }
    let (Ok(stock), Ok(deposit_at)) = (
        type_of(loader, catalogue::CRAFTING_STOCK),
        offset(loader, catalogue::CRAFTING_STOCK_DEPOSIT),
    ) else {
        return;
    };
    if let Some(data) = data(loader, entity, stock) {
        // SAFETY: `data` is the entity's live `InventoryCraftingStock`, with
        // its Quick Stash byte at the reflected offset; `world::server`
        // checked the game thread at a quiet moment, outside any step's
        // systems, so neither the game's own writer, the player's switch
        // action, nor a reader runs.
        unsafe { enshwick::raw::write_at(data.cast_mut(), deposit_at, value) };
    }
}

/// What `entity` holds that a respawn from its template would lose
/// (server 399, client 426), each read as `facts` places it: items in its
/// inventory, a plant (any, as its start level is not known), a production
/// station's
/// recipes, assigned survivors, written text, fuel above a fresh prop's,
/// logic wiring and a logic trigger.
fn state(loader: &Loader, facts: &Facts, entity: u32) -> StateReads {
    let factory = data(loader, entity, facts.factory).map(|_| FactoryRead {
        state: field::<u8>(loader, entity, facts.factory, facts.factory_state).unwrap_or(1),
        running: field::<u32>(loader, entity, facts.factory, facts.factory_running) != Some(0),
        next: field::<u32>(loader, entity, facts.factory, facts.factory_next) != Some(0),
    });
    let fuel =
        field::<i32>(loader, entity, facts.fuel, facts.fuel_charges).is_some_and(|charges| {
            let most = field::<i32>(loader, entity, facts.fuel_setup, facts.fuel_most);
            let start = field::<f32>(loader, entity, facts.fuel_setup, facts.fuel_start);
            fuel_above_start(charges, most.zip(start))
        });
    let wiring = data(loader, entity, facts.connected).is_some_and(|_| {
        let writer = field::<u32>(loader, entity, facts.connected, facts.connected_writer);
        let readers = field::<u8>(loader, entity, facts.connected, facts.connected_readers);
        wired(writer.unwrap_or(1), readers.unwrap_or(1))
    });
    let mut setup = |link: u32| {
        let found = data(loader, link, facts.setup)?;
        // SAFETY: `found` is the link's live `InventorySetup`, with its
        // `isInitialized` byte at the reflected offset.
        let initialized: u8 = unsafe { enshwick::raw::read_at(found, facts.setup_initialized) };
        let mut links = [(0, 0); LINKS];
        for (i, slot) in links.iter_mut().enumerate() {
            // SAFETY: `found` is the live `InventorySetup`, whose `LINKS`
            // `u32` linked entities start at the reflected offset, and
            // `i < LINKS`.
            let link: u32 = unsafe { enshwick::raw::read_at(found, facts.setup_links + 4 * i) };
            // SAFETY: `found` is the live `InventorySetup`, whose `LINKS`
            // category bytes start at the reflected offset, and `i < LINKS`.
            let category: u8 = unsafe { enshwick::raw::read_at(found, facts.setup_categories + i) };
            *slot = (link, category);
        }
        Some(Setup {
            initialized: initialized != 0,
            links,
        })
    };
    let mut slots = |link: u32| {
        let found = data(loader, link, facts.inventory)?;
        let mut items = [0u32; SLOTS];
        for (slot, item) in items.iter_mut().enumerate() {
            // SAFETY: `found` is the link's live `Inventory`, which holds
            // `SLOTS` stacks of the reflected `ItemStack` size, each
            // starting with its `u32` item, and `slot < SLOTS`.
            *item = unsafe { enshwick::raw::read_at(found, slot * facts.stack) };
        }
        Some(items)
    };
    StateReads {
        items: holds_items(entity, &mut setup, &mut slots),
        grown: data(loader, entity, facts.growth).is_some(),
        factory,
        bed_npc: field::<u32>(loader, entity, facts.bed, facts.bed_npc).is_some_and(|npc| npc != 0),
        store_npcs: field::<[u32; 2]>(loader, entity, facts.store, facts.store_ids)
            .is_some_and(|ids| ids != [0, 0]),
        text: field::<u64>(loader, entity, facts.text, facts.text_id).is_some_and(|id| id != 0),
        fuel,
        logic: wiring,
        trigger: data(loader, entity, facts.trigger).is_some()
            || data(loader, entity, facts.trigger_shape).is_some(),
    }
}

/// The start state of the prop `entity` as bytes to compare: the `u32`
/// settings its `LogicGraphTrigger` uses, which a player's change of them
/// replaces, and a plant's `Level`, which grows; nothing the game rewrites
/// on its own (a trigger's query, its new-settings flag, its shape copied
/// each step). `None` for a prop with neither, [`Status::MISSING`] while
/// it is not in the world or this build lacks the facts. On the game
/// thread.
pub(super) fn start_state(loader: &Loader, entity: u32) -> Result<Option<Vec<u8>>, Status> {
    let facts = Facts::resolve(loader)?;
    crate::world::template(loader, Entity(entity)).map_err(|_| Status::MISSING)?;
    let trigger = data(loader, entity, facts.trigger).is_some()
        || data(loader, entity, facts.trigger_shape).is_some();
    let plant = data(loader, entity, facts.growth).is_some();
    if !trigger && !plant {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(8);
    if trigger {
        let config = field::<u32>(loader, entity, facts.trigger, facts.trigger_config);
        bytes.extend_from_slice(&config.unwrap_or(0).to_le_bytes());
    }
    if plant {
        let level = field::<u32>(loader, entity, facts.level, facts.level_data);
        bytes.extend_from_slice(&level.unwrap_or(u32::MAX).to_le_bytes());
    }
    Ok(Some(bytes))
}
