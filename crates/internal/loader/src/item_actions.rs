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
//! item. A use whose player moves, sorts or equips a stack before then
//! raises none, since a stack dragged out of the slot looks taken. Pickups
//! and placements reach
//! the filters from the server's event streams, after the game made them.
//! Reflection of dedicated server build 26319703 names all of it.

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick::abi::{Header, HookFlags, ItemAction, ItemActionFn, Side, Status};
use enshwick::raw::{read_at, write_at};

use crate::chains::Next;
use crate::ecs::{Components, Engine};
use crate::host::{Loader, Registered, Target};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::registry::Registry;

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
static ITEMS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("item filters");

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
    /// The handled `u32` versions in `ServerConsumedPlayerInput` of the
    /// actions that move a stack and keep it: a transfer, a sort, an
    /// equipment change and an equipment set swap.
    moves: [usize; 4],
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
    let hash = |name: &str| crate::world::type_hash(reflection.type_info(name).ok()?.va());
    let input = field("keen::ecs::PlayerInput", "fromClient")?;
    let data = "keen::ecs::ClientPlayerInputData";
    let consumed = "keen::ecs::ServerConsumedPlayerInput";
    let item = input + field(data, "itemAction")?;
    let change = input + field(data, "changeEquipmentAction")?;
    let (action, equipment) = ("keen::ecs::ItemAction", "keen::ecs::ChangeEquipmentAction");
    let stack = crate::catalogue::size(loader, crate::catalogue::ids::ITEM_STACK)?;
    let inventory = crate::catalogue::size(loader, crate::catalogue::ids::INVENTORY)?;
    let stack_item = field(
        enshwick_gamedata::catalogue::type_name(crate::catalogue::ids::ITEM_STACK),
        "id",
    )?;
    let stack_count = crate::catalogue::offset(loader, crate::catalogue::ids::ITEM_STACK_COUNT)?;
    // Both u32 reads lie inside the reflected stack.
    if stack_item.max(stack_count) + 4 > stack {
        return None;
    }
    Some(Layout {
        stack_item,
        player_input: hash("keen::ecs::PlayerInput")?,
        consumed_input: hash(consumed)?,
        inventory: crate::catalogue::component_hash(loader, crate::catalogue::ids::INVENTORY)?,
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
        moves: [
            field(consumed, "consumedInventoryTransferAction")?,
            field(consumed, "consumedSortInventoryAction")?,
            field(consumed, "consumedChangeEquipmentAction")?,
            field(consumed, "consumedSwapEquipmentSetAction")?,
        ],
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
            hash: crate::world::type_hash(info.va())?,
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
        || crate::ecs::engine(loader).is_none()
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
        crate::profile::call(f.owner, crate::profile::Feature::Items, || unsafe {
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
        || unsafe { engine.is_world(ctx, crate::world::server_world()) }
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
    let (Some((loader, layout, components)), Some(engine)) = (LOADED.get(), crate::ecs::resolved())
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
    let players = crate::interactions::players();
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
            player: enshwick::Entity(player),
            item: enshwick::ItemId::new(item),
            count: if kind == ItemAction::CONSUME {
                1
            } else {
                count
            },
            inventory: enshwick::Entity(inventory),
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
                crate::recipes::item_words(loader, item)
            );
        } else if kind == ItemAction::CONSUME {
            watch(
                ItemAction {
                    kind: ItemAction::CONSUMED,
                    ..call
                },
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

unsafe fn stack(
    components: &Components,
    layout: &Layout,
    ctx: *mut u64,
    inventory: u32,
    index: u8,
) -> Option<(u32, u32)> {
    // SAFETY: the caller's contract is `holding`'s.
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

/// How long a use every filter allowed waits for the game to take its item
/// before the loader takes it as refused.
const USE_WAIT: Duration = Duration::from_secs(5);
/// Uses watched at most at once; a use past them raises no `CONSUMED`.
const MAX_WATCHED: usize = 64;

/// A use every filter allowed, watched until the game takes its item from
/// the slot: the game refuses some only after the filters, a potion at full
/// health a step later, and then keeps the item.
#[derive(Clone, Copy, Debug)]
struct Use {
    call: ItemAction,
    /// What the slot held when the use came.
    item: u32,
    count: u32,
    /// The player's handled versions of [`Layout::moves`] then.
    moves: [u32; 4],
    until: Instant,
}

/// The handled versions of [`Layout::moves`] in `consumed`.
///
/// # Safety
///
/// - `consumed` is a player's live `ServerConsumedPlayerInput` component,
///   read on the game thread.
unsafe fn moves(layout: &Layout, consumed: *const u8) -> [u32; 4] {
    // SAFETY: the caller's `consumed` is the live component; each u32
    // version sits at its reflected offset in `layout.moves`.
    layout.moves.map(|at| unsafe { read_at(consumed, at) })
}

/// What a watched use's slot shows now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    /// The game took the item: its stack went down or left the slot.
    Taken,
    /// Nothing yet.
    Waiting,
    /// The time ran out, the inventory is gone or the player moved a stack:
    /// the game refused the use, or the slot can no longer prove it took it.
    Refused,
}

impl Use {
    /// Reads the slot's `now`, `None` when it proves nothing, at `at`; a
    /// stack that grew, as by a pickup, is the new count to go down from.
    fn see(&mut self, now: Option<(u32, u32)>, at: Instant) -> Seen {
        let Some((item, count)) = now else {
            return Seen::Refused;
        };
        if item != self.item || count < self.count {
            return Seen::Taken;
        }
        self.count = count;
        if at >= self.until {
            Seen::Refused
        } else {
            Seen::Waiting
        }
    }

    fn same_slot(&self, other: &Self) -> bool {
        self.call.inventory == other.call.inventory && self.call.slot == other.call.slot
    }
}

/// The uses the loader watches, and whether it watches any, so a step
/// without one takes no lock.
struct Uses {
    watched: Mutex<Vec<Use>>,
    any: AtomicBool,
}

static USES: Uses = Uses {
    watched: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
};

/// Starts watching `call`, whose slot holds `count` of `item`, while the
/// player's handled moves are `moves`.
fn watch(call: ItemAction, item: u32, count: u32, moves: [u32; 4], at: Instant) {
    let mut watched = USES.watched.relock();
    if watched.len() >= MAX_WATCHED {
        crate::log!(
            WARN,
            "{MAX_WATCHED} item uses wait for the game already: this one raises no consumed"
        );
        return;
    }
    watched.push(Use {
        call,
        item,
        count,
        moves,
        until: at + USE_WAIT,
    });
    USES.any.store(true, Ordering::Release);
}

/// The watched uses whose item the game took, read through `slot_now` with
/// the player's handled moves, each item gone from a stack paying one use
/// of its slot, oldest first; refused ones are let go, and so are those
/// whose player moved a stack since: dragged out of its slot, a stack the
/// game kept looks like one it took.
fn taken(
    at: Instant,
    mut slot_now: impl FnMut(&ItemAction) -> Option<((u32, u32), [u32; 4])>,
) -> Vec<ItemAction> {
    if !USES.any.load(Ordering::Acquire) {
        return Vec::new();
    }
    let mut watched = USES.watched.relock();
    let mut done = Vec::new();
    // Per slot that went down this pass: what it holds now, and how many
    // more of its watched uses the items gone from it pay.
    let mut paying: Vec<(Use, (u32, u32), u32)> = Vec::new();
    let mut kept = Vec::with_capacity(watched.len());
    for mut used in core::mem::take(&mut *watched) {
        if let Some((_, (item, count), more)) =
            paying.iter_mut().find(|(paid, ..)| paid.same_slot(&used))
        {
            if *more > 0 {
                *more -= 1;
                done.push(used.call);
            } else {
                used.item = *item;
                used.count = *count;
                kept.push(used);
            }
            continue;
        }
        let now = slot_now(&used.call)
            .filter(|&(_, moves)| moves == used.moves)
            .map(|(stack, _)| stack);
        match (used.see(now, at), now) {
            (Seen::Waiting, _) => kept.push(used),
            (Seen::Taken, Some((item, count))) => {
                let gone = if item == used.item {
                    used.count - count
                } else {
                    used.count
                };
                paying.push((used, (item, count), gone.saturating_sub(1)));
                done.push(used.call);
            }
            _ => {}
        }
    }
    USES.any.store(!kept.is_empty(), Ordering::Release);
    *watched = kept;
    done
}

/// Forgets the watched uses, when a world ends.
pub(crate) fn forget() {
    USES.watched.relock().clear();
    USES.any.store(false, Ordering::Release);
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
            player: enshwick::Entity(player),
            item: enshwick::ItemId::new(item),
            count,
            inventory: enshwick::Entity(0),
            slot: 0,
        };
        let _heard = ask(&call, false);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: Layout = Layout {
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
        moves: [32, 36, 120, 124],
        consume: 1,
        trash: 2,
        equip: 0,
        unequip: 1,
    };

    /// Serialises the tests of the watched uses, which share them.
    static SHARED: Mutex<()> = Mutex::new(());

    fn drink(inventory: u32, slot: u32) -> ItemAction {
        ItemAction {
            header: Header::of::<ItemAction>(),
            kind: ItemAction::CONSUMED,
            player: enshwick::Entity(1),
            item: enshwick::ItemId::new(7),
            count: 1,
            inventory: enshwick::Entity(inventory),
            slot,
        }
    }

    #[test]
    fn a_use_is_consumed_only_once_the_game_takes_its_item() {
        let at = Instant::now();
        let mut used = Use {
            call: drink(100, 0),
            item: 7,
            count: 5,
            moves: STILL,
            until: at + USE_WAIT,
        };
        assert_eq!(used.see(Some((7, 5)), at), Seen::Waiting, "kept so far");
        assert_eq!(used.see(Some((7, 6)), at), Seen::Waiting, "a pickup");
        assert_eq!(used.see(Some((7, 5)), at), Seen::Taken, "down from six");
        assert_eq!(used.see(Some((0, 0)), at), Seen::Taken, "the last one");
        assert_eq!(
            used.see(Some((9, 1)), at),
            Seen::Taken,
            "another item there"
        );
        assert_eq!(used.see(None, at), Seen::Refused, "the inventory went");
        assert_eq!(
            used.see(Some((7, 6)), at + USE_WAIT),
            Seen::Refused,
            "the game kept it"
        );
    }

    /// The player's handled moves while none ran.
    const STILL: [u32; 4] = [3, 0, 1, 0];

    #[test]
    fn a_refused_use_is_let_go_and_one_stack_down_pays_one_use() {
        let _alone = SHARED.relock();
        forget();
        let at = Instant::now();
        // A potion at full health: the game keeps all five.
        watch(drink(100, 0), 7, 5, STILL, at);
        assert!(taken(at, |_| Some(((7, 5), STILL))).is_empty());
        assert!(taken(at + USE_WAIT, |_| Some(((7, 5), STILL))).is_empty());
        assert!(!USES.any.load(Ordering::Acquire), "let go");
        // Two uses of one stack, then one more of another slot: the stack
        // goes down once.
        watch(drink(100, 0), 7, 5, STILL, at);
        watch(drink(100, 0), 7, 5, STILL, at);
        watch(drink(100, 1), 7, 2, STILL, at);
        let slots = |call: &ItemAction| Some((if call.slot == 0 { (7, 4) } else { (7, 2) }, STILL));
        assert_eq!(taken(at, slots).len(), 1, "one use paid");
        assert!(
            taken(at, slots).is_empty(),
            "the other waits for the next one"
        );
        let slots = |call: &ItemAction| Some((if call.slot == 0 { (7, 3) } else { (7, 1) }, STILL));
        assert_eq!(taken(at, slots).len(), 2, "both stacks went down");
        assert!(!USES.any.load(Ordering::Acquire));
        // Two uses of one stack that the game ran in one step: down by two.
        watch(drink(100, 0), 7, 5, STILL, at);
        watch(drink(100, 0), 7, 5, STILL, at);
        watch(drink(100, 0), 7, 5, STILL, at);
        assert_eq!(
            taken(at, |_| Some(((7, 3), STILL))).len(),
            2,
            "two items gone, two uses"
        );
        assert_eq!(
            taken(at, |_| Some(((0, 0), STILL))).len(),
            1,
            "the stack went"
        );
        assert!(!USES.any.load(Ordering::Acquire));
    }

    #[test]
    fn a_refused_use_whose_stack_the_player_drags_away_pays_nothing() {
        let _alone = SHARED.relock();
        forget();
        let at = Instant::now();
        // A potion at full health, then the stack dragged out of its slot,
        // split in half, sorted away or swapped with another item: each move
        // is a handled version that went up.
        for (i, stack) in [(0, 0), (7, 2), (0, 0), (9, 1)].into_iter().enumerate() {
            watch(drink(100, 0), 7, 5, STILL, at);
            assert!(taken(at, |_| Some(((7, 5), STILL))).is_empty(), "kept");
            let mut moved = STILL;
            moved[i] += 1;
            assert!(
                taken(at, |_| Some((stack, moved))).is_empty(),
                "move {i} pays no use"
            );
            assert!(!USES.any.load(Ordering::Acquire), "move {i}: let go");
        }
        // A move that ran before the game took the item still pays nothing:
        // the slot alone cannot tell the two apart.
        watch(drink(100, 0), 7, 5, STILL, at);
        let mut moved = STILL;
        moved[0] += 1;
        assert!(taken(at, |_| Some(((7, 4), moved))).is_empty());
        // Moves before the use are the ones it starts from.
        watch(drink(100, 0), 7, 5, [9, 9, 9, 9], at);
        assert_eq!(taken(at, |_| Some(((7, 4), [9, 9, 9, 9]))).len(), 1);
    }

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
