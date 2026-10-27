//! Players' interactions with entities, asked of the mods' filters before
//! the game allows them. A refusal takes the game's own path for a refused
//! action, so nothing moves and nothing is lost; the filters see the server's
//! world only.

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::OnceLock;

use arc_swap::ArcSwapOption;
use enshwick::abi::{Header, HookFlags, Interaction, InteractionFn, Side, Status};
use enshwick::raw::{read_at, read_ptr_at, write_at};

use crate::chains::Next;
use crate::ecs::{Components, Engine};
use crate::host::{Loader, Registered};
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::access::BASE_STOCK.id(),
    pinned::access::BASE_STOCK_UI_RETURN.id(),
    pinned::access::CONSUMED_DEPOSIT.id(),
    pinned::access::CONSUMED_UPGRADE.id(),
    pinned::access::DEPOSIT_TARGET.id(),
    pinned::access::DEPOSIT_VERSION.id(),
    pinned::access::DISMANTLE_METHOD.id(),
    pinned::access::OBJECT_ID.id(),
    pinned::access::OBJECT_KIND.id(),
    pinned::access::OFFER_TEST.id(),
    pinned::access::SORT.id(),
    pinned::access::SORT_TARGET.id(),
    pinned::access::STATE_INPUT.id(),
    pinned::access::STATE_OWNER_INDEX.id(),
    pinned::access::TRANSFER.id(),
    pinned::access::TRANSFER_SOURCE.id(),
    pinned::access::TRANSFER_SOURCE_SLOT.id(),
    pinned::access::TRANSFER_TARGET.id(),
    pinned::access::TRANSFER_TARGET_SLOT.id(),
    pinned::access::UPGRADE_TARGET.id(),
    pinned::access::UPGRADE_VERSION.id(),
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

static FILTERS: Registry<Filter> = Registry::new();
static LOADED: OnceLock<(&'static Loader, Layout, Components)> = OnceLock::new();
/// The players' character entities, read on the game thread now and then.
static PLAYERS: ArcSwapOption<Vec<u32>> = ArcSwapOption::const_empty();

// SAFETY: `OfferFn` is the signature of the build table's
// `access.offer_test`, and `link` uses this slot only as that function's
// next.
static OFFER_NEXT: Next<OfferFn> = unsafe { Next::new() };
// SAFETY: `ActionFn` is the signature of the build table's
// `access.transfer`, and `link` uses this slot only as that function's next.
static TRANSFER_NEXT: Next<ActionFn> = unsafe { Next::new() };
// SAFETY: `ActionFn` is the signature of the build table's `access.sort`,
// and `link` uses this slot only as that function's next.
static SORT_NEXT: Next<ActionFn> = unsafe { Next::new() };
// SAFETY: `StockFn` is the signature of the build table's
// `access.base_stock`, and `link` uses this slot only as that function's
// next.
static STOCK_NEXT: Next<StockFn> = unsafe { Next::new() };
// SAFETY: `DismantleFn` is the signature of the build table's
// `access.dismantle_method`, and `link` uses this slot only as that
// function's next.
static DISMANTLE_NEXT: Next<DismantleFn> = unsafe { Next::new() };
// SAFETY: `ActionsFn` is the signature of a system entry, here the
// reflected `inventory_actions` system's, and `link` uses this slot only as
// that function's next.
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
        crate::world::type_hash(descriptor)
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
            loader.base
                + loader
                    .build
                    .symbol(pinned::access::BASE_STOCK_UI_RETURN.id())?
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
    owner: ModIndex,
    callback: InteractionFn,
    user: *mut c_void,
) -> Status {
    if loader.delivery.get().is_some() || FILTERS.is_sealed() {
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

pub(crate) fn link(loader: &'static Loader) {
    let filters = FILTERS.seal();
    // Linked on every side, as any can host a world whose transactions hold
    // props; the detours act only in a server's world.
    let (Some(layout), Some(components)) = (layout(loader), Components::resolve(loader)) else {
        crate::transactions::holds().linked(filters.len(), false);
        return;
    };
    let _first = LOADED.set((loader, layout, components));
    let at = |name: &str| loader.address_by_name(&format!("access.{name}"));
    let actions = loader
        .reflection
        .system("inventory_actions")
        .ok()
        .map(enshwick_gamedata::reflect::System::entry);
    let stock_flags = if layout.stock_ui_return == 0 {
        HookFlags::NONE
    } else {
        HookFlags::CALLER_FRAME
    };
    let none = HookFlags::NONE;
    let offers = loader.link_feature(
        "interaction offers",
        at("offer_test"),
        &OFFER_NEXT,
        offer,
        none,
    );
    let transfers = loader.link_feature(
        "transfers",
        at("transfer"),
        &TRANSFER_NEXT,
        transfer as ActionFn,
        none,
    );
    let sorting = loader.link_feature("sorting", at("sort"), &SORT_NEXT, sort as ActionFn, none);
    let _stock = loader.link_feature(
        "base stock",
        at("base_stock"),
        &STOCK_NEXT,
        stock,
        stock_flags,
    );
    let dismantling = loader.link_feature(
        "dismantling",
        at("dismantle_method"),
        &DISMANTLE_NEXT,
        dismantle,
        none,
    );
    let actions = loader.link_feature(
        "inventory actions",
        actions,
        &ACTIONS_NEXT,
        inventory_actions,
        none,
    );
    let hold_ready = offers && transfers && sorting && dismantling && actions;
    if !hold_ready {
        crate::log!(
            WARN,
            "prop removal by transactions off: a hook that keeps players from a prop did not link"
        );
    }
    crate::transactions::holds().linked(filters.len(), hold_ready);
    crate::log!(INFO, "interaction filters on");
}

/// Whether the filter's fuse is off, after a panic in its work: a hold
/// would not keep anyone out.
pub(crate) fn filter_off() -> bool {
    INTERACTIONS_FUSE.is_off()
}

/// Reads the players again now and then, and lets destroyed props go, on
/// the game thread.
pub(crate) fn after_step(loader: &Loader, step: u64) {
    crate::transactions::holds().after_step(step);
    if LOADED.get().is_none() || !step.is_multiple_of(PLAYERS_EVERY) {
        return;
    }
    if let Ok(entities) = crate::world::player_entities(loader)
        && PLAYERS.load().as_deref().map(Vec::as_slice) != Some(entities.as_slice())
    {
        PLAYERS.store(Some(std::sync::Arc::new(entities.to_vec())));
    }
}

pub(crate) fn forget() {
    PLAYERS.store(None);
}

fn allowed(call: &Interaction) -> bool {
    if crate::transactions::holds().is_held(call.target.0) {
        return false;
    }
    let Some(filters) = FILTERS.sealed() else {
        return true;
    };
    filters.iter().filter(|f| f.live()).all(|f| {
        // SAFETY: `f` is live, so the mod's DLL that holds the callback stays
        // loaded; the callback gets the user pointer its mod registered and
        // `call`, which lives for the call.
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
/// - `ctx` is null or the live context of a running system, used on its
///   thread.
unsafe fn interaction(kind: u32, player: u32, target: u32, ctx: *mut u64) -> Interaction {
    let mut call = Interaction {
        header: Header::of::<Interaction>(),
        kind,
        player: enshwick::Entity(player),
        target: enshwick::Entity(target),
        item: enshwick::ItemId::new(0),
        known: 0,
        reserved: [0; 3],
        position: [0.0; 3],
    };
    if let Some((_, layout, components)) = LOADED.get()
        && !ctx.is_null()
    {
        // SAFETY: `ctx` is non-null by the check before it and, by the caller's
        // contract, a running system's live context on its thread.
        let item = unsafe { components.component(ctx, target, layout.used_item) };
        // SAFETY: `ctx` is non-null by the check before it and, by the caller's
        // contract, a running system's live context on its thread.
        let transform = unsafe { components.component(ctx, target, layout.transform) };
        if let (Some(item), Some(transform)) = (item, transform) {
            // SAFETY: `item` is the target's live `UsedItem` component, which
            // is the u32 item id alone.
            call.item = unsafe { read_at(item, 0) };
            // SAFETY: `transform` is the target's live `CurrentTransform`
            // component, which starts with its 32.32 position, three i64.
            let fixed: [i64; 3] = unsafe { read_at(transform, 0) };
            call.position = enshwick::ecs::metres(fixed);
            call.known = 1;
        }
    }
    call
}

/// Whether `ctx` belongs to the server's world.
///
/// # Safety
///
/// - `ctx` is null or the live context of a running system, used on its
///   thread.
unsafe fn server_world(loader: &Loader, engine: &Engine, ctx: *mut u64) -> bool {
    loader.side == Side::SERVER
        // SAFETY: the caller guarantees a null or live system context on its
        // thread, all `is_world` asks for.
        || unsafe { engine.is_world(ctx, crate::world::server_world()) }
}

/// The acting player of `ctx` when it is a server context.
///
/// # Safety
///
/// - `ctx` is null or the live context of a running system, used on its
///   thread.
unsafe fn server_player(ctx: *mut u64) -> Option<u32> {
    let (loader, _, _) = LOADED.get()?;
    let engine = crate::ecs::resolved()?;
    if ctx.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a null or live system context on its
    // thread, all `server_world` asks for.
    if !unsafe { server_world(loader, engine, ctx) } {
        return None;
    }
    // SAFETY: `ctx` is non-null by the check before it and, by the caller's
    // contract, a running system's live context on its thread.
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
    if crate::transactions::holds().quiet() {
        // SAFETY: this detour runs only as the `access.offer_test` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe {
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
        };
    }
    let refused = !out.is_null()
        && !query.is_null()
        && INTERACTIONS_FUSE
            .run("the offer", || {
                // SAFETY: `query` is non-null by the check before it and is the
                // offer test's query state, which starts with its system
                // context pointer.
                let ctx = unsafe { read_ptr_at::<u64>(query, 0) };
                // SAFETY: `ctx` is null or the system context of the running
                // offer test, which runs inside its system on its thread.
                let Some(player) = (unsafe { server_player(ctx) }) else {
                    return false;
                };
                // SAFETY: `ctx` is the system context of the running offer
                // test, on its system's thread.
                !allowed(&unsafe { interaction(Interaction::OPEN, player, candidate, ctx) })
            })
            .unwrap_or(false);
    if refused {
        // SAFETY: `out` is non-null by the check before it and is the offer
        // test's 0x14-byte out record, which starts with its u8 result.
        unsafe { write_at(out, 0, NOT_OFFERED) };
        // SAFETY: `out` is the offer test's 0x14-byte out record; bytes 4..8
        // are inside it.
        unsafe { write_at(out, 4, 0u32) };
        // SAFETY: `out` is the offer test's 0x14-byte out record; bytes
        // 8..0x10 are inside it.
        unsafe { write_at(out, 8, 0u64) };
        // SAFETY: `out` is the offer test's 0x14-byte out record; bytes
        // 0x10..0x14 are inside it.
        unsafe { write_at(out, 0x10, 0u32) };
        return out;
    }
    // SAFETY: this detour runs only as the `access.offer_test` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
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
/// - `ctx` is null or the live system context of a running inventory
///   action, used on its thread.
/// - `state` is that action's live, non-null state.
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
    // SAFETY: the caller guarantees a null or live system context on its
    // thread, all `server_player` asks for.
    let Some(player) = (unsafe { server_player(ctx) }) else {
        return true;
    };
    // SAFETY: the caller guarantees the action's live state; its
    // `OwnerRelationship` index sits at the build table's
    // `access.state_owner_index` offset.
    let owner_index: u64 = unsafe { read_at(state, layout.state_owner_index) };
    let roots = slots.iter().filter(|&&slot| slot != 0).filter_map(|&slot| {
        // SAFETY: `server_player` found a player, so `ctx` is non-null and,
        // by the caller's contract, the live context on its thread;
        // `owner_index` comes from the same system's action state.
        unsafe { components.slot_root(ctx, slot, owner_index) }
    });
    targets
        .iter()
        .copied()
        .chain(roots)
        .filter(|&target| target != 0)
        // SAFETY: the caller guarantees `ctx` is null or the live system
        // context on its thread.
        .all(|target| allowed(&unsafe { interaction(kind, player, target, ctx) }))
}

unsafe extern "system" fn transfer(ctx: *mut u64, state: *mut u8) {
    if crate::transactions::holds().quiet() {
        // SAFETY: this detour runs only as the `access.transfer` link, so the
        // slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { TRANSFER_NEXT.get()(ctx, state) };
    }
    let refused = LOADED.get().is_some_and(|(_, layout, _)| {
        !state.is_null()
            && INTERACTIONS_FUSE
                .run("the transfer", || {
                    // SAFETY: `state` is non-null by the check before it and is
                    // the transfer's action state; its pointer to the
                    // player's input sits at the build table's
                    // `access.state_input` offset.
                    let input =
                        unsafe { read_ptr_at::<u8>(state, layout.state_input) }.cast_const();
                    if input.is_null() {
                        return false;
                    }
                    // SAFETY: `input` is the player's non-null input by the
                    // check before it; its u32 source sits at the build table's
                    // `access.transfer_source` offset.
                    let source: u32 = unsafe { read_at(input, layout.transfer_source) };
                    // SAFETY: `input` is the player's non-null input; its u32
                    // target sits at the build table's
                    // `access.transfer_target` offset.
                    let target: u32 = unsafe { read_at(input, layout.transfer_target) };
                    // SAFETY: `input` is the player's non-null input; its u64
                    // source slot sits at the build table's
                    // `access.transfer_source_slot` offset.
                    let source_slot: u64 = unsafe { read_at(input, layout.transfer_source_slot) };
                    // SAFETY: `input` is the player's non-null input; its u64
                    // target slot sits at the build table's
                    // `access.transfer_target_slot` offset.
                    let target_slot: u64 = unsafe { read_at(input, layout.transfer_target_slot) };
                    // SAFETY: `ctx` and `state` are the running transfer's own
                    // arguments, on its system's thread; `state` is non-null.
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
    // SAFETY: this detour runs only as the `access.transfer` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    unsafe { TRANSFER_NEXT.get()(ctx, state) }
}

unsafe extern "system" fn sort(ctx: *mut u64, state: *mut u8) {
    if crate::transactions::holds().quiet() {
        // SAFETY: this detour runs only as the `access.sort` link, so the
        // slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { SORT_NEXT.get()(ctx, state) };
    }
    let refused = LOADED.get().is_some_and(|(_, layout, _)| {
        !state.is_null()
            && INTERACTIONS_FUSE
                .run("the sort", || {
                    // SAFETY: `state` is non-null by the check before it and is
                    // the sort's action state; its pointer to the player's
                    // input sits at the build table's `access.state_input`
                    // offset.
                    let input =
                        unsafe { read_ptr_at::<u8>(state, layout.state_input) }.cast_const();
                    if input.is_null() {
                        return false;
                    }
                    // SAFETY: `input` is the player's non-null input by the
                    // check before it; its u32 sort target sits at the build
                    // table's `access.sort_target` offset.
                    let target: u32 = unsafe { read_at(input, layout.sort_target) };
                    // SAFETY: `ctx` and `state` are the running sort's own
                    // arguments, on its system's thread; `state` is non-null.
                    !unsafe { may_touch(Interaction::SORT, ctx, state, &[target], &[]) }
                })
                .unwrap_or(false)
    });
    if refused {
        return;
    }
    // SAFETY: this detour runs only as the `access.sort` link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { SORT_NEXT.get()(ctx, state) }
}

/// Filtered copies of the base stock lists one thread handed out in one
/// world step, by their ids.
#[derive(Default)]
struct Stock {
    step: u64,
    lists: rustc_hash::FxHashSet<Box<[u32]>>,
}

thread_local! {
    /// This thread's filtered stock lists of the current step: the same
    /// list for the same ids within the step. The game's callers walk a list
    /// before they return, so a new step retires the old step's lists.
    static STOCK: RefCell<Stock> = RefCell::new(Stock::default());
}

/// A copy of `list` that stays unchanged at its address for the rest of
/// this step and [`crate::retire::GRACE`] steps after: its address and
/// length.
fn kept_list(list: &[u32]) -> (*const u32, usize) {
    kept_in_step(list, crate::retire::current())
}

/// [`kept_list`] in world step `step`.
fn kept_in_step(list: &[u32], step: u64) -> (*const u32, usize) {
    STOCK.with_borrow_mut(|kept| {
        if kept.step != step {
            let old = std::mem::take(&mut kept.lists);
            if !old.is_empty() {
                crate::retire::retire(old);
            }
            kept.step = step;
        }
        if let Some(found) = kept.lists.get(list) {
            return (found.as_ptr(), found.len());
        }
        let copy: Box<[u32]> = list.into();
        let at = (copy.as_ptr(), copy.len());
        kept.lists.insert(copy);
        at
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
    if crate::transactions::holds().quiet() {
        // SAFETY: this detour runs only as the `access.base_stock` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { STOCK_NEXT.get()(out, lists, server, client, position, player) };
    }
    // Asked by the client's own interface, which sees the client's world;
    // read before the chain calls on.
    let ui = LOADED.get().is_some_and(|(loader, layout, _)| {
        if layout.stock_ui_return == 0 {
            return false;
        }
        let frame = (loader.caller_frame)();
        // SAFETY: the link asks for `CALLER_FRAME` when `stock_ui_return` is
        // set, so the frame was taken at entry of this chain and `rsp`,
        // non-zero by the check before it, points at the live return
        // address.
        frame.rsp != 0 && unsafe { frame.return_address() } == layout.stock_ui_return
    });
    // SAFETY: this detour runs only as the `access.base_stock` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let result = unsafe { STOCK_NEXT.get()(out, lists, server, client, position, player) };
    // SAFETY: `out` is non-null by the check before it and is the base
    // stock's result, whose first byte is the game's error code, 0 for
    // success.
    if ui || out.is_null() || lists.is_null() || unsafe { read_at::<u8>(out, 0) } != 0 {
        return result;
    }
    // SAFETY: `lists` is non-null by the check before it and is the base stock's
    // lists argument, which the game has just filled, on its thread.
    let _done = INTERACTIONS_FUSE.run("the stock", || unsafe { filter_stock(lists, player) });
    result
}

/// Leaves out of the stock lists every inventory a filter refuses.
///
/// # Safety
///
/// - `lists` is the non-null, writable `[u64; 4]` the game's base stock has
///   just filled: the stock ids and count, then the fuel ids and count.
/// - Each ids pointer is null or readable for its count of `u32` ids, alive
///   for the call.
/// - Called on the base stock's thread, which alone writes `lists`
///   meanwhile.
unsafe fn filter_stock(lists: *mut [u64; 4], player: u32) {
    // SAFETY: the caller guarantees `lists` is the readable `[u64; 4]` the
    // game has just filled.
    let mut found: [u64; 4] = unsafe { read_at(lists.cast_const().cast(), 0) };
    let ids = |ptr: u64, count: u64| {
        let ptr = core::ptr::with_exposed_provenance::<u8>(usize::try_from(ptr).unwrap_or(0));
        let count = if ptr.is_null() {
            0
        } else {
            usize::try_from(count).unwrap_or(0)
        };
        (0..count).map(move |i| {
            // SAFETY: `ptr` is non-null whenever `count` is, and the caller
            // guarantees it holds `count` readable u32 ids; `i` is below
            // `count`.
            unsafe { read_at::<u32>(ptr, i * 4) }
        })
    };
    let (stock, fuel) = (ids(found[0], found[1]), ids(found[2], found[3]));
    let refused = |id: u32| {
        // SAFETY: a null context, which `interaction` takes: the call carries
        // no item or position.
        !allowed(&unsafe { interaction(Interaction::STOCK, player, id, core::ptr::null_mut()) })
    };
    let Some(first) = stock.clone().chain(fuel.clone()).position(refused) else {
        return;
    };
    // Never filter in place: every player reads the same arrays.
    let mut index = 0;
    let mut copy = Vec::new();
    for (source, at) in [(stock, 0), (fuel, 2)] {
        copy.clear();
        for id in source {
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
        let (kept, len) = kept_list(&copy);
        found[at] = kept.expose_provenance() as u64;
        found[at + 1] = len as u64;
    }
    // SAFETY: the caller guarantees `lists` is writable and only this thread
    // writes it; the copies it now names stay unchanged until the world has
    // stepped past this caller, which reads them before it returns.
    unsafe { write_at(lists.cast(), 0, found) };
}

unsafe extern "system" fn dismantle(
    ctx: *mut u8,
    object: *mut u8,
    held: *mut u8,
    permissions: u8,
) -> u64 {
    // SAFETY: this detour runs only as the `access.dismantle_method` link, so
    // the slot holds the next function of that signature; the game's
    // arguments pass unchanged.
    let method = unsafe { DISMANTLE_NEXT.get()(ctx, object, held, permissions) };
    if crate::transactions::holds().quiet() {
        return method;
    }
    if method.trailing_zeros() >= 8 || ctx.is_null() || object.is_null() {
        return method;
    }
    let Some((_, layout, _)) = LOADED.get() else {
        return method;
    };
    let refused = INTERACTIONS_FUSE
        .run("the dismantle", || {
            // SAFETY: `object` is non-null by the check before it and is the
            // dismantle's object; its u64 id sits at the build table's
            // `access.object_id` offset.
            let id: u64 = unsafe { read_at(object, layout.object_id) };
            // SAFETY: `object` is non-null by the check before it and is the
            // dismantle's object; its u64 kind, 0 for an entity, sits at the
            // build table's `access.object_kind` offset.
            let kind: u64 = unsafe { read_at(object, layout.object_kind) };
            if kind != 0 {
                return false;
            }
            // SAFETY: `ctx` is non-null by the check before it and is the
            // cursor context, which starts with its system context pointer.
            let system = unsafe { read_ptr_at::<u64>(ctx, 0) };
            // SAFETY: `system` is null or the system context of the running
            // cursor, which runs inside its system on its thread.
            let Some(player) = (unsafe { server_player(system) }) else {
                return false;
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the entity id is the low half"
            )]
            let target = id as u32;
            // SAFETY: `system` is the running cursor's system context, on
            // its thread.
            !allowed(&unsafe { interaction(Interaction::DISMANTLE, player, target, system) })
        })
        .unwrap_or(false);
    if refused { 0 } else { method }
}

/// Before the inventory system runs, marks as done the Quick Stash switch and
/// the upgrade a filter refuses; the system then skips them as already
/// handled.
unsafe extern "system" fn inventory_actions(ctx: *mut u64) -> u64 {
    if !crate::transactions::holds().quiet() && LOADED.get().is_some() && !ctx.is_null() {
        // SAFETY: `ctx` is non-null by the check before it and is the system's
        // own context, at its entry on its thread.
        let _done =
            INTERACTIONS_FUSE.run("the inventory actions", || unsafe { refuse_pending(ctx) });
    }
    // SAFETY: this detour runs only as the `inventory_actions` system's link,
    // so the slot holds the next function of that signature; the game's
    // context passes unchanged.
    unsafe { ACTIONS_NEXT.get()(ctx) }
}

/// Marks the pending Quick Stash switches and upgrades a filter refuses as
/// already handled.
///
/// # Safety
///
/// - `ctx` is the inventory system's live, non-null context, at its entry
///   on its thread.
unsafe fn refuse_pending(ctx: *mut u64) {
    let (Some((loader, layout, components)), Some(engine)) = (LOADED.get(), crate::ecs::resolved())
    else {
        return;
    };
    // SAFETY: the caller guarantees the system's live context on its thread.
    if !unsafe { server_world(loader, engine, ctx) } {
        return;
    }
    let players = PLAYERS.load();
    let Some(players) = players.as_deref() else {
        return;
    };
    for &player in players {
        // SAFETY: the caller guarantees the system's live context, at its
        // entry on its thread.
        let input = unsafe { components.component(ctx, player, layout.player_input) };
        // SAFETY: the caller guarantees the system's live context, at its
        // entry on its thread.
        let consumed = unsafe { components.component_mut(ctx, player, layout.consumed_input) };
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
            // SAFETY: `input` is the player's live `PlayerInput` component;
            // its u32 version sits at the build table's
            // `access.deposit_version` or `access.upgrade_version` offset.
            let version: u32 = unsafe { read_at(input, version_at) };
            // SAFETY: `consumed` is the player's live
            // `ServerConsumedPlayerInput` component; its u32 handled version
            // sits at the build table's `access.consumed_deposit` or
            // `access.consumed_upgrade` offset.
            let done: u32 = unsafe { read_at(consumed, done_at) };
            // SAFETY: `input` is the player's live `PlayerInput` component;
            // its u32 target sits at the build table's
            // `access.deposit_target` or `access.upgrade_target` offset.
            let target: u32 = unsafe { read_at(input, target_at) };
            // SAFETY: the caller guarantees the system's live context, on its
            // thread.
            if version != done && !allowed(&unsafe { interaction(kind, player, target, ctx) }) {
                // SAFETY: `consumed` is the player's live, server-only
                // consumed record, written at the system's entry on its
                // thread before the system reads it; no client sees it
                // change.
                unsafe { write_at(consumed, done_at, version) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ids at a kept list's address.
    fn ids((at, len): (*const u32, usize)) -> Vec<u32> {
        // SAFETY: `at` and `len` come from `kept_in_step` in the same step on
        // this thread, so the copy is in this thread's cache, unchanged.
        unsafe { core::slice::from_raw_parts(at, len) }.to_vec()
    }

    #[test]
    fn a_filtered_stock_list_keeps_its_address_within_a_step() {
        let first = kept_in_step(&[3, 5, 8], 1_000);
        let other = kept_in_step(&[3, 5], 1_000);
        let again = kept_in_step(&[3, 5, 8], 1_000);
        assert_eq!(first, again);
        assert_eq!((ids(first), ids(other)), (vec![3, 5, 8], vec![3, 5]));
    }

    #[test]
    fn a_new_step_retires_the_old_lists_and_copies_again() {
        let first = kept_in_step(&[4, 6], 2_000);
        assert_eq!(ids(first), [4, 6]);
        let next = kept_in_step(&[4, 6], 2_001);
        assert_eq!(ids(next), [4, 6]);
        STOCK.with_borrow(|kept| assert_eq!((kept.step, kept.lists.len()), (2_001, 1)));
    }
}
