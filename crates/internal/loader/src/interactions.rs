//! Players' interactions with entities, asked of the mods' filters before
//! the game allows them. A refusal takes the game's own path for a refused
//! action, so nothing moves and nothing is lost; the filters see the server's
//! world only.

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{Header, HookFlags, Interaction, InteractionFn, Side, Status};

use arc_swap::ArcSwapOption;

use crate::chains::Next;
use crate::ecs::{Components, Engine};
use crate::host::{Loader, Registered, take};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "access.base_stock",
    "access.base_stock_ui_return",
    "access.consumed_deposit",
    "access.consumed_upgrade",
    "access.deposit_target",
    "access.deposit_version",
    "access.dismantle_method",
    "access.object_id",
    "access.object_kind",
    "access.offer_test",
    "access.sort",
    "access.sort_target",
    "access.state_input",
    "access.state_owner_index",
    "access.transfer",
    "access.transfer_source",
    "access.transfer_source_slot",
    "access.transfer_target",
    "access.transfer_target_slot",
    "access.upgrade_target",
    "access.upgrade_version",
];

/// The offer result the game writes when it does not offer an interaction.
/// Found on the 1.0 build 1269579, server and client.
const NOT_OFFERED: u8 = 6;
/// How many steps apart the players are read again.
const PLAYERS_EVERY: u64 = 30;

#[derive(Debug, Clone, Copy)]
struct Layout {
    state_input: usize,
    state_owner_index: usize,
    transfer_source: usize,
    transfer_target: usize,
    transfer_source_slot: usize,
    transfer_target_slot: usize,
    sort_target: usize,
    object_id: usize,
    object_kind: usize,
    deposit_target: usize,
    deposit_version: usize,
    consumed_deposit: usize,
    upgrade_target: usize,
    upgrade_version: usize,
    consumed_upgrade: usize,
    /// Where base stock returns to in the client's interface; 0 for none.
    stock_ui_return: u64,
    used_item: u32,
    transform: u32,
    player_input: u32,
    consumed_input: u32,
}

type Filter = Registered<InteractionFn>;

static FILTERS: Mutex<Vec<Filter>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Box<[Filter]>> = OnceLock::new();
static LOADED: OnceLock<(&'static Loader, Layout, Components)> = OnceLock::new();
/// The players' character entities, read on the game thread now and then.
static PLAYERS: ArcSwapOption<Vec<u32>> = ArcSwapOption::const_empty();

// SAFETY: each slot joins the chain of the function its type names.
static OFFER_NEXT: Next<OfferFn> = unsafe { Next::new() };
// SAFETY: as above.
static TRANSFER_NEXT: Next<ActionFn> = unsafe { Next::new() };
// SAFETY: as above.
static SORT_NEXT: Next<ActionFn> = unsafe { Next::new() };
// SAFETY: as above.
static STOCK_NEXT: Next<StockFn> = unsafe { Next::new() };
// SAFETY: as above.
static DISMANTLE_NEXT: Next<DismantleFn> = unsafe { Next::new() };
// SAFETY: as above.
static ACTIONS_NEXT: Next<ActionsFn> = unsafe { Next::new() };

type OfferFn =
    unsafe extern "system" fn(*mut u8, *mut u8, *mut u8, u32, *mut u8, *mut u8, u8, u32) -> *mut u8;
type ActionFn = unsafe extern "system" fn(*mut u64, *mut u8);
type StockFn =
    unsafe extern "system" fn(*mut u8, *mut [u64; 4], *mut u8, *mut u8, *const f32, u32) -> *mut u8;
type DismantleFn = unsafe extern "system" fn(*mut u8, *mut u8, *mut u8, u8) -> u64;
type ActionsFn = unsafe extern "system" fn(*mut u64) -> u64;

fn layout(loader: &Loader) -> Option<Layout> {
    let size = |name: &str| {
        loader
            .build
            .constant(&format!("access.{name}"))
            .and_then(|v| usize::try_from(v).ok())
    };
    let hash = |name: &str| {
        let descriptor = loader.reflection.type_info(name).ok()?.va();
        crate::world::type_hash(loader, descriptor)
    };
    Some(Layout {
        state_input: size("state_input")?,
        state_owner_index: size("state_owner_index")?,
        transfer_source: size("transfer_source")?,
        transfer_target: size("transfer_target")?,
        transfer_source_slot: size("transfer_source_slot")?,
        transfer_target_slot: size("transfer_target_slot")?,
        sort_target: size("sort_target")?,
        object_id: size("object_id")?,
        object_kind: size("object_kind")?,
        deposit_target: size("deposit_target")?,
        deposit_version: size("deposit_version")?,
        consumed_deposit: size("consumed_deposit")?,
        upgrade_target: size("upgrade_target")?,
        upgrade_version: size("upgrade_version")?,
        consumed_upgrade: size("consumed_upgrade")?,
        stock_ui_return: if loader.side == Side::CLIENT {
            loader.base + loader.build.symbol("access.base_stock_ui_return")?
        } else {
            0
        },
        used_item: hash("keen::ecs::UsedItem")?,
        transform: hash("keen::ecs::CurrentTransform")?,
        player_input: hash("keen::ecs::PlayerInput")?,
        consumed_input: hash("keen::ecs::ServerConsumedPlayerInput")?,
    })
}

pub(crate) fn register(
    loader: &Loader,
    owner: usize,
    callback: InteractionFn,
    user: *mut c_void,
) -> Status {
    if loader.delivery.get().is_some() || SEALED.get().is_some() {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none()
        || Components::resolve(loader).is_none()
        || crate::ecs::engine(loader).is_none()
    {
        return Status::MISSING;
    }
    FILTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn link(loader: &'static Loader) {
    let filters = take(&FILTERS);
    let wanted = !filters.is_empty();
    let _first = SEALED.set(filters.into_boxed_slice());
    if !wanted {
        return;
    }
    let (Some(layout), Some(components)) = (layout(loader), Components::resolve(loader)) else {
        return;
    };
    let _first = LOADED.set((loader, layout, components));
    let at = |name: &str| loader.address(&format!("access.{name}"));
    let actions = loader
        .reflection
        .system("inventory_actions")
        .ok()
        .map(enshwick_loader_reflect::System::entry);
    let stock_flags = if layout.stock_ui_return == 0 {
        HookFlags::NONE
    } else {
        HookFlags::CALLER_FRAME
    };
    let none = HookFlags::NONE;
    loader.link_feature(
        "interaction offers",
        at("offer_test"),
        &OFFER_NEXT,
        offer,
        none,
    );
    loader.link_feature(
        "transfers",
        at("transfer"),
        &TRANSFER_NEXT,
        transfer as ActionFn,
        none,
    );
    loader.link_feature("sorting", at("sort"), &SORT_NEXT, sort as ActionFn, none);
    loader.link_feature(
        "base stock",
        at("base_stock"),
        &STOCK_NEXT,
        stock,
        stock_flags,
    );
    loader.link_feature(
        "dismantling",
        at("dismantle_method"),
        &DISMANTLE_NEXT,
        dismantle,
        none,
    );
    loader.link_feature(
        "inventory actions",
        actions,
        &ACTIONS_NEXT,
        inventory_actions,
        none,
    );
    crate::log!(INFO, "interaction filters on");
}

/// Reads the players again now and then, on the game thread.
pub(crate) fn after_step(loader: &Loader, step: u64) {
    if LOADED.get().is_none() || !step.is_multiple_of(PLAYERS_EVERY) {
        return;
    }
    if let Ok(players) = crate::world::players(loader) {
        let entities: Vec<u32> = players
            .iter()
            .map(|p| p.entity)
            .filter(|&e| e != 0)
            .collect();
        PLAYERS.store(Some(std::sync::Arc::new(entities)));
    }
}

pub(crate) fn forget() {
    PLAYERS.store(None);
}

unsafe fn read<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

unsafe fn write<T: Copy>(base: *mut u8, offset: usize, value: T) {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().write_unaligned(value) };
}

fn allowed(call: &Interaction) -> bool {
    let Some(filters) = SEALED.get() else {
        return true;
    };
    filters.iter().filter(|f| f.live()).all(|f| {
        // SAFETY: the callback lives in the mod's DLL, which stays loaded,
        // and takes the pointer it registered; the call is valid for it.
        crate::profile::call(f.owner, crate::profile::Feature::Interactions, || unsafe {
            (f.callback)(call, f.user()) != 0
        })
    })
}

/// The interaction `kind` of `player` with `target`, with what the target
/// holds read in `ctx` when it can be.
///
/// # Safety
///
/// `ctx` is the live context of a running system, used on its thread, or
/// null.
unsafe fn interaction(kind: u32, player: u32, target: u32, ctx: *mut u64) -> Interaction {
    let mut call = Interaction {
        header: Header::of::<Interaction>(),
        kind,
        player,
        target,
        item: 0,
        known: 0,
        reserved: [0; 3],
        position: [0.0; 3],
    };
    if let Some((_, layout, components)) = LOADED.get()
        && !ctx.is_null()
    {
        // SAFETY: forwarded from the caller.
        let item = unsafe { components.component(ctx, target, layout.used_item) };
        // SAFETY: as above.
        let transform = unsafe { components.component(ctx, target, layout.transform) };
        if let (Some(item), Some(transform)) = (item, transform) {
            // SAFETY: UsedItem is the item id alone.
            call.item = unsafe { read(item, 0) };
            // SAFETY: a transform starts with its 32.32 position.
            let fixed: [i64; 3] = unsafe { read(transform, 0) };
            call.position = enshwick_loader_api::ecs::metres(fixed);
            call.known = 1;
        }
    }
    call
}

/// Whether `ctx` belongs to the server's world.
///
/// # Safety
///
/// `ctx` is the live context of a running system, or null.
unsafe fn server_world(loader: &Loader, engine: &Engine, ctx: *mut u64) -> bool {
    loader.side == Side::SERVER
        // SAFETY: forwarded from the caller.
        || unsafe { engine.is_world(ctx, crate::world::server_world()) }
}

/// The acting player of `ctx` when it is a server context.
///
/// # Safety
///
/// `ctx` is the live context of a running system, or null.
unsafe fn server_player(ctx: *mut u64) -> Option<u32> {
    let (loader, _, _) = LOADED.get()?;
    let engine = crate::ecs::resolved()?;
    if ctx.is_null() {
        return None;
    }
    // SAFETY: forwarded from the caller.
    if !unsafe { server_world(loader, engine, ctx) } {
        return None;
    }
    // SAFETY: as above.
    Some(unsafe { engine.entity(ctx) })
}

/// The switch of the interaction filters.
static INTERACTIONS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("interaction filters");

unsafe extern "system" fn offer(
    out: *mut u8,
    query: *mut u8,
    hits: *mut u8,
    candidate: u32,
    shape: *mut u8,
    transform: *mut u8,
    permissions: u8,
    range: u32,
) -> *mut u8 {
    let refused = !out.is_null()
        && !query.is_null()
        && INTERACTIONS_FUSE
            .run("the offer", || {
                // SAFETY: the query state starts with the system context.
                let ctx: *mut u64 = unsafe { read(query, 0) };
                // SAFETY: the offer test runs inside its system, on its thread.
                let Some(player) = (unsafe { server_player(ctx) }) else {
                    return false;
                };
                // SAFETY: as above.
                !allowed(&unsafe { interaction(Interaction::OPEN, player, candidate, ctx) })
            })
            .unwrap_or(false);
    if refused {
        // SAFETY: the offer's 0x14-byte out record.
        unsafe { write(out, 0, NOT_OFFERED) };
        // SAFETY: as above.
        unsafe { write(out, 4, 0u32) };
        // SAFETY: as above.
        unsafe { write(out, 8, 0u64) };
        // SAFETY: as above.
        unsafe { write(out, 0x10, 0u32) };
        return out;
    }
    // SAFETY: forwarded unchanged.
    unsafe {
        OFFER_NEXT.get()(
            out,
            query,
            hits,
            candidate,
            shape,
            transform,
            permissions,
            range,
        )
    }
}

/// Whether the player acting in `ctx` may touch every inventory in `targets`
/// and the roots of `slots`.
///
/// # Safety
///
/// `ctx` and `state` are a running inventory action's.
unsafe fn may_touch(
    kind: u32,
    ctx: *mut u64,
    state: *mut u8,
    targets: &[u32],
    slots: &[u64],
) -> bool {
    let Some((_, layout, components)) = LOADED.get() else {
        return true;
    };
    // SAFETY: forwarded from the caller.
    let Some(player) = (unsafe { server_player(ctx) }) else {
        return true;
    };
    // SAFETY: the action state holds the system's OwnerRelationship index.
    let owner_index: u64 = unsafe { read(state, layout.state_owner_index) };
    let roots = slots.iter().filter(|&&slot| slot != 0).filter_map(|&slot| {
        // SAFETY: as above.
        unsafe { components.slot_root(ctx, slot, owner_index) }
    });
    targets
        .iter()
        .copied()
        .chain(roots)
        .filter(|&target| target != 0)
        // SAFETY: as above.
        .all(|target| allowed(&unsafe { interaction(kind, player, target, ctx) }))
}

unsafe extern "system" fn transfer(ctx: *mut u64, state: *mut u8) {
    let refused = LOADED.get().is_some_and(|(_, layout, _)| {
        !state.is_null()
            && INTERACTIONS_FUSE
                .run("the transfer", || {
                    // SAFETY: the action state points at the player's input.
                    let input: *const u8 = unsafe { read(state, layout.state_input) };
                    if input.is_null() {
                        return false;
                    }
                    // SAFETY: the input holds the transfer at these offsets.
                    let source: u32 = unsafe { read(input, layout.transfer_source) };
                    // SAFETY: as above.
                    let target: u32 = unsafe { read(input, layout.transfer_target) };
                    // SAFETY: as above.
                    let source_slot: u64 = unsafe { read(input, layout.transfer_source_slot) };
                    // SAFETY: as above.
                    let target_slot: u64 = unsafe { read(input, layout.transfer_target_slot) };
                    // SAFETY: the running action's context and state.
                    !unsafe {
                        may_touch(
                            Interaction::MOVE,
                            ctx,
                            state,
                            &[source, target],
                            &[source_slot, target_slot],
                        )
                    }
                })
                .unwrap_or(false)
    });
    if refused {
        // Skipped: the caller marks the action consumed and resends the
        // inventories, undoing the client's move.
        return;
    }
    // SAFETY: forwarded unchanged.
    unsafe { TRANSFER_NEXT.get()(ctx, state) }
}

unsafe extern "system" fn sort(ctx: *mut u64, state: *mut u8) {
    let refused = LOADED.get().is_some_and(|(_, layout, _)| {
        !state.is_null()
            && INTERACTIONS_FUSE
                .run("the sort", || {
                    // SAFETY: as for the transfer.
                    let input: *const u8 = unsafe { read(state, layout.state_input) };
                    if input.is_null() {
                        return false;
                    }
                    // SAFETY: the input holds the sort's target there.
                    let target: u32 = unsafe { read(input, layout.sort_target) };
                    // SAFETY: the running action's context and state.
                    !unsafe { may_touch(Interaction::SORT, ctx, state, &[target], &[]) }
                })
                .unwrap_or(false)
    });
    if refused {
        return;
    }
    // SAFETY: forwarded unchanged.
    unsafe { SORT_NEXT.get()(ctx, state) }
}

thread_local! {
    /// Filtered copies of the base stock lists, kept for the process: the
    /// game may read a list it was handed long after, and the same filtered
    /// list comes back for every later ask.
    static STOCK: RefCell<rustc_hash::FxHashSet<&'static [u32]>> =
        RefCell::new(rustc_hash::FxHashSet::default());
}

/// `list`, kept for the process on this thread: the same address for the
/// same ids.
fn kept_list(list: &[u32]) -> &'static [u32] {
    STOCK.with_borrow_mut(|kept| {
        if let Some(&found) = kept.get(list) {
            return found;
        }
        let leaked: &'static [u32] = Box::leak(list.into());
        kept.insert(leaked);
        leaked
    })
}

unsafe extern "system" fn stock(
    out: *mut u8,
    lists: *mut [u64; 4],
    server: *mut u8,
    client: *mut u8,
    position: *const f32,
    player: u32,
) -> *mut u8 {
    // Asked by the client's own interface, which sees the client's world;
    // read before the chain calls on.
    let ui = LOADED.get().is_some_and(|(loader, layout, _)| {
        if layout.stock_ui_return == 0 {
            return false;
        }
        let frame = (loader.caller_frame)();
        // SAFETY: the frame was taken at entry of this chain.
        frame.rsp != 0 && unsafe { frame.return_address() } == layout.stock_ui_return
    });
    // SAFETY: forwarded unchanged.
    let result = unsafe { STOCK_NEXT.get()(out, lists, server, client, position, player) };
    // SAFETY: the first byte is the game's error code, 0 for success.
    if ui || out.is_null() || lists.is_null() || unsafe { out.read() } != 0 {
        return result;
    }
    // SAFETY: the game's lists, filled for this call.
    let _done = INTERACTIONS_FUSE.run("the stock", || unsafe { filter_stock(lists, player) });
    result
}

/// Leaves out of the stock lists every inventory a filter refuses.
///
/// # Safety
///
/// `lists` holds the stock ids and count and the fuel ids and count the game
/// just filled.
unsafe fn filter_stock(lists: *mut [u64; 4], player: u32) {
    // SAFETY: forwarded from the caller.
    let mut found = unsafe { lists.read() };
    let list = |ptr: u64, count: u64| -> &[u32] {
        let ptr = core::ptr::with_exposed_provenance::<u32>(usize::try_from(ptr).unwrap_or(0));
        if ptr.is_null() {
            return &[];
        }
        // SAFETY: the base's arrays hold `count` ids, valid for this call.
        unsafe { core::slice::from_raw_parts(ptr, usize::try_from(count).unwrap_or(0)) }
    };
    let (stock, fuel) = (list(found[0], found[1]), list(found[2], found[3]));
    let refused = |id: u32| {
        // SAFETY: no context: the call carries no item or position.
        !allowed(&unsafe { interaction(Interaction::STOCK, player, id, core::ptr::null_mut()) })
    };
    let Some(first) = stock.iter().chain(fuel).position(|&id| refused(id)) else {
        return;
    };
    // Never filter in place: every player reads the same arrays.
    let mut index = 0;
    let mut copy = Vec::new();
    for (source, at) in [(stock, 0), (fuel, 2)] {
        copy.clear();
        for &id in source {
            let keep = match index.cmp(&first) {
                core::cmp::Ordering::Less => true,
                core::cmp::Ordering::Equal => false,
                core::cmp::Ordering::Greater => !refused(id),
            };
            index += 1;
            if keep {
                copy.push(id);
            }
        }
        let kept = kept_list(&copy);
        found[at] = kept.as_ptr().expose_provenance() as u64;
        found[at + 1] = kept.len() as u64;
    }
    // SAFETY: the game's lists, rewritten to the filtered copies.
    unsafe { lists.write(found) };
}

unsafe extern "system" fn dismantle(
    ctx: *mut u8,
    object: *mut u8,
    held: *mut u8,
    permissions: u8,
) -> u64 {
    // SAFETY: forwarded unchanged.
    let method = unsafe { DISMANTLE_NEXT.get()(ctx, object, held, permissions) };
    if method.trailing_zeros() >= 8 || ctx.is_null() || object.is_null() {
        return method;
    }
    let Some((_, layout, _)) = LOADED.get() else {
        return method;
    };
    let refused = INTERACTIONS_FUSE
        .run("the dismantle", || {
            // SAFETY: the object describes an entity when its kind word is 0.
            let id: u64 = unsafe { read(object, layout.object_id) };
            // SAFETY: as above.
            let kind: u64 = unsafe { read(object, layout.object_kind) };
            if kind != 0 {
                return false;
            }
            // SAFETY: the cursor context starts with the system context.
            let system: *mut u64 = unsafe { read(ctx, 0) };
            // SAFETY: the cursor runs inside its system, on its thread.
            let Some(player) = (unsafe { server_player(system) }) else {
                return false;
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the entity id is the low half"
            )]
            let target = id as u32;
            // SAFETY: as above.
            !allowed(&unsafe { interaction(Interaction::DISMANTLE, player, target, system) })
        })
        .unwrap_or(false);
    if refused { 0 } else { method }
}

/// Before the inventory system runs, marks as done the Quick Stash switch and
/// the upgrade a filter refuses; the system then skips them as already
/// handled.
unsafe extern "system" fn inventory_actions(ctx: *mut u64) -> u64 {
    if LOADED.get().is_some() && !ctx.is_null() {
        // SAFETY: the system's context, at its entry on its thread.
        let _done =
            INTERACTIONS_FUSE.run("the inventory actions", || unsafe { refuse_pending(ctx) });
    }
    // SAFETY: forwarded unchanged.
    unsafe { ACTIONS_NEXT.get()(ctx) }
}

/// # Safety
///
/// `ctx` is the inventory system's live context, at its entry.
unsafe fn refuse_pending(ctx: *mut u64) {
    let (Some((loader, layout, components)), Some(engine)) = (LOADED.get(), crate::ecs::resolved())
    else {
        return;
    };
    // SAFETY: forwarded from the caller.
    if !unsafe { server_world(loader, engine, ctx) } {
        return;
    }
    let players = PLAYERS.load();
    let Some(players) = players.as_deref() else {
        return;
    };
    for &player in players {
        // SAFETY: the system's context, at its entry on its thread.
        let input = unsafe { components.component(ctx, player, layout.player_input) };
        // SAFETY: as above.
        let consumed = unsafe { components.component(ctx, player, layout.consumed_input) };
        let (Some(input), Some(consumed)) = (input, consumed) else {
            continue;
        };
        for (kind, target_at, version_at, done_at) in [
            (
                Interaction::QUICK_STASH,
                layout.deposit_target,
                layout.deposit_version,
                layout.consumed_deposit,
            ),
            (
                Interaction::UPGRADE,
                layout.upgrade_target,
                layout.upgrade_version,
                layout.consumed_upgrade,
            ),
        ] {
            // SAFETY: the player's input and its server-side consumed record.
            let version: u32 = unsafe { read(input, version_at) };
            // SAFETY: as above.
            let done: u32 = unsafe { read(consumed, done_at) };
            // SAFETY: as above.
            let target: u32 = unsafe { read(input, target_at) };
            // SAFETY: inside the system, on its thread.
            if version != done && !allowed(&unsafe { interaction(kind, player, target, ctx) }) {
                // SAFETY: the consumed record is server-only; no client sees
                // it change.
                unsafe { write(consumed, done_at, version) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filtered_stock_list_keeps_its_address_for_every_later_ask() {
        let first = kept_list(&[3, 5, 8]);
        let other = kept_list(&[3, 5]);
        let again = kept_list(&[3, 5, 8]);
        assert_eq!(first.as_ptr(), again.as_ptr());
        assert_eq!((first, other), (&[3, 5, 8][..], &[3, 5][..]));
    }
}
