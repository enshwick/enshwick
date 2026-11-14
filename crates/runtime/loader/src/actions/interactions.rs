//! Players' interactions with entities, asked of the mods' filters before
//! the game allows them. A refusal takes the game's own path for a refused
//! action, so nothing moves and nothing is lost; the filters see the server's
//! world only.

use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwapOption;
use enshwick_core::abi::{Header, HookFlags, Interaction, InteractionFn, Side, Status};
use enshwick_core::raw::{read_at, read_ptr_at, write_at};

use super::UNREFLECTED;
use crate::engine::builds::ABSENT;
use crate::engine::ecs::{Components, Engine};
use crate::engine::fuse::{Fuse, Off};
use crate::engine::loader::{Loader, Registered, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned::{self, Constant, Symbol};
use crate::engine::registry::Registry;

mod stock;

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
    pinned::access::OFFER_OUT_TAIL.id(),
    pinned::access::OFFER_OUT_U32.id(),
    pinned::access::OFFER_OUT_U64.id(),
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
    pinned::ecs::COMPONENT_READ_RAW.id(),
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
    /// The words of the offer test's out record a refused offer zeroes
    /// after its result: a `u32`, a `u64` and the last `u32`.
    offer_out: [usize; 3],
    /// Where base stock returns to in the client's interface; 0 for none.
    stock_ui_return: u64,
    used_item: u32,
    transform: u32,
    player_input: u32,
    consumed_input: u32,
}

type Filter = Registered<InteractionFn>;

static FILTERS: Registry<Filter> = Registry::new();
/// The layout and components the links read, once linked.
static LINKED: OnceLock<(Layout, Components)> = OnceLock::new();

/// The loader with what the links read, once linked.
fn linked() -> Option<(&'static Loader, &'static Layout, &'static Components)> {
    let (layout, components) = LINKED.get()?;
    Some((crate::engine::loader::loader()?, layout, components))
}
/// The players' character entities, read on the game thread now and then.
static PLAYERS: ArcSwapOption<Vec<u32>> = ArcSwapOption::const_empty();

/// The switch of the interaction filters and of every link they make: a
/// link this build cannot make turns them all off, and so does a panic in
/// their work.
static INTERACTIONS_FUSE: Fuse = Fuse::new("interaction filters");
/// The switch of the link on [`ACTIONS_SYSTEM`], which the interaction
/// filters, the item uses and the controls that follow game actions share;
/// each runs under its own fuse inside it, so a panic in one leaves the
/// others linked.
static ACTIONS_FUSE: Fuse = Fuse::new("inventory actions");
/// The report that props stay open to players while transactions hold them,
/// when a link the hold relies on is off.
static HOLDS_FUSE: Fuse = Fuse::new("prop removal by transactions");

/// A table entry or reflected type the interactions lack, and why.
type Lacks = (&'static str, &'static str);

/// Where the layout comes from: the table entry or reflected type it lacks.
fn layout(loader: &Loader) -> Result<Layout, Lacks> {
    let size = |id: Constant| loader.offset(id).ok_or((id.id(), ABSENT));
    let hash = |name: &'static str| {
        let unreflected = (name, UNREFLECTED);
        let descriptor = loader
            .reflection
            .type_info(name)
            .map_err(|_| unreflected)?
            .va();
        crate::engine::world::type_hash(descriptor).ok_or(unreflected)
    };
    let stock_ui_return = if loader.side == Side::CLIENT {
        let id = pinned::access::BASE_STOCK_UI_RETURN;
        loader.address(id).ok_or((id.id(), ABSENT))?
    } else {
        0
    };
    Ok(Layout {
        state_input: size(pinned::access::STATE_INPUT)?,
        state_owner_index: size(pinned::access::STATE_OWNER_INDEX)?,
        transfer_source: size(pinned::access::TRANSFER_SOURCE)?,
        transfer_target: size(pinned::access::TRANSFER_TARGET)?,
        transfer_source_slot: size(pinned::access::TRANSFER_SOURCE_SLOT)?,
        transfer_target_slot: size(pinned::access::TRANSFER_TARGET_SLOT)?,
        sort_target: size(pinned::access::SORT_TARGET)?,
        object_id: size(pinned::access::OBJECT_ID)?,
        object_kind: size(pinned::access::OBJECT_KIND)?,
        deposit_target: size(pinned::access::DEPOSIT_TARGET)?,
        deposit_version: size(pinned::access::DEPOSIT_VERSION)?,
        consumed_deposit: size(pinned::access::CONSUMED_DEPOSIT)?,
        upgrade_target: size(pinned::access::UPGRADE_TARGET)?,
        upgrade_version: size(pinned::access::UPGRADE_VERSION)?,
        consumed_upgrade: size(pinned::access::CONSUMED_UPGRADE)?,
        offer_out: [
            size(pinned::access::OFFER_OUT_U32)?,
            size(pinned::access::OFFER_OUT_U64)?,
            size(pinned::access::OFFER_OUT_TAIL)?,
        ],
        stock_ui_return,
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
    if layout(loader).is_err()
        || Components::resolve(loader).is_none()
        || crate::engine::ecs::engine(loader).is_none()
    {
        return Status::MISSING;
    }
    FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// The system that runs players' inventory actions.
pub(crate) const ACTIONS_SYSTEM: &str = "inventory_actions";
/// Whether the link on [`ACTIONS_SYSTEM`] is made, which reads the players'
/// input for the controls that follow game actions.
static READS_INPUTS: AtomicBool = AtomicBool::new(false);

/// Whether the players' input is read each step, for the controls that
/// follow game actions: the link is made and still on.
pub(crate) fn reads_inputs() -> bool {
    still_linked(READS_INPUTS.load(Ordering::Acquire), &ACTIONS_FUSE)
}

/// Whether a link `made` at link time still runs: its `fuse` did not turn
/// off since.
fn still_linked(made: bool, fuse: &Fuse) -> bool {
    made && !fuse.is_off()
}

/// Why the props stay open while transactions hold them: the link `off`,
/// which the hold relies on, is off. Neither the build's fault nor a
/// panic of the holds, so no error code.
fn holds_off(off: &Fuse) -> Off {
    let why = format!("{} is off", off.feature());
    Off::reported(format!("{} off: {why}", HOLDS_FUSE.feature()), why)
}

pub(crate) fn link(loader: &'static Loader) {
    let filters = FILTERS.seal();
    // Linked on every side, as any can host a world whose transactions hold
    // props; the detours act only in a server's world.
    let found = layout(loader).and_then(|layout| {
        let components = Components::found(loader).map_err(|id| (id, ABSENT))?;
        Ok((layout, components))
    });
    let (filtered, actions) = match found {
        Ok((layout, components)) => {
            let _first = LINKED.set((layout, components));
            let stock_flags = if layout.stock_ui_return == 0 {
                HookFlags::NONE
            } else {
                HookFlags::CALLER_FRAME
            };
            let entry = loader
                .reflection
                .system(ACTIONS_SYSTEM)
                .ok()
                .map(enshwick_gamedata::reflect::System::entry);
            (
                link_filters(loader, Target::Pinned, stock_flags),
                loader.link_fused(
                    &ACTIONS_FUSE,
                    Target::Found(ACTIONS_SYSTEM, entry),
                    &ACTIONS_NEXT,
                    inventory_actions,
                    HookFlags::NONE,
                ),
            )
        }
        Err((id, reason)) => {
            for fuse in [&INTERACTIONS_FUSE, &ACTIONS_FUSE] {
                let _off = loader.fuse_off(fuse, id, reason);
            }
            (false, false)
        }
    };
    READS_INPUTS.store(actions, Ordering::Release);
    let hold_ready = filtered && actions;
    if !hold_ready {
        let off = if filtered {
            &ACTIONS_FUSE
        } else {
            &INTERACTIONS_FUSE
        };
        HOLDS_FUSE.turn_off(&holds_off(off));
    }
    crate::world::transactions::holds().linked(filters.len(), hold_ready);
    if filtered {
        crate::log!(INFO, "interaction filters on");
    }
}

/// Links every hook the filters screen under [`INTERACTIONS_FUSE`], each
/// game function where `at` finds it: one that does not link turns the
/// filters wholly off. True when all linked.
fn link_filters(
    loader: &Loader,
    at: impl Fn(Symbol) -> Target<'static>,
    stock_flags: HookFlags,
) -> bool {
    let fuse = &INTERACTIONS_FUSE;
    let none = HookFlags::NONE;
    loader.link_fused(
        fuse,
        at(pinned::access::OFFER_TEST),
        &OFFER_NEXT,
        offer,
        none,
    ) && loader.link_fused(
        fuse,
        at(pinned::access::TRANSFER),
        &TRANSFER_NEXT,
        transfer,
        none,
    ) && loader.link_fused(fuse, at(pinned::access::SORT), &SORT_NEXT, sort, none)
        && stock::link(loader, at(pinned::access::BASE_STOCK), stock_flags)
        && loader.link_fused(
            fuse,
            at(pinned::access::DISMANTLE_METHOD),
            &DISMANTLE_NEXT,
            dismantle,
            none,
        )
}

/// Whether a hold would keep no one out: the filters or the link that
/// refuses pending Quick Stash switches is off.
pub(crate) fn filter_off() -> bool {
    INTERACTIONS_FUSE.is_off() || ACTIONS_FUSE.is_off()
}

pub(crate) fn after_step(loader: &Loader, step: u64) {
    crate::world::transactions::holds().after_step(step);
    if read_due(step, PLAYERS.load().is_some()) {
        read_players(loader);
    }
}

/// Reads the players again once one reached the game, so the screens hold
/// for them from the next step; on the game thread after the step.
pub(crate) fn player_joined(loader: &Loader) {
    read_players(loader);
}

/// Whether the players are read after `step`: every [`PLAYERS_EVERY`]
/// steps, and on each step while no list is `listed`, as after a world loads.
const fn read_due(step: u64, listed: bool) -> bool {
    !listed || step.is_multiple_of(PLAYERS_EVERY)
}

fn read_players(loader: &Loader) {
    if LINKED.get().is_none() && !crate::inventory::item_actions::wanted() {
        return;
    }
    if let Ok(entities) = crate::engine::world::player_entities(loader)
        && PLAYERS.load().as_deref().map(Vec::as_slice) != Some(entities.as_slice())
    {
        PLAYERS.store(Some(std::sync::Arc::new(entities.to_vec())));
    }
}

/// The players' character entities as last read on the game thread, for
/// the systems that act on each player's input.
pub(crate) fn players() -> arc_swap::Guard<Option<std::sync::Arc<Vec<u32>>>> {
    PLAYERS.load()
}

pub(crate) fn forget() {
    PLAYERS.store(None);
}

fn allowed(call: &Interaction) -> bool {
    if crate::world::transactions::holds().is_held(call.target.0) {
        return false;
    }
    let Some(filters) = FILTERS.sealed() else {
        return true;
    };
    filters.iter().filter(|f| f.live()).all(|f| {
        // SAFETY: `f` is live, so the mod's DLL that holds the callback stays
        // loaded; the callback gets the user pointer its mod registered and
        // `call`, which lives for the call.
        crate::engine::profile::call(
            f.owner,
            crate::engine::profile::Feature::Interactions,
            || unsafe { (f.callback)(call, f.user()) != 0 },
        )
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
        player: enshwick_core::Entity(player),
        target: enshwick_core::Entity(target),
        item: enshwick_core::ItemId::new(0),
        known: 0,
        reserved: [0; 3],
        position: [0.0; 3],
    };
    if let Some((_, layout, components)) = linked()
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
            call.position = enshwick_core::ecs::metres(fixed);
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
        || unsafe { engine.is_world(ctx, crate::engine::world::server_world()) }
}

/// The acting player of `ctx` when it is a server context.
///
/// # Safety
///
/// - `ctx` is null or the live context of a running system, used on its
///   thread.
unsafe fn server_player(ctx: *mut u64) -> Option<u32> {
    let (loader, _, _) = linked()?;
    let engine = crate::engine::ecs::resolved()?;
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

crate::engine::chains::chain_detour! {
    /// The table's `access.offer_test`: an interaction a filter refuses is
    /// not offered, as the game's own result for no offer says.
    fn offer(
        out: *mut u8,
        query: *mut u8,
        hits: *mut u8,
        candidate: u32,
        shape: *mut u8,
        transform: *mut u8,
        permissions: u8,
        range: u32,
    ) -> *mut u8;
    next OFFER_NEXT;
    fuse &INTERACTIONS_FUSE, "the offer";
    |onward| {
        if crate::world::transactions::holds().quiet() || out.is_null() || query.is_null() {
            return onward.call();
        }
        let Some((_, layout, _)) = linked() else {
            return onward.call();
        };
        // SAFETY: `query` is non-null by the check before it and is the
        // offer test's query state, which starts with its system context
        // pointer.
        let ctx = unsafe { read_ptr_at::<u64>(query, 0) };
        // SAFETY: `ctx` is null or the system context of the running offer
        // test, which runs inside its system on its thread.
        let Some(player) = (unsafe { server_player(ctx) }) else {
            return onward.call();
        };
        // SAFETY: `ctx` is the system context of the running offer test, on
        // its system's thread.
        if allowed(&unsafe { interaction(Interaction::OPEN, player, candidate, ctx) }) {
            return onward.call();
        }
        let [word, wide, tail] = layout.offer_out;
        // SAFETY: `out` is non-null by the check before it and is the offer
        // test's out record, which starts with its u8 result.
        unsafe { write_at(out, 0, NOT_OFFERED) };
        // SAFETY: `out` is the offer test's out record, whose u32 the game
        // zeroes for no offer sits at the build table's
        // `access.offer_out_u32`.
        unsafe { write_at(out, word, 0u32) };
        // SAFETY: `out` is the offer test's out record, whose u64 the game
        // zeroes for no offer sits at the build table's
        // `access.offer_out_u64`.
        unsafe { write_at(out, wide, 0u64) };
        // SAFETY: `out` is the offer test's out record, whose last u32 the
        // game zeroes for no offer sits at the build table's
        // `access.offer_out_tail`.
        unsafe { write_at(out, tail, 0u32) };
        out
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
    let Some((_, layout, components)) = linked() else {
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

crate::engine::chains::chain_detour! {
    /// The table's `access.transfer`: a move a filter refuses is skipped,
    /// and the caller marks the action consumed and resends the
    /// inventories, undoing the client's move.
    fn transfer(ctx: *mut u64, state: *mut u8) -> ();
    next TRANSFER_NEXT;
    fuse &INTERACTIONS_FUSE, "the transfer";
    |onward| {
        // SAFETY: `ctx` and `state` are the running transfer's own
        // arguments, on its system's thread.
        if !unsafe { transfer_refused(ctx, state) } {
            onward.call();
        }
    }
}

/// Whether a filter refuses the transfer of `state`, run in `ctx`.
///
/// # Safety
///
/// - `ctx` and `state` are the arguments of a running `access.transfer`,
///   used on its system's thread: `ctx` null or its live system context,
///   `state` null or its live action state.
unsafe fn transfer_refused(ctx: *mut u64, state: *mut u8) -> bool {
    if crate::world::transactions::holds().quiet() || state.is_null() {
        return false;
    }
    let Some((_, layout, _)) = linked() else {
        return false;
    };
    // SAFETY: `state` is non-null by the check before it and is the
    // transfer's action state; its pointer to the player's input sits at
    // the build table's `access.state_input` offset.
    let input = unsafe { read_ptr_at::<u8>(state, layout.state_input) }.cast_const();
    if input.is_null() {
        return false;
    }
    // SAFETY: `input` is the player's non-null input by the check before
    // it; its u32 source sits at the build table's `access.transfer_source`
    // offset.
    let source: u32 = unsafe { read_at(input, layout.transfer_source) };
    // SAFETY: `input` is the player's non-null input; its u32 target sits
    // at the build table's `access.transfer_target` offset.
    let target: u32 = unsafe { read_at(input, layout.transfer_target) };
    // SAFETY: `input` is the player's non-null input; its u64 source slot
    // sits at the build table's `access.transfer_source_slot` offset.
    let source_slot: u64 = unsafe { read_at(input, layout.transfer_source_slot) };
    // SAFETY: `input` is the player's non-null input; its u64 target slot
    // sits at the build table's `access.transfer_target_slot` offset.
    let target_slot: u64 = unsafe { read_at(input, layout.transfer_target_slot) };
    // SAFETY: the caller's `ctx` and non-null `state`, on the system's
    // thread.
    !unsafe {
        may_touch(
            Interaction::MOVE,
            ctx,
            state,
            &[source, target],
            &[source_slot, target_slot],
        )
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `access.sort`: a sort a filter refuses is skipped.
    fn sort(ctx: *mut u64, state: *mut u8) -> ();
    next SORT_NEXT;
    fuse &INTERACTIONS_FUSE, "the sort";
    |onward| {
        // SAFETY: `ctx` and `state` are the running sort's own arguments,
        // on its system's thread.
        if !unsafe { sort_refused(ctx, state) } {
            onward.call();
        }
    }
}

/// Whether a filter refuses the sort of `state`, run in `ctx`.
///
/// # Safety
///
/// - `ctx` and `state` are the arguments of a running `access.sort`, used
///   on its system's thread: `ctx` null or its live system context, `state`
///   null or its live action state.
unsafe fn sort_refused(ctx: *mut u64, state: *mut u8) -> bool {
    if crate::world::transactions::holds().quiet() || state.is_null() {
        return false;
    }
    let Some((_, layout, _)) = linked() else {
        return false;
    };
    // SAFETY: `state` is non-null by the check before it and is the sort's
    // action state; its pointer to the player's input sits at the build
    // table's `access.state_input` offset.
    let input = unsafe { read_ptr_at::<u8>(state, layout.state_input) }.cast_const();
    if input.is_null() {
        return false;
    }
    // SAFETY: `input` is the player's non-null input by the check before
    // it; its u32 sort target sits at the build table's
    // `access.sort_target` offset.
    let target: u32 = unsafe { read_at(input, layout.sort_target) };
    // SAFETY: the caller's `ctx` and non-null `state`, on the system's
    // thread.
    !unsafe { may_touch(Interaction::SORT, ctx, state, &[target], &[]) }
}

crate::engine::chains::chain_detour! {
    /// The table's `access.dismantle_method`: an entity a filter keeps the
    /// player from dismantling gets no method.
    fn dismantle(ctx: *mut u8, object: *mut u8, held: *mut u8, permissions: u8) -> u64;
    next DISMANTLE_NEXT;
    fuse &INTERACTIONS_FUSE, "the dismantle";
    |onward| {
        let method = onward.call();
        if crate::world::transactions::holds().quiet()
            || method.trailing_zeros() >= 8
            || ctx.is_null()
            || object.is_null()
        {
            return method;
        }
        let Some((_, layout, _)) = linked() else {
            return method;
        };
        // SAFETY: `object` is non-null by the check before it and is the
        // dismantle's object; its u64 kind, 0 for an entity, sits at the
        // build table's `access.object_kind` offset.
        let kind: u64 = unsafe { read_at(object, layout.object_kind) };
        if kind != 0 {
            return method;
        }
        // SAFETY: `object` is non-null by the check before it and is the
        // dismantle's object; its u64 id sits at the build table's
        // `access.object_id` offset.
        let id: u64 = unsafe { read_at(object, layout.object_id) };
        // SAFETY: `ctx` is non-null by the check before it and is the cursor
        // context, which starts with its system context pointer.
        let system = unsafe { read_ptr_at::<u64>(ctx, 0) };
        // SAFETY: `system` is null or the system context of the running
        // cursor, which runs inside its system on its thread.
        let Some(player) = (unsafe { server_player(system) }) else {
            return method;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the entity id is the low half"
        )]
        let target = id as u32;
        // SAFETY: `system` is the running cursor's system context, on its
        // thread.
        if allowed(&unsafe { interaction(Interaction::DISMANTLE, player, target, system) }) {
            method
        } else {
            0
        }
    }
}

crate::engine::chains::chain_detour! {
    /// The reflected `inventory_actions` system's entry, which the
    /// interaction filters, the item uses and the controls that follow game
    /// actions share. Each runs under its own fuse, not this link's: a panic
    /// in one turns off only that one, and the others stay linked.
    fn inventory_actions(ctx: *mut u64) -> u64;
    next ACTIONS_NEXT;
    fuse &ACTIONS_FUSE, "the inventory actions";
    |onward| {
        // SAFETY: `ctx` is the game's argument to the `inventory_actions`
        // system, at its entry on its thread, which is all `screen_uses`
        // asks.
        unsafe { crate::inventory::item_actions::screen_uses(ctx) };
        if ctx.is_null() || LINKED.get().is_none() {
            return onward.call();
        }
        if !crate::world::transactions::holds().quiet() {
            // SAFETY: `ctx` is non-null by the check before it and is the
            // system's own context, at its entry on its thread.
            let _done =
                INTERACTIONS_FUSE.run("the inventory actions", || unsafe { refuse_pending(ctx) });
        }
        if crate::actions::controls::linked() {
            // SAFETY: `ctx` is non-null by the check before it and is the
            // system's own context, at its entry on its thread.
            unsafe { player_inputs(ctx) };
        }
        onward.call()
    }
}

/// Hands each player's `PlayerInput` in the server's world to the controls
/// that follow game actions.
///
/// # Safety
///
/// - `ctx` is the inventory system's live, non-null context, at its entry
///   on its thread.
unsafe fn player_inputs(ctx: *mut u64) {
    let (Some((loader, layout, components)), Some(engine)) =
        (linked(), crate::engine::ecs::resolved())
    else {
        return;
    };
    // SAFETY: the caller guarantees the system's live context on its thread.
    if !unsafe { server_world(loader, engine, ctx) } {
        return;
    }
    let players = PLAYERS.load();
    let players = players.as_deref().map_or(&[][..], Vec::as_slice);
    let input = |player: enshwick_core::Entity| {
        // SAFETY: the caller guarantees the system's live context, at its
        // entry on its thread.
        unsafe { components.component(ctx, player.0, layout.player_input) }
    };
    // SAFETY: each pointer `input` gives is the player's live `PlayerInput`
    // component in the server's world, read at the system's entry on its
    // thread, before the system runs.
    unsafe { crate::actions::controls::step(bytemuck::cast_slice(players), input) };
}

/// Marks the pending Quick Stash switches and upgrades a filter refuses as
/// already handled.
///
/// # Safety
///
/// - `ctx` is the inventory system's live, non-null context, at its entry
///   on its thread.
unsafe fn refuse_pending(ctx: *mut u64) {
    let (Some((loader, layout, components)), Some(engine)) =
        (linked(), crate::engine::ecs::resolved())
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

    #[test]
    fn the_players_are_read_on_every_step_without_a_list_and_every_thirty_with_one() {
        assert!(read_due(1, false), "the first step after a world loads");
        assert!(read_due(7, false));
        assert!(!read_due(7, true));
        assert!(read_due(PLAYERS_EVERY, true));
        assert!(read_due(2 * PLAYERS_EVERY, true));
    }

    #[test]
    fn the_filters_are_wholly_off_when_their_stock_link_fails() {
        use enshwick_base::Relock;

        let loader = crate::start::tests::bare_loader();
        let found = [
            (pinned::access::OFFER_TEST, 0x1000),
            (pinned::access::TRANSFER, 0x2000),
            (pinned::access::SORT, 0x3000),
            (pinned::access::DISMANTLE_METHOD, 0x4000),
        ];
        let at = |symbol: Symbol| {
            let address = found
                .iter()
                .find(|(s, _)| s.id() == symbol.id())
                .map(|&(_, address)| address);
            Target::Found(symbol.id(), address)
        };
        assert!(
            !filter_off(),
            "a hold keeps players out while the filters run"
        );
        assert!(!link_filters(&loader, at, HookFlags::NONE));
        assert!(INTERACTIONS_FUSE.is_off());
        assert!(filter_off(), "a hold keeps no one out");
        {
            let mut chains = loader.chains.relock();
            chains.seal();
            assert!(
                chains.is_empty(),
                "the offer, transfer and sort links go with the stock's, never patched"
            );
        }
        let listed = crate::engine::diag::parts_off();
        let off = listed.iter().find(|p| p.part == "interaction filters");
        assert!(
            off.is_some_and(|p| p.reason.starts_with("access.base_stock ")),
            "{listed:?}"
        );
    }

    #[test]
    fn a_build_without_the_component_reads_names_the_first_entry_it_lacks() {
        let loader = crate::start::tests::bare_loader();
        assert_eq!(
            Components::found(&loader).err(),
            Some(pinned::ecs::TYPE_INDEX.id())
        );
    }

    #[test]
    fn a_build_without_the_layout_lists_the_filters_actions_and_holds_off() {
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        link(loader);
        assert!(INTERACTIONS_FUSE.is_off() && ACTIONS_FUSE.is_off() && HOLDS_FUSE.is_off());
        assert!(!reads_inputs());
        assert_eq!(
            crate::base::off_reason(&HOLDS_FUSE).as_deref(),
            Some("interaction filters is off")
        );
    }

    #[test]
    fn props_open_to_players_name_the_link_that_is_off_without_a_build_code() {
        let turn = crate::engine::log::capture_turn();
        HOLDS_FUSE.turn_off(&holds_off(&ACTIONS_FUSE));
        let lines = crate::engine::log::captured();
        drop(turn);
        assert!(
            lines
                .iter()
                .any(|(_, _, line)| line
                    == "prop removal by transactions off: inventory actions is off"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_link_made_at_link_time_counts_only_while_its_fuse_is_on() {
        let fuse = Fuse::new("test inventory actions");
        assert!(!still_linked(false, &fuse), "never made");
        assert!(still_linked(true, &fuse));
        fuse.blow("a test");
        assert!(!still_linked(true, &fuse), "turned off after it was made");
    }
}
