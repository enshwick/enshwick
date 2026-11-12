//! Players' actions with items, asked of the mods' item filters in the
//! server's world.
//!
//! A use or a trash is the player input's `itemAction`, which the
//! server's `inventory_actions` system runs, and an equip or an unequip its
//! `changeEquipmentAction`, which `equipment_actions` runs. Each carries a
//! version, and a system runs one whose version differs from the one the
//! player's `ServerConsumedPlayerInput` last handled. At each system's
//! entry the loader asks the filters about every pending action and refuses
//! one by writing its version as handled, the game's own path for an action
//! it already ran: the item stays where it was. A use every filter allowed
//! reaches them all again as `CONSUMED` once the game took the item from
//! its slot, at a later entry of `inventory_actions`: the game refuses some
//! uses after the filters, such as a potion at full health, and keeps the
//! item (a healing potion at full health, client 26430625 and server
//! 26430253). A use whose player does anything else before then raises
//! none, since a stack dragged or stashed out of the slot looks taken. Pickups
//! and placements reach
//! the filters from the server's event streams, after the game made them.
//! Reflection of dedicated server build 26319703 names all of it.

mod uses;

use core::ffi::c_void;
use std::sync::OnceLock;
use std::time::Instant;

use enshwick_core::abi::{Header, HookFlags, ItemAction, ItemActionFn, Side, Status};
use enshwick_core::raw::{read_at, write_at};

use crate::engine::chains::Next;
use crate::engine::ecs::{Components, Engine};
use crate::engine::mod_index::ModIndex;
use crate::engine::registry::Registry;
use crate::host::{Loader, Registered, Target};
pub(crate) use uses::forget;
use uses::{moves, taken, watch};

/// The system that runs the players' equips and unequips.
const EQUIPMENT_SYSTEM: &str = "equipment_actions";

static FILTERS: Registry<Registered<ItemActionFn>> = Registry::new();
static LOADED: OnceLock<(&'static Loader, Layout, Components)> = OnceLock::new();
static EVENTS: OnceLock<Events> = OnceLock::new();

type ActionsFn = unsafe extern "system" fn(*mut u64) -> u64;

// SAFETY: `ActionsFn` is the signature of a system entry, here the reflected
// `equipment_actions` system's, and `link` uses this slot only as that
// function's next.
static EQUIPMENT_NEXT: Next<ActionsFn> = unsafe { Next::new() };

/// The switch of the item filters.
static ITEMS_FUSE: crate::engine::fuse::Fuse = crate::engine::fuse::Fuse::new("item filters");

/// Where one versioned action sits in the player's input, and where its
/// handled version sits in the consumed input, in bytes.
#[derive(Debug, Clone, Copy)]
struct Versioned {
    /// Its `InventorySlotId`.
    slot: usize,
    /// Its `u32` version.
    version: usize,
    /// Its `u8` type.
    kind: usize,
    /// The handled `u32` version in `ServerConsumedPlayerInput`.
    consumed: usize,
}

/// The reflected layouts the item filters read.
#[derive(Debug, Clone, Copy)]
struct Layout {
    player_input: u32,
    consumed_input: u32,
    inventory: u32,
    stack: usize,
    /// Where a stack holds its `u32` item id.
    stack_item: usize,
    stack_count: usize,
    slots: usize,
    slot_entity: usize,
    slot_index: usize,
    item: Versioned,
    equipment: Versioned,
    /// Where the handled `u32` versions of `ServerConsumedPlayerInput`
    /// start and the component ends, so a watched use sees whether its
    /// player did anything else; `None` turns `CONSUMED` off alone.
    moves: Option<(usize, usize)>,
    /// `ItemActionType` `Consume` and `Trash`.
    consume: u8,
    trash: u8,
    /// `ChangeEquipment` `Equip` and `Unequip`.
    equip: u8,
    unequip: u8,
}

impl Layout {
    /// The filter kind of an action of type `kind`, `None` for one the
    /// filters are not asked about.
    fn kind(&self, equipment: bool, kind: u8) -> Option<u32> {
        match (equipment, kind) {
            (false, k) if k == self.consume => Some(ItemAction::CONSUME),
            (false, k) if k == self.trash => Some(ItemAction::TRASH),
            (true, k) if k == self.equip => Some(ItemAction::EQUIP),
            (true, k) if k == self.unequip => Some(ItemAction::UNEQUIP),
            _ => None,
        }
    }
}

/// A game event the filters hear after the fact: its hash, its size and
/// where its fields sit.
#[derive(Debug, Clone, Copy)]
struct Heard {
    hash: u32,
    size: usize,
    item: usize,
    count: Option<usize>,
    player: usize,
    kind: u32,
}

/// The events the filters hear, on builds that reflect them.
#[derive(Debug, Clone, Copy, Default)]
struct Events {
    pickup: Option<Heard>,
    place: Option<Heard>,
}

fn layout(loader: &Loader) -> Option<Layout> {
    let reflection = &loader.reflection;
    let field = |ty: &str, name: &str| {
        reflection
            .field_offset(ty, name)
            .ok()
            .and_then(|v| usize::try_from(v).ok())
    };
    let value = |ty: &str, name: &str| {
        reflection
            .enum_value(ty, name)
            .ok()
            .and_then(|v| u8::try_from(v).ok())
    };
    let hash = |name: &str| crate::engine::world::type_hash(reflection.type_info(name).ok()?.va());
    let input = field("keen::ecs::PlayerInput", "fromClient")?;
    let data = "keen::ecs::ClientPlayerInputData";
    let consumed = "keen::ecs::ServerConsumedPlayerInput";
    let item = input + field(data, "itemAction")?;
    let change = input + field(data, "changeEquipmentAction")?;
    let (action, equipment) = ("keen::ecs::ItemAction", "keen::ecs::ChangeEquipmentAction");
    let stack = crate::engine::catalogue::size(loader, crate::engine::catalogue::ids::ITEM_STACK)?;
    let inventory =
        crate::engine::catalogue::size(loader, crate::engine::catalogue::ids::INVENTORY)?;
    let stack_item = field(
        enshwick_gamedata::catalogue::type_name(crate::engine::catalogue::ids::ITEM_STACK),
        "id",
    )?;
    let stack_count =
        crate::engine::catalogue::offset(loader, crate::engine::catalogue::ids::ITEM_STACK_COUNT)?;
    // Both u32 reads lie inside the reflected stack.
    if stack_item.max(stack_count) + 4 > stack {
        return None;
    }
    Some(Layout {
        stack_item,
        player_input: hash("keen::ecs::PlayerInput")?,
        consumed_input: hash(consumed)?,
        inventory: crate::engine::catalogue::component_hash(
            loader,
            crate::engine::catalogue::ids::INVENTORY,
        )?,
        stack,
        stack_count,
        slots: inventory.checked_div(stack)?,
        slot_entity: field("keen::InventorySlotId", "entityId")?,
        slot_index: field("keen::InventorySlotId", "slotIndex")?,
        item: Versioned {
            slot: item + field(action, "inventorySlotId")?,
            version: item + field(action, "versionData")?,
            kind: item + field(action, "type")?,
            consumed: field(consumed, "consumedItemAction")?,
        },
        equipment: Versioned {
            slot: change + field(equipment, "inventorySlotId")?,
            version: change + field(equipment, "versionData")?,
            kind: change + field(equipment, "type")?,
            consumed: field(consumed, "consumedChangeEquipmentAction")?,
        },
        moves: field(consumed, "consumedCursorSelectObjectAction").zip(
            reflection
                .type_info(consumed)
                .ok()
                .and_then(|info| usize::try_from(info.size()).ok()),
        ),
        consume: value("keen::ecs::ItemActionType", "Consume")?,
        trash: value("keen::ecs::ItemActionType", "Trash")?,
        equip: value("keen::ecs::ChangeEquipment", "Equip")?,
        unequip: value("keen::ecs::ChangeEquipment", "Unequip")?,
    })
}

fn events(loader: &Loader) -> Events {
    let reflection = &loader.reflection;
    let heard = |name: &str, item: &str, count: Option<&str>, player: &str, kind: u32| {
        let info = reflection.type_info(name).ok()?;
        let field = |field: &str| {
            reflection
                .field_offset(name, field)
                .ok()
                .and_then(|v| usize::try_from(v).ok())
        };
        Some(Heard {
            hash: crate::engine::world::type_hash(info.va())?,
            size: usize::try_from(info.size()).ok()?,
            item: field(item)?,
            count: match count {
                Some(count) => Some(field(count)?),
                None => None,
            },
            player: field(player)?,
            kind,
        })
    };
    Events {
        pickup: heard(
            "keen::ecs::PickupEvent",
            "itemId",
            Some("itemCount"),
            "pickedUpByEntityId",
            ItemAction::PICKUP,
        ),
        place: heard(
            "keen::ecs::BuildingPlaceEvent",
            "trackingItemId",
            None,
            "ownerId",
            ItemAction::PLACE,
        ),
    }
}

/// Registers mod `owner`'s item filter.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    callback: ItemActionFn,
    user: *mut c_void,
) -> Status {
    if FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none()
        || Components::resolve(loader).is_none()
        || crate::engine::ecs::engine(loader).is_none()
    {
        return Status::MISSING;
    }
    FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Whether a mod filters item actions, so the players are read and the
/// event streams screened for it.
pub(crate) fn wanted() -> bool {
    FILTERS.sealed().is_some_and(|filters| !filters.is_empty())
}

/// Seals the filters and links the two systems' entries when a mod
/// registered one.
pub(crate) fn link(loader: &'static Loader) {
    let filters = FILTERS.seal();
    if filters.is_empty() {
        return;
    }
    let (Some(layout), Some(components)) = (layout(loader), Components::resolve(loader)) else {
        return;
    };
    if layout.moves.is_none() {
        crate::log!(
            WARN,
            "item filters hear no consumed: this build does not reflect the players' handled actions"
        );
    }
    let _first = LOADED.set((loader, layout, components));
    let _events = EVENTS.set(events(loader));
    let entry = |name: &str| {
        loader
            .reflection
            .system(name)
            .ok()
            .map(enshwick_gamedata::reflect::System::entry)
    };
    // The uses go through interactions' link on the same system: the
    // loader links a target once.
    let equips = loader.link_feature(
        "item equips",
        Target::Found(EQUIPMENT_SYSTEM, entry(EQUIPMENT_SYSTEM)),
        &EQUIPMENT_NEXT,
        equipment_actions as ActionsFn,
        HookFlags::NONE,
    );
    crate::log!(
        INFO,
        "item filters on: {} (equips {equips}; uses with the inventory actions)",
        filters.len()
    );
}

/// Asks every live filter about `call`; a refusable action stops at the
/// first refusal, one after the fact reaches every filter.
fn ask(call: &ItemAction, refusable: bool) -> bool {
    let Some(filters) = FILTERS.sealed() else {
        return true;
    };
    let mut live = filters.iter().filter(|f| f.live());
    let one = |f: &Registered<ItemActionFn>| {
        // SAFETY: `f` is live, so the mod's DLL that holds the callback stays
        // loaded; the callback gets the user pointer its mod registered and
        // `call`, which lives for the call.
        crate::engine::profile::call(f.owner, crate::engine::profile::Feature::Items, || unsafe {
            (f.callback)(call, f.user()) != 0
        })
    };
    if refusable {
        live.all(one)
    } else {
        live.for_each(|f| {
            let _ignored = one(f);
        });
        true
    }
}

/// Whether `ctx` belongs to the server's world.
///
/// # Safety
///
/// - `ctx` is the live context of a running system, used on its thread.
unsafe fn server_world(loader: &Loader, engine: &Engine, ctx: *mut u64) -> bool {
    loader.side == Side::SERVER
        // SAFETY: the caller guarantees a live system context on its thread,
        // all `is_world` asks for.
        || unsafe { engine.is_world(ctx, crate::engine::world::server_world()) }
}

/// Asks the filters about the players' pending uses and trashes, from
/// interactions' link on the `inventory_actions` system.
///
/// # Safety
///
/// - `ctx` is null or the live context of the `inventory_actions` system,
///   at its entry on its thread.
pub(crate) unsafe fn screen_uses(ctx: *mut u64) {
    if !ctx.is_null() && LOADED.get().is_some() {
        // SAFETY: `ctx` is non-null by the check before it and, by the
        // caller's contract, the system's own context at its entry on its
        // thread.
        let _done = ITEMS_FUSE.run("the item uses", || unsafe { screen_pending(ctx, false) });
    }
}

unsafe extern "system" fn equipment_actions(ctx: *mut u64) -> u64 {
    if !ctx.is_null() {
        // SAFETY: `ctx` is non-null by the check before it and is the system's
        // own context, at its entry on its thread.
        let _done = ITEMS_FUSE.run("the item equips", || unsafe { screen_pending(ctx, true) });
    }
    // SAFETY: this detour runs only as the `equipment_actions` system's
    // link, so the slot holds the next function of that signature; the game's
    // context passes unchanged.
    unsafe { EQUIPMENT_NEXT.get()(ctx) }
}

/// Asks the filters about each player's pending item action, or equipment
/// change when `equipment`, and marks a refused one as handled.
///
/// # Safety
///
/// - `ctx` is the live, non-null context of the `inventory_actions` system
///   (`equipment` false) or the `equipment_actions` system (`equipment`
///   true), at its entry on its thread.
unsafe fn screen_pending(ctx: *mut u64, equipment: bool) {
    let (Some((loader, layout, components)), Some(engine)) =
        (LOADED.get(), crate::engine::ecs::resolved())
    else {
        return;
    };
    // SAFETY: the caller guarantees the system's live context on its thread.
    if !unsafe { server_world(loader, engine, ctx) } {
        return;
    }
    let at = if equipment {
        layout.equipment
    } else {
        layout.item
    };
    let now = Instant::now();
    if !equipment {
        let slot_now = |call: &ItemAction| {
            // SAFETY: the caller guarantees the system's live context, at its
            // entry on its thread.
            let consumed =
                unsafe { components.component(ctx, call.player.0, layout.consumed_input) }?;
            let slot = u8::try_from(call.slot).ok()?;
            // SAFETY: the caller guarantees the system's live context, at its
            // entry on its thread.
            let stack = unsafe { holding(components, layout, ctx, call.inventory.0, slot) }?;
            // SAFETY: `consumed` is the player's live
            // `ServerConsumedPlayerInput` component.
            Some((stack, unsafe { moves(layout, consumed) }))
        };
        for consumed in taken(now, slot_now) {
            let _heard = ask(&consumed, false);
        }
    }
    let players = crate::actions::interactions::players();
    for &player in players.as_deref().map_or(&[][..], Vec::as_slice) {
        // SAFETY: the caller guarantees the system's live context, at its
        // entry on its thread.
        let input = unsafe { components.component(ctx, player, layout.player_input) };
        // SAFETY: the caller guarantees the system's live context, at its
        // entry on its thread.
        let consumed = unsafe { components.component_mut(ctx, player, layout.consumed_input) };
        let (Some(input), Some(consumed)) = (input, consumed) else {
            continue;
        };
        // SAFETY: `input` is the player's live `PlayerInput` component; the
        // action's u32 version sits at the reflected offset `at.version`.
        let version: u32 = unsafe { read_at(input, at.version) };
        // SAFETY: `consumed` is the player's live `ServerConsumedPlayerInput`
        // component; the action's handled u32 version sits at the reflected
        // offset `at.consumed`.
        let done: u32 = unsafe { read_at(consumed, at.consumed) };
        if version == done {
            continue;
        }
        // SAFETY: `input` is the player's live `PlayerInput` component; the
        // action's u8 type sits at the reflected offset `at.kind`.
        let Some(kind) = layout.kind(equipment, unsafe { read_at::<u8>(input, at.kind) }) else {
            continue;
        };
        // SAFETY: `input` is the player's live `PlayerInput` component; the
        // action's `InventorySlotId` starts at `at.slot`, its u32 entity at
        // the reflected `slot_entity` within it.
        let inventory: u32 = unsafe { read_at(input, at.slot + layout.slot_entity) };
        // SAFETY: `input` is the player's live `PlayerInput` component; the
        // slot id's u8 index sits at the reflected `slot_index` within it.
        let index: u8 = unsafe { read_at(input, at.slot + layout.slot_index) };
        // SAFETY: the caller guarantees the system's live context, at its
        // entry on its thread.
        let Some((item, count)) = (unsafe { stack(components, layout, ctx, inventory, index) })
        else {
            continue;
        };
        let call = ItemAction {
            header: Header::of::<ItemAction>(),
            kind,
            player: enshwick_core::Entity(player),
            item: enshwick_core::ItemId::new(item),
            count: if kind == ItemAction::CONSUME {
                1
            } else {
                count
            },
            inventory: enshwick_core::Entity(inventory),
            slot: u32::from(index),
        };
        if !ask(&call, true) {
            // SAFETY: `consumed` is the player's live, server-only consumed
            // record, written at the system's entry on its thread before the
            // system reads it; no client sees it change.
            unsafe { write_at(consumed, at.consumed, version) };
            crate::log!(
                DEBUG,
                "item action {kind} of {} by player {player} refused",
                crate::content::recipes::item_words(loader, item)
            );
        } else if kind == ItemAction::CONSUME {
            watch(
                ItemAction {
                    kind: ItemAction::CONSUMED,
                    ..call
                },
                version,
                item,
                count,
                // SAFETY: `consumed` is the player's live
                // `ServerConsumedPlayerInput` component.
                unsafe { moves(layout, consumed) },
                now,
            );
        }
    }
}

/// The item and count in slot `index` of the inventory `inventory`; `None`
/// for an empty slot, an index past the inventory or no inventory.
///
/// # Safety
///
/// - `ctx` is the live context of a running system, used on its thread.
unsafe fn stack(
    components: &Components,
    layout: &Layout,
    ctx: *mut u64,
    inventory: u32,
    index: u8,
) -> Option<(u32, u32)> {
    // SAFETY: `ctx` is the live context of a running system on its thread,
    // by this function's `# Safety`, all `holding` asks for.
    unsafe { holding(components, layout, ctx, inventory, index) }
        .filter(|&(item, count)| item != 0 && count != 0)
}

/// The item and count in slot `index` of the inventory `inventory`, both 0
/// for an empty slot; `None` for an index past the inventory or no
/// inventory.
///
/// # Safety
///
/// - `ctx` is the live context of a running system, used on its thread.
unsafe fn holding(
    components: &Components,
    layout: &Layout,
    ctx: *mut u64,
    inventory: u32,
    index: u8,
) -> Option<(u32, u32)> {
    let index = usize::from(index);
    if index >= layout.slots {
        return None;
    }
    // SAFETY: the caller guarantees the system's live context on its thread.
    let slots = unsafe { components.component(ctx, inventory, layout.inventory) }?;
    let at = index * layout.stack;
    // SAFETY: `slots` is the live `Inventory` component, `layout.slots`
    // stacks of the reflected `ItemStack` size, and `index` is below it; the
    // stack's u32 item id sits at the reflected `stack_item`, inside it
    // (checked by `layout`).
    let item: u32 = unsafe { read_at(slots, at + layout.stack_item) };
    // SAFETY: `slots` is the live `Inventory` component and `at` the start of
    // stack `index`, below `layout.slots`; the stack's u32 count sits at the
    // catalogue's `item_stack.count` offset within it.
    let count: u32 = unsafe { read_at(slots, at + layout.stack_count) };
    Some((item, count))
}

/// Whether the filters hear events of `type_hash`, so the event link copies
/// them for [`screen`].
pub(crate) fn hears(type_hash: u32) -> bool {
    EVENTS.get().is_some_and(|events| {
        [events.pickup, events.place]
            .into_iter()
            .flatten()
            .any(|h| h.hash == type_hash)
    })
}

/// Tells the filters of a pickup or a placement in one event the server's
/// streams held, on the game thread after the merge.
///
/// # Safety
///
/// - `payload` is a copy of an event of `type_hash`, readable for `size`
///   bytes, which nothing writes during the call.
pub(crate) unsafe fn screen(type_hash: u32, payload: *const u8, size: usize) {
    let Some(events) = EVENTS.get() else {
        return;
    };
    let Some(heard) = [events.pickup, events.place]
        .into_iter()
        .flatten()
        .find(|h| h.hash == type_hash && size >= h.size)
    else {
        return;
    };
    let _done = ITEMS_FUSE.run("the item events", || {
        // SAFETY: the event is of `heard`'s type and at least its reflected
        // size (`find` checks both), readable for `size` bytes (the caller's
        // contract); its u32 item id sits at the reflected `heard.item`.
        let item: u32 = unsafe { read_at(payload, heard.item) };
        // SAFETY: the event is of `heard`'s type and at least its reflected
        // size, readable for `size` bytes; its u32 player entity sits at the
        // reflected `heard.player`.
        let player: u32 = unsafe { read_at(payload, heard.player) };
        let count = heard.count.map_or(1, |at| {
            // SAFETY: the event is of `heard`'s type and at least its
            // reflected size, readable for `size` bytes; its u32 count sits
            // at the reflected `at`.
            unsafe { read_at::<u32>(payload, at) }
        });
        if item == 0 || player == 0 {
            return;
        }
        let call = ItemAction {
            header: Header::of::<ItemAction>(),
            kind: heard.kind,
            player: enshwick_core::Entity(player),
            item: enshwick_core::ItemId::new(item),
            count,
            inventory: enshwick_core::Entity(0),
            slot: 0,
        };
        let _heard = ask(&call, false);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) const LAYOUT: Layout = Layout {
        player_input: 1,
        consumed_input: 2,
        inventory: 3,
        stack: 12,
        stack_item: 0,
        stack_count: 4,
        slots: 8,
        slot_entity: 0,
        slot_index: 4,
        item: Versioned {
            slot: 0,
            version: 16,
            kind: 20,
            consumed: 128,
        },
        equipment: Versioned {
            slot: 0,
            version: 8,
            kind: 13,
            consumed: 120,
        },
        moves: Some((24, 224)),
        consume: 1,
        trash: 2,
        equip: 0,
        unequip: 1,
    };

    #[test]
    fn only_uses_trashing_and_equipment_changes_reach_the_filters() {
        assert_eq!(LAYOUT.kind(false, 1), Some(ItemAction::CONSUME));
        assert_eq!(LAYOUT.kind(false, 2), Some(ItemAction::TRASH));
        assert_eq!(LAYOUT.kind(false, 0), None, "an inspect is not asked");
        assert_eq!(LAYOUT.kind(true, 0), Some(ItemAction::EQUIP));
        assert_eq!(LAYOUT.kind(true, 1), Some(ItemAction::UNEQUIP));
        assert_eq!(LAYOUT.kind(true, 2), None);
    }
}
