//! Containers: the mods' rules over which stacks enter the containers placed
//! from an item, asked in the game's put of one stack into one slot, and the
//! pickup messages mods show without a give.
//!
//! Every player's move puts its stacks through one function, the table's
//! `inventory.put_slot`: a single move, a swap, Take All, Deposit All and
//! Stacks, the action bar and backpack moves, and an area deposit (server
//! 26430253 and client 26430625, read in the decompiler). Its "put nothing,
//! the whole count left over" answer is the game's own for a slot that does
//! not take the stack: a caller that picked the slot rolls its transaction
//! back, one that picks the slots tries the next. Gives and loot put through
//! another function and are not asked.

use core::ffi::c_void;
use core::ops::ControlFlow;
use std::sync::OnceLock;

use enshwick_core::abi::{ContainerPut, ContainerRuleFn, FILTER_REFUSE, HookFlags};
use enshwick_core::raw::{read_at, read_ptr_at, write_at};
use enshwick_core::{Entity, ItemId};

use crate::base::emits::EmitError;
use crate::engine::catalogue::{self as names, ids};
use crate::engine::ecs::Components;
use crate::engine::filters::{self, Filters};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Registered, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::profile::Feature;
use crate::engine::registry::JoinFeatureError;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::inventory::PUT_OUT_ERROR.id(),
    pinned::inventory::PUT_OUT_LEFT.id(),
    pinned::inventory::PUT_OUT_PLACED.id(),
    pinned::inventory::PUT_SLOT.id(),
    pinned::inventory::TXN_OWNERS.id(),
    pinned::ecs::SLOT_ROOT.id(),
];

/// The most items one pickup message shows: the game's op record keeps the
/// count as a `u16` (notify step 0x1401b8820, server 26430253).
const MOST_SHOWN: u32 = u16::MAX as u32;

/// One mod's rule over the containers placed from `item`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rule {
    item: ItemId,
    callback: ContainerRuleFn,
}

static RULES: Filters<Rule> = Filters::new(Feature::Inventory);

/// The switch of the rules' link: a panic in the put turns them off, and
/// the game's own put runs unasked.
static RULES_FUSE: Fuse = Fuse::new("container rules");

/// What the put reads, once linked.
#[derive(Debug, Clone, Copy)]
struct Layout {
    /// Where a transaction keeps its `u64` owner index, which
    /// `ecs.slot_root` walks.
    owners: usize,
    /// `UsedItem`'s type hash.
    used_item: u32,
    /// Where the put's result record keeps its fields.
    out: PutOut,
}

/// Where the put's result record keeps its `u8` error code, its `u32` the
/// put writes zero beside it, and its `u32` count left over, as the put
/// writes them when a slot does not take the stack (server 26430253 and
/// client 26430625, decompiler).
#[derive(Debug, Clone, Copy)]
struct PutOut {
    error: usize,
    placed: usize,
    left: usize,
}

impl PutOut {
    /// The fields at the routed offsets; `None` when two of them overlap,
    /// which no record the put writes has.
    fn new(error: usize, placed: usize, left: usize) -> Option<Self> {
        let fields = [(error, 1), (placed, 4), (left, 4)];
        let apart = |(a, a_len): (usize, usize), (b, b_len): (usize, usize)| {
            a + a_len <= b || b + b_len <= a
        };
        (apart(fields[0], fields[1]) && apart(fields[0], fields[2]) && apart(fields[1], fields[2]))
            .then_some(Self {
                error,
                placed,
                left,
            })
    }
}

static LINKED: OnceLock<(Layout, Components)> = OnceLock::new();

/// The layout and components on this build, or the id of the part it lacks.
fn layout(loader: &Loader) -> Result<(Layout, Components), &'static str> {
    let offset = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    // The put reads the container's item as `UsedItem`'s one u32.
    if names::size(loader, ids::USED_ITEM) != Some(size_of::<u32>()) {
        return Err(ids::USED_ITEM);
    }
    let layout = Layout {
        owners: offset(pinned::inventory::TXN_OWNERS)?,
        used_item: names::component_hash(loader, ids::USED_ITEM).ok_or(ids::USED_ITEM)?,
        out: PutOut::new(
            offset(pinned::inventory::PUT_OUT_ERROR)?,
            offset(pinned::inventory::PUT_OUT_PLACED)?,
            offset(pinned::inventory::PUT_OUT_LEFT)?,
        )
        .ok_or(pinned::inventory::PUT_OUT_PLACED.id())?,
    };
    Ok((layout, Components::found(loader)?))
}

/// Registers mod `owner`'s rule over the containers placed from `item`.
///
/// # Errors
///
/// [`JoinFeatureError`] after init or on a build without the put.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    item: ItemId,
    callback: ContainerRuleFn,
    user: *mut c_void,
) -> Result<(), JoinFeatureError> {
    loader.before_delivery()?;
    let available =
        || loader.address(pinned::inventory::PUT_SLOT).is_some() && layout(loader).is_ok();
    RULES.register_when(available, owner, Rule { item, callback }, user)
}

/// Seals the rules once every mod's init ran, before the links.
pub(crate) fn seal() {
    let _rules = RULES.seal();
}

/// Links the put when a mod registered a rule.
pub(crate) fn link(loader: &'static Loader) {
    if !RULES.has_sealed() {
        return;
    }
    let linked = match layout(loader) {
        Ok(linked) => linked,
        Err(id) => {
            let _off = loader.fuse_off(&RULES_FUSE, id, crate::engine::builds::ABSENT);
            return;
        }
    };
    let _first = LINKED.set(linked);
    if loader.link_feature(
        &RULES_FUSE,
        Target::Pinned(pinned::inventory::PUT_SLOT),
        &PUT_NEXT,
        put,
        HookFlags::NONE,
    ) {
        crate::log!(INFO, "container rules on");
    }
}

/// Whether a rule of `put`'s container item refuses the stack. Only the
/// rules of that item are asked, in load order until one refuses, so a rule
/// of another container costs no call.
fn refused_by(rules: &[Registered<Rule>], put: &ContainerPut) -> bool {
    let of_item = rules
        .iter()
        .filter(|rule| rule.callback.item == put.container_item);
    filters::ask(of_item, Feature::Inventory, |rule| {
        // SAFETY: a rule's mod runs, so its DLL that holds the callback stays
        // loaded; it gets the user pointer its mod registered and `put`,
        // which lives for the call.
        let lets_in =
            rule.call(|rule, user| unsafe { (rule.callback)(put, user) } != FILTER_REFUSE);
        if lets_in {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    })
    .is_some()
}

crate::engine::chains::chain_detour! {
    /// The table's `inventory.put_slot`: a stack a rule refuses enters
    /// nothing, as for a slot that does not take it.
    fn put(
        out: *mut u8,
        txn: *mut u8,
        slot: u64,
        info: *const u32,
        extra: u32,
        count: u32,
    ) -> *mut u8;
    next PUT_NEXT;
    fuse &RULES_FUSE, "the container put";
    |onward| {
        if out.is_null() || txn.is_null() || info.is_null() || count == 0 {
            return onward.call();
        }
        let Some((layout, components)) = LINKED.get() else {
            return onward.call();
        };
        // SAFETY: `txn` and `info` are the running put's own non-null
        // arguments (checked before it), on its system's thread.
        if !unsafe { refused(layout, components, txn, slot, info, count) } {
            return onward.call();
        }
        // SAFETY: `out` is the put's non-null result record (checked before
        // it), its fields where the routes found the put writes them.
        unsafe { put_nothing(out, layout.out, count) };
        out
    }
}

/// Writes the put's "put nothing" answer into its result record at `out`:
/// no error, zero beside it and the whole `count` left over, as the game's
/// put answers for a slot that does not take the stack.
///
/// # Safety
///
/// - `out` is writable at each of `fields`: a byte at `error`, four bytes at
///   `placed` and `left`.
unsafe fn put_nothing(out: *mut u8, fields: PutOut, count: u32) {
    // SAFETY: the caller's contract: a byte is writable at `error`.
    unsafe { write_at(out, fields.error, 0u8) };
    // SAFETY: the caller's contract: four bytes are writable at `placed`.
    unsafe { write_at(out, fields.placed, 0u32) };
    // SAFETY: the caller's contract: four bytes are writable at `left`.
    unsafe { write_at(out, fields.left, count) };
}

/// Whether a rule refuses `count` of the item `info` describes entering
/// `slot` in the transaction `txn`.
///
/// # Safety
///
/// - `txn` is the live transaction of a running `inventory.put_slot`, used on
///   its system's thread; `info` is the item info that put received.
unsafe fn refused(
    layout: &Layout,
    components: &Components,
    txn: *mut u8,
    slot: u64,
    info: *const u32,
    count: u32,
) -> bool {
    // SAFETY: the caller's contract, which `asked` shares.
    unsafe { asked(layout, components, txn, slot, info, count) }
        .is_some_and(|put| RULES.sealed().is_some_and(|rules| refused_by(rules, &put)))
}

/// The put a rule hears: `count` of the item `info` describes entering
/// `slot` in the transaction `txn`; `None` for a slot or container the put
/// does not name.
///
/// # Safety
///
/// As [`refused`].
unsafe fn asked(
    layout: &Layout,
    components: &Components,
    txn: *mut u8,
    slot: u64,
    info: *const u32,
    count: u32,
) -> Option<ContainerPut> {
    if slot == 0 {
        return None;
    }
    // SAFETY: `txn` is the put's live transaction; `inventory.txn_init`
    // writes its system context to the first word (server 26430253).
    let ctx = unsafe { read_ptr_at::<u64>(txn, 0) };
    // SAFETY: `txn` is the put's live transaction, which holds its u64 owner
    // index at the build table's `inventory.txn_owners`.
    let owners: u64 = unsafe { read_at(txn, layout.owners) };
    if ctx.is_null() {
        return None;
    }
    // SAFETY: `ctx` is the non-null context of the system running the put,
    // on its thread, and `owners` comes from that system's transaction.
    let container = unsafe { components.slot_root(ctx, slot, owners) }?;
    // SAFETY: `ctx` is the running system's live context on its thread.
    let used = unsafe { components.component(ctx, container, layout.used_item) }?;
    // SAFETY: `used` is the container's live `UsedItem` component, which
    // reflection sizes at 4 bytes, its u32 item id alone (checked by
    // `layout`).
    let container_item = ItemId::new(unsafe { read_at(used, 0) });
    Some(enshwick_abi::headed!(ContainerPut {
        container: Entity::from_engine(container),
        container_item,
        // SAFETY: `info` is the put's non-null item info, which starts with
        // the u32 item id the put writes into the slot (server 26430253).
        item: ItemId::new(unsafe { read_at(info.cast::<u8>(), 0) }),
        count,
        slot: crate::inventory::stacks::slot_id(slot),
    }))
}

/// Raises a pickup of `count` of `item` by `character`'s player on the
/// server, with nothing moved, so the player's client shows the game's
/// pickup message; the item filters do not hear it.
///
/// # Errors
///
/// [`EmitError::Unread`] on a build without the event,
/// [`EmitError::NoEntity`] for an entity that is no player in game, and
/// [`EmitError::World`] off the server's game thread.
pub(crate) fn show_pickup(
    loader: &Loader,
    character: Entity,
    item: ItemId,
    count: u32,
) -> Result<(), EmitError> {
    let layout = crate::inventory::item_actions::pickup_event(loader).ok_or(EmitError::Unread)?;
    let count_at = layout.count.ok_or(EmitError::Unread)?;
    let players = crate::engine::world::player_entities(loader).map_err(EmitError::World)?;
    if !players.contains(&character.get()) {
        return Err(EmitError::NoEntity);
    }
    let shown = count.min(MOST_SHOWN);
    let mut bytes = vec![0u8; layout.size];
    for (at, value) in [
        (layout.item, item.get()),
        (count_at, shown),
        (layout.player, character.get()),
    ] {
        let field = bytes.get_mut(at..at + 4).ok_or(EmitError::BadSize)?;
        field.copy_from_slice(&value.to_le_bytes());
    }
    crate::base::emits::push(loader, layout.hash, &bytes)?;
    crate::inventory::item_actions::own_pickup(character.get(), item.get(), shown);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn refuse_ore(put: *const ContainerPut, _: *mut c_void) -> u8 {
        // SAFETY: the test passes a live put.
        if unsafe { (*put).item }.get() == 7 {
            FILTER_REFUSE
        } else {
            0
        }
    }

    static ASKED: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

    unsafe extern "system" fn count_asks(_: *const ContainerPut, _: *mut c_void) -> u8 {
        let _before = ASKED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        0
    }

    #[test]
    fn put_nothing_writes_each_field_where_the_routes_found_it() {
        let mut record = [0xaau8; 16];
        let fields = PutOut {
            error: 0,
            placed: 4,
            left: 8,
        };
        // SAFETY: `record` is writable for 16 bytes, past each field.
        unsafe { put_nothing(record.as_mut_ptr(), fields, 7) };
        assert_eq!(
            record,
            [
                0, 0xaa, 0xaa, 0xaa, 0, 0, 0, 0, 7, 0, 0, 0, 0xaa, 0xaa, 0xaa, 0xaa
            ],
            "server 26430253: the padding and the bytes past the record stay"
        );
        let moved = PutOut {
            error: 1,
            placed: 8,
            left: 4,
        };
        let mut record = [0xaau8; 16];
        // SAFETY: `record` is writable for 16 bytes, past each field.
        unsafe { put_nothing(record.as_mut_ptr(), moved, 7) };
        assert_eq!(
            &record[..12],
            &[0xaa, 0, 0xaa, 0xaa, 7, 0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn put_result_fields_that_overlap_are_refused() {
        assert!(PutOut::new(0, 4, 8).is_some(), "server 26430253");
        assert!(PutOut::new(1, 8, 4).is_some());
        assert!(PutOut::new(2, 0, 8).is_none(), "placed over the error byte");
        assert!(PutOut::new(0, 4, 6).is_none(), "left over placed");
        assert!(
            PutOut::new(4, 4, 8).is_none(),
            "the error byte inside placed"
        );
    }

    #[test]
    fn only_the_rules_of_the_container_item_are_called() {
        // The loader's own index always runs, so its rules are live.
        let rule = |item: u32, callback: ContainerRuleFn| {
            Registered::new(
                ModIndex::LOADER,
                Rule {
                    item: ItemId::new(item),
                    callback,
                },
                core::ptr::null_mut(),
            )
        };
        let rules = [rule(0x5e01, refuse_ore), rule(0x5e02, count_asks)];
        let put = |container_item: u32, item: u32| {
            enshwick_abi::headed!(ContainerPut {
                container: Entity::from_engine(9),
                container_item: ItemId::new(container_item),
                item: ItemId::new(item),
                count: 1,
                slot: 10,
            })
        };
        assert!(refused_by(&rules, &put(0x5e01, 7)));
        assert!(!refused_by(&rules, &put(0x5e01, 8)));
        assert_eq!(
            ASKED.load(core::sync::atomic::Ordering::Relaxed),
            0,
            "another container's rule is never called"
        );
        assert!(!refused_by(&rules, &put(0x5e02, 7)));
        assert_eq!(ASKED.load(core::sync::atomic::Ordering::Relaxed), 1);
        assert!(!refused_by(&rules, &put(0x5e03, 7)), "an unruled container");
    }
}
