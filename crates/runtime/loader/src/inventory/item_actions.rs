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

use enshwick_core::abi::{Header, HookFlags, ItemAction, ItemFilterFn, Side};
use enshwick_core::raw::{read_at, write_at};

use crate::engine::ecs::{Components, Engine};
use crate::engine::filters::Filters;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::parts::EventFollower;
use crate::engine::profile::Feature;
use crate::engine::registry::JoinFeatureError;
use crate::inventory::stacks::Stacks;
use enshwick_base::Relock as _;
use uses::forget as forget_uses;
use uses::{USES, moves};

/// The system that runs the players' equips and unequips.
const EQUIPMENT_SYSTEM: &str = "equipment_actions";

static FILTERS: Filters<ItemFilterFn> = Filters::new(Feature::Items);
/// The layout and components the links read, once linked.
static LINKED: OnceLock<(Layout, Components)> = OnceLock::new();

/// The loader with what the links read, once linked.
fn linked() -> Option<(&'static Loader, &'static Layout, &'static Components)> {
    let (loader, (layout, components)) = crate::engine::loader::linked(&LINKED)?;
    Some((loader, layout, components))
}
static EVENTS: OnceLock<Events> = OnceLock::new();

/// What the item filters follow while the game runs.
struct State {
    /// The pickups the loader raised itself, as `(player, item, count)`,
    /// which the event screen leaves out once each; listed only while the
    /// filters hear pickups.
    own_pickups: std::sync::Mutex<Vec<(u32, u32, u32)>>,
    /// How many `own_pickups` wait, so the screen takes no lock while none
    /// does.
    own_waiting: std::sync::atomic::AtomicUsize,
    /// The pickup event's layout, resolved once for the screen and the
    /// pickups the loader raises alike.
    pickup: OnceLock<Option<Heard>>,
    /// The players' character entities, which the interactions read on the
    /// game thread now and then.
    players: arc_swap::ArcSwapOption<Vec<u32>>,
}

impl State {
    const fn new() -> Self {
        Self {
            own_pickups: std::sync::Mutex::new(Vec::new()),
            own_waiting: std::sync::atomic::AtomicUsize::new(0),
            pickup: OnceLock::new(),
            players: arc_swap::ArcSwapOption::const_empty(),
        }
    }
}

static STATE: State = State::new();

/// The players' character entities as the interactions last read them;
/// `None` before a read and after a hosted game ended.
pub(crate) fn players() -> arc_swap::Guard<Option<std::sync::Arc<Vec<u32>>>> {
    STATE.players.load()
}

/// Keeps `players`, the players' character entities the interactions read,
/// or forgets them with `None`.
pub(crate) fn keep_players(players: Option<std::sync::Arc<Vec<u32>>>) {
    STATE.players.store(players);
}

/// The switch of the item filters.
static ITEMS_FUSE: Fuse = Fuse::new("item filters");

/// The switch of the equips the filters see, apart from the uses and events
/// so a build without the equipment system leaves those on.
static EQUIPS_FUSE: Fuse = Fuse::new("item equips");

/// The switch of the pickups and placements the filters hear from the
/// server's event streams, apart from the uses so a build without the
/// streams leaves those on.
static EVENTS_FUSE: Fuse = Fuse::new("item event filters");

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
    stacks: Stacks,
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
pub(crate) struct Heard {
    pub(crate) hash: u32,
    pub(crate) size: usize,
    pub(crate) item: usize,
    pub(crate) count: Option<usize>,
    pub(crate) player: usize,
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
    Some(Layout {
        player_input: hash("keen::ecs::PlayerInput")?,
        consumed_input: hash(consumed)?,
        stacks: Stacks::resolve(loader)?,
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

/// Where the event `name` keeps the fields the filters read, on this build.
fn heard(
    loader: &Loader,
    name: &str,
    item: &str,
    count: Option<&str>,
    player: &str,
    kind: u32,
) -> Option<Heard> {
    let reflection = &loader.reflection;
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
}

/// The pickup event's layout on this build, which the screen reads and the
/// loader's own pickups write.
pub(crate) fn pickup_event(loader: &Loader) -> Option<Heard> {
    *STATE.pickup.get_or_init(|| {
        heard(
            loader,
            "keen::ecs::PickupEvent",
            "itemId",
            Some("itemCount"),
            "pickedUpByEntityId",
            ItemAction::PICKUP,
        )
    })
}

fn events(loader: &Loader) -> Events {
    Events {
        pickup: pickup_event(loader),
        place: heard(
            loader,
            "keen::ecs::BuildingPlaceEvent",
            "trackingItemId",
            None,
            "ownerId",
            ItemAction::PLACE,
        ),
    }
}

/// Registers mod `owner`'s item filter.
///
/// # Errors
///
/// [`JoinFeatureError`] after init or on a build without item actions.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    callback: ItemFilterFn,
    user: *mut c_void,
) -> Result<(), JoinFeatureError> {
    let available = || {
        layout(loader).is_some()
            && Components::resolve(loader).is_some()
            && crate::engine::ecs::engine(loader).is_some()
    };
    FILTERS.register_when(available, owner, callback, user)
}

/// Whether a mod filters item actions, so the players are read and the
/// event streams screened for it.
pub(crate) fn wanted() -> bool {
    FILTERS.has_sealed()
}

/// Seals the filters before any link: game events link the event move when
/// a mod registered one.
pub(crate) fn seal() {
    let _sealed = FILTERS.seal();
}

/// Seals the filters and links the two systems' entries when a mod
/// registered one.
pub(crate) fn link(loader: &'static Loader) {
    let filters = FILTERS.sealed().unwrap_or_default();
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
    let _first = LINKED.set((layout, components));
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
        &EQUIPS_FUSE,
        Target::Found(EQUIPMENT_SYSTEM, entry(EQUIPMENT_SYSTEM)),
        &EQUIPMENT_NEXT,
        equipment_actions,
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
    // SAFETY: a filter's mod runs, so its DLL that holds the callback stays
    // loaded; it gets the user pointer its mod registered and `call`, which
    // lives for the call.
    let allows = |filter: &ItemFilterFn, user| unsafe {
        filter(call, user) != enshwick_core::abi::FILTER_REFUSE
    };
    if refusable {
        FILTERS.refuser(allows).is_none()
    } else {
        FILTERS.each(|filter, user| {
            let _ignored = allows(filter, user);
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
    if !ctx.is_null() && LINKED.get().is_some() {
        // SAFETY: `ctx` is non-null by the check before it and, by the
        // caller's contract, the system's own context at its entry on its
        // thread.
        let _done = ITEMS_FUSE.run("the item uses", || unsafe { screen_pending(ctx, false) });
    }
}

crate::engine::chains::chain_detour! {
    /// The entry of the reflected `equipment_actions` system, a system entry
    /// taking its context: asks the filters about the pending equips first.
    fn equipment_actions(ctx: *mut u64) -> u64;
    next EQUIPMENT_NEXT;
    fuse &EQUIPS_FUSE, "the item equips";
    |onward| {
        if !ctx.is_null() {
            // SAFETY: `ctx` is non-null by the check before it and is the
            // system's own context, at its entry on its thread.
            unsafe { screen_pending(ctx, true) };
        }
        onward.call()
    }
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
        (linked(), crate::engine::ecs::resolved())
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
                unsafe { components.component(ctx, call.character.0, layout.consumed_input) }?;
            let slot = u8::try_from(call.slot).ok()?;
            // SAFETY: the caller guarantees the system's live context, at its
            // entry on its thread.
            let stack = unsafe {
                layout
                    .stacks
                    .holding(components, ctx, call.inventory.0, slot)
            }?;
            // SAFETY: `consumed` is the player's live
            // `ServerConsumedPlayerInput` component.
            Some((stack, unsafe { moves(layout, consumed) }))
        };
        for consumed in USES.taken(now, slot_now) {
            let _heard = ask(&consumed, false);
        }
    }
    let players = players();
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
        let held = unsafe { layout.stacks.holding(components, ctx, inventory, index) };
        let Some((item, count)) = held.filter(|&(item, count)| item != 0 && count != 0) else {
            continue;
        };
        let call = ItemAction {
            header: Header::of::<ItemAction>(),
            kind,
            character: enshwick_core::Entity(player),
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
            USES.watch(
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

/// Lists a pickup the loader raised itself, so the filters do not hear it
/// as one the game made; called after the push, and listed only while the
/// screen reads pickups, so each entry meets its event at the next merge.
pub(crate) fn own_pickup(player: u32, item: u32, count: u32) {
    if screens_pickups() {
        STATE.own_pickups.relock().push((player, item, count));
        STATE
            .own_waiting
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }
}

/// Whether [`follow`] reads the pickups the server's streams hold: a mod
/// filters items, the event filters are on and the build reflects the event.
fn screens_pickups() -> bool {
    wanted() && !EVENTS_FUSE.is_off() && EVENTS.get().is_some_and(|e| e.pickup.is_some())
}

/// Whether the pickup is one the loader raised itself, which it then
/// forgets.
fn own(player: u32, item: u32, count: u32) -> bool {
    if STATE.own_waiting.load(std::sync::atomic::Ordering::Acquire) == 0 {
        return false;
    }
    let mut own = STATE.own_pickups.relock();
    let Some(at) = own.iter().position(|&p| p == (player, item, count)) else {
        return false;
    };
    own.swap_remove(at);
    STATE
        .own_waiting
        .fetch_sub(1, std::sync::atomic::Ordering::Release);
    true
}

/// Forgets a hosted game's watched uses and the loader's own pickups.
pub(crate) fn forget() {
    forget_uses();
    STATE.own_pickups.relock().clear();
    STATE
        .own_waiting
        .store(0, std::sync::atomic::Ordering::Release);
}

/// The item filters' follower in the event move: once a mod filters items,
/// it copies the pickups and placements this build reflects for
/// [`follow`], and only the move hears them, so its being off lists the
/// item event filters off.
pub(crate) const FOLLOWER: EventFollower = EventFollower {
    types: |loader| {
        wanted().then(|| {
            let events = events(loader);
            [events.pickup, events.place]
                .into_iter()
                .flatten()
                .map(|heard| heard.hash)
                .collect()
        })
    },
    screen: None,
    follow: Some(follow),
    fuse: Some(&EVENTS_FUSE),
};

/// Tells the filters of a pickup or a placement in the copy `event` of one
/// event of `type_hash` the server's streams held, on the game thread after
/// the merge.
fn follow(type_hash: u32, event: &[u8]) {
    let (payload, size) = (event.as_ptr(), event.len());
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
    let _done = EVENTS_FUSE.run("the item events", || {
        // SAFETY: the event is of `heard`'s type and at least its reflected
        // size (`find` checks both), readable for `size` bytes (the slice's
        // length); its u32 item id sits at the reflected `heard.item`.
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
        if item == 0
            || player == 0
            || (heard.kind == ItemAction::PICKUP && own(player, item, count))
        {
            return;
        }
        let call = ItemAction {
            header: Header::of::<ItemAction>(),
            kind: heard.kind,
            character: enshwick_core::Entity(player),
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
        stacks: match Stacks::new(3, 12, 0, 4, 8) {
            Some(stacks) => stacks,
            None => panic!("the test stacks fit"),
        },
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

    unsafe extern "system" fn filter(_: *const ItemAction, _: *mut c_void) -> u8 {
        0
    }

    /// The one test that registers a filter: the registry is the process's,
    /// and a sealed one takes no more.
    #[test]
    fn a_registered_filter_is_wanted_once_sealed_and_the_move_follows_it_under_its_events_switch() {
        let loader = crate::start::tests::bare_loader();
        assert!(
            (FOLLOWER.types)(&loader).is_none(),
            "no mod filters items yet"
        );
        FILTERS
            .register(ModIndex::new(0), filter, core::ptr::null_mut())
            .unwrap();
        assert!(!wanted(), "open filters answer nothing yet");
        seal();
        assert!(wanted(), "game events read it before this part links");
        assert!(
            (FOLLOWER.types)(&loader).is_some(),
            "the move follows the filters even where this build reflects neither event"
        );
        assert!(FOLLOWER.follow.is_some() && FOLLOWER.screen.is_none());
        assert!(
            FOLLOWER
                .fuse
                .is_some_and(|fuse| core::ptr::eq(fuse, &raw const EVENTS_FUSE)),
            "the move lists the item event filters off with it, not the item uses"
        );
    }

    #[test]
    fn a_pickup_the_screen_never_reads_is_not_listed_as_the_loaders_own() {
        own_pickup(10, 0x0e17, 3);
        assert_eq!(
            STATE.own_waiting.load(std::sync::atomic::Ordering::Acquire),
            0
        );
        assert!(
            !own(10, 0x0e17, 3),
            "a later real pickup of the same stack reaches the filters"
        );
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
