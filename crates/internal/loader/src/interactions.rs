//! Players' interactions with entities, asked of the mods' filters before
//! the game allows them. A refusal takes the game's own path for a refused
//! action, so nothing moves and nothing is lost; the filters see the server's
//! world only.

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{Header, HookFlags, Interaction, InteractionFn, Side, Status};

use arc_swap::ArcSwapOption;

use crate::chains::{LOADER, Link};
use crate::ecs::{Components, Engine};
use crate::host::Loader;

/// The offer result the game writes when it does not offer an interaction.
const NOT_OFFERED: u8 = 6;
const RING: usize = 4;
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

#[derive(Debug, Clone, Copy)]
struct Filter {
    owner: usize,
    callback: InteractionFn,
    user: usize,
}

static FILTERS: Mutex<Vec<Filter>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Box<[Filter]>> = OnceLock::new();
static LOADED: OnceLock<(&'static Loader, Layout, Components)> = OnceLock::new();
/// The players' character entities, read on the game thread now and then.
static PLAYERS: ArcSwapOption<Vec<u32>> = ArcSwapOption::const_empty();

static OFFER_NEXT: AtomicUsize = AtomicUsize::new(0);
static TRANSFER_NEXT: AtomicUsize = AtomicUsize::new(0);
static SORT_NEXT: AtomicUsize = AtomicUsize::new(0);
static STOCK_NEXT: AtomicUsize = AtomicUsize::new(0);
static DISMANTLE_NEXT: AtomicUsize = AtomicUsize::new(0);
static ACTIONS_NEXT: AtomicUsize = AtomicUsize::new(0);

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
        .push(Filter {
            owner,
            callback,
            user: user.expose_provenance(),
        });
    Status::OK
}

pub(crate) fn link(loader: &'static Loader) {
    let filters = std::mem::take(&mut *FILTERS.lock().unwrap_or_else(PoisonError::into_inner));
    let wanted = !filters.is_empty();
    let _first = SEALED.set(filters.into_boxed_slice());
    if !wanted {
        return;
    }
    let (Some(layout), Some(components)) = (layout(loader), Components::resolve(loader)) else {
        return;
    };
    let _first = LOADED.set((loader, layout, components));
    let at = |name: &str| {
        loader
            .build
            .symbol(&format!("access.{name}"))
            .map_or(0, |rva| loader.base + rva)
    };
    let actions = loader
        .reflection
        .system("inventory_actions")
        .map_or(0, enshwick_loader_reflect::System::entry);
    let stock_flags = if layout.stock_ui_return == 0 {
        HookFlags::NONE
    } else {
        HookFlags::CALLER_FRAME
    };
    let links: [(&str, u64, *const (), &'static AtomicUsize, HookFlags); 6] = [
        (
            "offer",
            at("offer_test"),
            offer as OfferFn as *const (),
            &OFFER_NEXT,
            HookFlags::NONE,
        ),
        (
            "transfer",
            at("transfer"),
            transfer as ActionFn as *const (),
            &TRANSFER_NEXT,
            HookFlags::NONE,
        ),
        (
            "sort",
            at("sort"),
            sort as ActionFn as *const (),
            &SORT_NEXT,
            HookFlags::NONE,
        ),
        (
            "base stock",
            at("base_stock"),
            stock as StockFn as *const (),
            &STOCK_NEXT,
            stock_flags,
        ),
        (
            "dismantle",
            at("dismantle_method"),
            dismantle as DismantleFn as *const (),
            &DISMANTLE_NEXT,
            HookFlags::NONE,
        ),
        (
            "inventory actions",
            actions,
            inventory_actions as ActionsFn as *const (),
            &ACTIONS_NEXT,
            HookFlags::NONE,
        ),
    ];
    for (name, target, detour, next, flags) in links {
        if target == 0 {
            crate::log!(WARN, "interactions: {name} is not in this build");
            continue;
        }
        let link = Link {
            owner: LOADER,
            detour: detour.addr(),
            next,
            priority: i32::MIN,
            flags,
        };
        let status = loader
            .chains
            .lock()
            .map_or(Status::REFUSED, |mut chains| chains.add(target, link));
        if !status.is_ok() {
            crate::log!(WARN, "interactions: {name} refused ({})", status.0);
        }
    }
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

fn next<F: Copy>(slot: &AtomicUsize) -> F {
    let address = slot.load(Ordering::Acquire);
    // SAFETY: the loader points the slot at the rest of the chain, which has
    // the hooked function's signature, before the link can run.
    unsafe { core::mem::transmute_copy::<usize, F>(&address) }
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
    filters
        .iter()
        .filter(|f| !crate::host::is_off(f.owner))
        .all(|f| {
            let user = core::ptr::with_exposed_provenance_mut(f.user);
            // SAFETY: the callback lives in the mod's DLL, which stays loaded,
            // and takes the pointer it registered; the call is valid for it.
            unsafe { (f.callback)(call, user) != 0 }
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
        && std::panic::catch_unwind(|| {
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
        next::<OfferFn>(&OFFER_NEXT)(
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
            && std::panic::catch_unwind(|| {
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
    unsafe { next::<ActionFn>(&TRANSFER_NEXT)(ctx, state) }
}

unsafe extern "system" fn sort(ctx: *mut u64, state: *mut u8) {
    let refused = LOADED.get().is_some_and(|(_, layout, _)| {
        !state.is_null()
            && std::panic::catch_unwind(|| {
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
    unsafe { next::<ActionFn>(&SORT_NEXT)(ctx, state) }
}

thread_local! {
    /// Filtered copies of the base stock lists, reused in turn.
    static STOCK: RefCell<(usize, [[Vec<u32>; 2]; RING])> = RefCell::new((0, Default::default()));
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
    let result =
        unsafe { next::<StockFn>(&STOCK_NEXT)(out, lists, server, client, position, player) };
    // SAFETY: the first byte is the game's error code, 0 for success.
    if ui || out.is_null() || lists.is_null() || unsafe { out.read() } != 0 {
        return result;
    }
    // SAFETY: the game's lists, filled for this call.
    let _done = std::panic::catch_unwind(|| unsafe { filter_stock(lists, player) });
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
    STOCK.with_borrow_mut(|(turn, ring)| {
        let copies = &mut ring[*turn % RING];
        *turn = turn.wrapping_add(1);
        let mut index = 0;
        for (copy, (source, at)) in copies.iter_mut().zip([(stock, 0), (fuel, 2)]) {
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
            found[at] = copy.as_ptr().expose_provenance() as u64;
            found[at + 1] = copy.len() as u64;
        }
    });
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
    let method = unsafe { next::<DismantleFn>(&DISMANTLE_NEXT)(ctx, object, held, permissions) };
    if method.trailing_zeros() >= 8 || ctx.is_null() || object.is_null() {
        return method;
    }
    let Some((_, layout, _)) = LOADED.get() else {
        return method;
    };
    let refused = std::panic::catch_unwind(|| {
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
        let _done = std::panic::catch_unwind(|| unsafe { refuse_pending(ctx) });
    }
    // SAFETY: forwarded unchanged.
    unsafe { next::<ActionsFn>(&ACTIONS_NEXT)(ctx) }
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
