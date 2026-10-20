//! The server's links that refuse a Safe to everyone but its owner and the
//! players they allowed (who never dismantle it): the Open
//! offer, item moves and sorting, base stock for crafting and building,
//! dismantling, and the upgrade and Quick Stash switches. Each refusal takes
//! the game's own path for a refused action, so nothing moves and nothing is
//! lost.
//!
//! On a client that hosts a game, the cursor's dismantle check and base stock
//! also serve the client's own world, whose entities are not the server's:
//! there the hooks keep out of the way.

use std::cell::RefCell;
use std::sync::OnceLock;

use enshwick_api::ecs::Entity;
use enshwick_api::{Load, Next, Off};

use crate::access::{ACCESS, Action, Reads};

/// Offsets into the game's action and cursor data, from the manifest.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    pub(crate) state_input: usize,
    pub(crate) state_owner_index: usize,
    pub(crate) transfer_source: usize,
    pub(crate) transfer_target: usize,
    pub(crate) transfer_source_slot: usize,
    pub(crate) transfer_target_slot: usize,
    pub(crate) sort_target: usize,
    pub(crate) object_id: usize,
    pub(crate) object_kind: usize,
    pub(crate) deposit_target: usize,
    pub(crate) deposit_version: usize,
    pub(crate) consumed_deposit: usize,
    pub(crate) upgrade_target: usize,
    pub(crate) upgrade_version: usize,
    pub(crate) consumed_upgrade: usize,
    /// Where base stock returns to in the client's interface; 0 for none.
    pub(crate) stock_ui_return: u64,
    pub(crate) player_input: u32,
    pub(crate) consumed_input: u32,
}

static READS: OnceLock<(Reads, Layout)> = OnceLock::new();

/// The offer result the game writes when it does not offer an interaction.
const NOT_OFFERED: u8 = 6;
/// Copies of the base stock per thread; callers use one result at a time.
const RING: usize = 4;

/// Reads `T` at `base + offset`.
///
/// # Safety
///
/// A readable `T` lies there.
unsafe fn read<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

/// Writes `value` at `base + offset`.
///
/// # Safety
///
/// A writable `T` lies there.
unsafe fn write<T: Copy>(base: *mut u8, offset: usize, value: T) {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().write_unaligned(value) };
}

/// Writes the game's own result for an interaction it does not offer.
///
/// # Safety
///
/// `out` is the offer test's 0x14-byte out record.
unsafe fn not_offered(out: *mut u8) {
    // SAFETY: forwarded from the caller.
    unsafe { write(out, 0, NOT_OFFERED) };
    // SAFETY: as above.
    unsafe { write(out, 4, 0u32) };
    // SAFETY: as above.
    unsafe { write(out, 8, 0u64) };
    // SAFETY: as above.
    unsafe { write(out, 0x10, 0u32) };
}

enshwick_api::detour! {
    static OFFER: unsafe extern "system" fn(
        out: *mut u8, query: *mut u8, hits: *mut u8, candidate: u32,
        shape: *mut u8, transform: *mut u8, permissions: u8, range: u32,
    ) -> *mut u8 = offer;
}

type OfferFn =
    unsafe extern "system" fn(*mut u8, *mut u8, *mut u8, u32, *mut u8, *mut u8, u8, u32) -> *mut u8;

#[expect(clippy::too_many_arguments, reason = "the game function's signature")]
fn offer(
    next: Next<OfferFn>,
    out: *mut u8,
    query: *mut u8,
    hits: *mut u8,
    candidate: u32,
    shape: *mut u8,
    transform: *mut u8,
    permissions: u8,
    range: u32,
) -> *mut u8 {
    if let Some((reads, _)) = READS.get()
        && !out.is_null()
        && !query.is_null()
    {
        // SAFETY: the query state starts with the system context.
        let ctx: *mut u64 = unsafe { read(query, 0) };
        // SAFETY: the offer test runs inside its system, on its thread.
        let player = unsafe { reads.ecs.entity(ctx) };
        // SAFETY: as above.
        if !unsafe { ACCESS.allowed(reads, ctx, Entity(candidate), player, Action::Use) } {
            // SAFETY: the out record is 0x14 bytes.
            unsafe { not_offered(out) };
            return out;
        }
    }
    // SAFETY: forwarded unchanged.
    unsafe {
        next.get()(
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

enshwick_api::detour! {
    static TRANSFER: unsafe extern "system" fn(ctx: *mut u64, state: *mut u8) = transfer;
}
enshwick_api::detour! {
    static SORT: unsafe extern "system" fn(ctx: *mut u64, state: *mut u8) = sort;
}

type ActionFn = unsafe extern "system" fn(*mut u64, *mut u8);

/// Whether the player acting in `ctx` may touch every inventory in `chests`
/// and the roots of `slots`.
fn may_touch(ctx: *mut u64, state: *mut u8, chests: &[u32], slots: &[u64]) -> bool {
    let Some((reads, layout)) = READS.get() else {
        return true;
    };
    // SAFETY: the action handler runs inside its system, on its thread.
    let player = unsafe { reads.ecs.entity(ctx) };
    // SAFETY: the action state holds the system's OwnerRelationship index.
    let owner_index: u64 = unsafe { read(state, layout.state_owner_index) };
    let roots = slots.iter().filter(|&&slot| slot != 0).filter_map(|&slot| {
        // SAFETY: as above.
        unsafe { reads.components.slot_root(ctx, slot, owner_index) }
    });
    chests
        .iter()
        .map(|&chest| Entity(chest))
        .chain(roots)
        // SAFETY: as above.
        .all(|chest| unsafe { ACCESS.allowed(reads, ctx, chest, player, Action::Use) })
}

fn transfer(next: Next<ActionFn>, ctx: *mut u64, state: *mut u8) {
    if let Some((_, layout)) = READS.get()
        && !state.is_null()
    {
        // SAFETY: the action state points at the player's input.
        let input: *const u8 = unsafe { read(state, layout.state_input) };
        if !input.is_null() {
            // SAFETY: the input holds the transfer action at these offsets.
            let source: u32 = unsafe { read(input, layout.transfer_source) };
            // SAFETY: as above.
            let target: u32 = unsafe { read(input, layout.transfer_target) };
            // SAFETY: as above.
            let source_slot: u64 = unsafe { read(input, layout.transfer_source_slot) };
            // SAFETY: as above.
            let target_slot: u64 = unsafe { read(input, layout.transfer_target_slot) };
            if !may_touch(ctx, state, &[source, target], &[source_slot, target_slot]) {
                // Skipped: the caller marks the action consumed and resends the
                // inventories, undoing the client's move.
                return;
            }
        }
    }
    // SAFETY: forwarded unchanged.
    unsafe { next.get()(ctx, state) }
}

fn sort(next: Next<ActionFn>, ctx: *mut u64, state: *mut u8) {
    if let Some((_, layout)) = READS.get()
        && !state.is_null()
    {
        // SAFETY: as for the transfer.
        let input: *const u8 = unsafe { read(state, layout.state_input) };
        if !input.is_null() {
            // SAFETY: the input holds the sort action's target there.
            let target: u32 = unsafe { read(input, layout.sort_target) };
            if !may_touch(ctx, state, &[target], &[]) {
                return;
            }
        }
    }
    // SAFETY: forwarded unchanged.
    unsafe { next.get()(ctx, state) }
}

enshwick_api::detour! {
    static BASE_STOCK: unsafe extern "system" fn(
        out: *mut u8, lists: *mut [u64; 4], server: *mut u8, client: *mut u8, position: *const f32, player: u32,
    ) -> *mut u8 = base_stock;
}
enshwick_api::detour! {
    static BASE_STOCK_FRAMED: unsafe extern "system" fn(
        out: *mut u8, lists: *mut [u64; 4], server: *mut u8, client: *mut u8, position: *const f32, player: u32,
    ) -> *mut u8 = base_stock, flags = enshwick_api::abi::HookFlags::CALLER_FRAME;
}

/// Whether base stock was asked for by the client's own interface, which
/// sees the client's world; read before the chain calls on.
fn for_client_ui(layout: &Layout) -> bool {
    if layout.stock_ui_return == 0 {
        return false;
    }
    enshwick_api::host().is_some_and(|host| {
        // SAFETY: the frame was taken at entry of this chain.
        let caller = unsafe { host.caller_frame().return_address() };
        caller == layout.stock_ui_return
    })
}

type StockFn =
    unsafe extern "system" fn(*mut u8, *mut [u64; 4], *mut u8, *mut u8, *const f32, u32) -> *mut u8;

thread_local! {
    /// Filtered copies of the base stock lists, reused in turn.
    static STOCK: RefCell<(usize, [[Vec<u32>; 2]; RING])> = RefCell::new((0, Default::default()));
}

fn base_stock(
    next: Next<StockFn>,
    out: *mut u8,
    lists: *mut [u64; 4],
    server: *mut u8,
    client: *mut u8,
    position: *const f32,
    player: u32,
) -> *mut u8 {
    let ui = READS.get().is_some_and(|(_, layout)| for_client_ui(layout));
    // SAFETY: forwarded unchanged.
    let result = unsafe { next.get()(out, lists, server, client, position, player) };
    if ui {
        return result;
    }
    // SAFETY: the first byte is the game's error code, 0 for success.
    if out.is_null() || lists.is_null() || unsafe { out.read() } != 0 {
        return result;
    }
    // SAFETY: the game filled the lists: stock ids, count, fuel ids, count.
    let mut found = unsafe { lists.read() };
    let player = Entity(player);
    let list = |ptr: u64, count: u64| -> &[u32] {
        let ptr = core::ptr::with_exposed_provenance::<u32>(usize::try_from(ptr).unwrap_or(0));
        if ptr.is_null() {
            return &[];
        }
        // SAFETY: the base's arrays hold `count` ids, valid for this call.
        unsafe { core::slice::from_raw_parts(ptr, usize::try_from(count).unwrap_or(0)) }
    };
    let (stock, fuel) = (list(found[0], found[1]), list(found[2], found[3]));
    if !stock
        .iter()
        .chain(fuel)
        .any(|&id| ACCESS.refused_seen(id, player))
    {
        return result;
    }
    // Never filter in place: every player reads the same arrays.
    STOCK.with_borrow_mut(|(turn, ring)| {
        let copies = &mut ring[*turn % RING];
        *turn = turn.wrapping_add(1);
        for (copy, (source, at)) in copies.iter_mut().zip([(stock, 0), (fuel, 2)]) {
            copy.clear();
            copy.extend(
                source
                    .iter()
                    .copied()
                    .filter(|&id| !ACCESS.refused_seen(id, player)),
            );
            found[at] = copy.as_ptr().expose_provenance() as u64;
            found[at + 1] = copy.len() as u64;
        }
    });
    // SAFETY: the game's lists, rewritten to the filtered copies.
    unsafe { lists.write(found) };
    result
}

enshwick_api::detour! {
    static DISMANTLE: unsafe extern "system" fn(ctx: *mut u8, object: *mut u8, held: *mut u8, permissions: u8) -> u64 = dismantle;
}

type DismantleFn = unsafe extern "system" fn(*mut u8, *mut u8, *mut u8, u8) -> u64;

fn dismantle(
    next: Next<DismantleFn>,
    ctx: *mut u8,
    object: *mut u8,
    held: *mut u8,
    permissions: u8,
) -> u64 {
    // SAFETY: forwarded unchanged.
    let method = unsafe { next.get()(ctx, object, held, permissions) };
    let Some((reads, layout)) = READS.get() else {
        return method;
    };
    if method.trailing_zeros() >= 8 || ctx.is_null() || object.is_null() {
        return method;
    }
    // SAFETY: the object describes an entity when its kind word is 0.
    let id: u64 = unsafe { read(object, layout.object_id) };
    // SAFETY: as above.
    let kind: u64 = unsafe { read(object, layout.object_kind) };
    if kind != 0 {
        return method;
    }
    // SAFETY: the cursor context starts with the system context.
    let system: *mut u64 = unsafe { read(ctx, 0) };
    // SAFETY: the cursor runs inside its system, on its thread.
    if !unsafe { reads.worlds.is_server(system) } {
        return method;
    }
    // SAFETY: the cursor runs inside its system, on its thread.
    let player = unsafe { reads.ecs.entity(system) };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the entity id is the low half"
    )]
    let chest = Entity(id as u32);
    // SAFETY: as above.
    if unsafe { ACCESS.allowed(reads, system, chest, player, Action::Remove) } {
        method
    } else {
        0
    }
}

enshwick_api::detour! {
    static INVENTORY_ACTIONS: unsafe extern "system" fn(ctx: *mut u64) -> u64 = inventory_actions;
}

type InventoryActionsFn = unsafe extern "system" fn(*mut u64) -> u64;

/// Before the inventory system runs, marks as done the Quick Stash switch on
/// someone else's Safe and any upgrade of a Safe, which would replace it with
/// an ordinary chest; the system then skips them as already handled.
fn inventory_actions(next: Next<InventoryActionsFn>, ctx: *mut u64) -> u64 {
    if let Some((reads, layout)) = READS.get() {
        for &player in ACCESS.players.load().keys() {
            let player = Entity(player);
            // SAFETY: the system's context, at its entry on its thread.
            let input = unsafe { reads.components.component(ctx, player, layout.player_input) };
            // SAFETY: as above.
            let consumed = unsafe {
                reads
                    .components
                    .component(ctx, player, layout.consumed_input)
            };
            let (Some(input), Some(consumed)) = (input, consumed) else {
                continue;
            };
            // SAFETY: the player's input and its server-side consumed record.
            unsafe { refuse_pending(reads, layout, ctx, player, input, consumed) };
        }
    }
    // SAFETY: forwarded unchanged.
    unsafe { next.get()(ctx) }
}

/// # Safety
///
/// `input` and `consumed` are `player`'s live components inside the system.
unsafe fn refuse_pending(
    reads: &Reads,
    layout: &Layout,
    ctx: *mut u64,
    player: Entity,
    input: *mut u8,
    consumed: *mut u8,
) {
    // SAFETY: the caller's components, read at the actions' offsets.
    let deposit_version: u32 = unsafe { read(input, layout.deposit_version) };
    // SAFETY: as above.
    let deposit_done: u32 = unsafe { read(consumed, layout.consumed_deposit) };
    // SAFETY: as above.
    let deposit_target: u32 = unsafe { read(input, layout.deposit_target) };
    if deposit_version != deposit_done {
        // SAFETY: inside the system, on its thread.
        let allowed =
            unsafe { ACCESS.allowed(reads, ctx, Entity(deposit_target), player, Action::Use) };
        if !allowed {
            // SAFETY: the consumed record is server-only; no client sees it change.
            unsafe { write(consumed, layout.consumed_deposit, deposit_version) };
        }
    }
    // SAFETY: as above.
    let upgrade_version: u32 = unsafe { read(input, layout.upgrade_version) };
    // SAFETY: as above.
    let upgrade_done: u32 = unsafe { read(consumed, layout.consumed_upgrade) };
    // SAFETY: as above.
    let upgrade_target: u32 = unsafe { read(input, layout.upgrade_target) };
    if upgrade_version != upgrade_done {
        // SAFETY: inside the system, on its thread.
        let safe = unsafe { ACCESS.owner(reads, ctx, Entity(upgrade_target)) }.is_some();
        if safe {
            // SAFETY: as above.
            unsafe { write(consumed, layout.consumed_upgrade, upgrade_version) };
        }
    }
}

/// Links every refusal.
///
/// # Errors
///
/// [`Off`] when a function or offset is missing on this build.
pub(crate) fn link(load: &mut Load<'_>, reads: Reads, layout: Layout) -> Result<(), Off> {
    if READS.set((reads, layout)).is_err() {
        return Err(Off::new("the Safe's checks are linked once"));
    }
    load.hook(&OFFER, load.symbol("safes.offer_test")?)?;
    load.hook(&TRANSFER, load.symbol("safes.transfer")?)?;
    load.hook(&SORT, load.symbol("safes.sort")?)?;
    let stock = load.symbol("safes.base_stock")?;
    if layout.stock_ui_return == 0 {
        load.hook(&BASE_STOCK, stock)?;
    } else {
        load.hook(&BASE_STOCK_FRAMED, stock)?;
    }
    load.hook(&DISMANTLE, load.symbol("safes.dismantle_method")?)?;
    load.hook(&INVENTORY_ACTIONS, load.symbol("safes.inventory_actions")?)?;
    Ok(())
}
