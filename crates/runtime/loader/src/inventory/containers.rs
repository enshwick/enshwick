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

use enshwick_core::abi::{ContainerPut, ContainerRuleFn, FILTER_REFUSE, Header, HookFlags};
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
}

static LINKED: OnceLock<(Layout, Components)> = OnceLock::new();

/// The layout and components on this build, or the id of the part it lacks.
fn layout(loader: &Loader) -> Result<(Layout, Components), &'static str> {
    let owners = pinned::inventory::TXN_OWNERS;
    // The put reads the container's item as `UsedItem`'s one u32.
    if names::size(loader, ids::USED_ITEM) != Some(size_of::<u32>()) {
        return Err(ids::USED_ITEM);
    }
    let layout = Layout {
        owners: loader.offset(owners).ok_or(owners.id())?,
        used_item: names::component_hash(loader, ids::USED_ITEM).ok_or(ids::USED_ITEM)?,
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
    if loader.link_fused(
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
        // SAFETY: `txn` and `info` are the running put's own non-null
        // arguments (checked before it), on its system's thread.
        if !unsafe { refused(txn, slot, info, count) } {
            return onward.call();
        }
        let nothing = PutResult {
            error: 0,
            reserved: [0; 3],
            placed: 0,
            left: count,
        };
        // SAFETY: `out` is the put's non-null result record (checked before
        // it), laid out as `PutResult`.
        unsafe { write_at(out, 0, nothing) };
        out
    }
}

/// The put's result record, as the game writes "put nothing" when a slot
/// does not take the stack (server 26430253, client 26430625).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PutResult {
    /// The game's error code, 0 for none.
    error: u8,
    reserved: [u8; 3],
    /// Zero for a put that placed nothing.
    placed: u32,
    /// The count left over: the whole count when nothing went in.
    left: u32,
}

/// Whether a rule refuses `count` of the item `info` describes entering
/// `slot` in the transaction `txn`.
///
/// # Safety
///
/// - `txn` is the live transaction of a running `inventory.put_slot`, used on
///   its system's thread; `info` is the item info that put received.
unsafe fn refused(txn: *mut u8, slot: u64, info: *const u32, count: u32) -> bool {
    let Some((layout, components)) = LINKED.get() else {
        return false;
    };
    if slot == 0 {
        return false;
    }
    // SAFETY: `txn` is the put's live transaction; `inventory.txn_init`
    // writes its system context to the first word (server 26430253).
    let ctx = unsafe { read_ptr_at::<u64>(txn, 0) };
    // SAFETY: `txn` is the put's live transaction, which holds its u64 owner
    // index at the build table's `inventory.txn_owners`.
    let owners: u64 = unsafe { read_at(txn, layout.owners) };
    if ctx.is_null() {
        return false;
    }
    // SAFETY: `ctx` is the non-null context of the system running the put,
    // on its thread, and `owners` comes from that system's transaction.
    let Some(container) = (unsafe { components.slot_root(ctx, slot, owners) }) else {
        return false;
    };
    // SAFETY: `ctx` is the running system's live context on its thread.
    let Some(used) = (unsafe { components.component(ctx, container, layout.used_item) }) else {
        return false;
    };
    // SAFETY: `used` is the container's live `UsedItem` component, which
    // reflection sizes at 4 bytes, its u32 item id alone (checked by
    // `layout`).
    let container_item = ItemId::new(unsafe { read_at(used, 0) });
    let Some(rules) = RULES.sealed() else {
        return false;
    };
    let put = ContainerPut {
        header: Header::of::<ContainerPut>(),
        container: Entity(container),
        container_item,
        // SAFETY: `info` is the put's non-null item info, which starts with
        // the u32 item id the put writes into the slot (server 26430253).
        item: ItemId::new(unsafe { read_at(info.cast::<u8>(), 0) }),
        count,
        slot: crate::inventory::stacks::slot_id(slot),
    };
    refused_by(rules, &put)
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
    if !players.contains(&character.0) {
        return Err(EmitError::NoEntity);
    }
    let shown = count.min(MOST_SHOWN);
    let mut bytes = vec![0u8; layout.size];
    for (at, value) in [
        (layout.item, item.get()),
        (count_at, shown),
        (layout.player, character.0),
    ] {
        let field = bytes.get_mut(at..at + 4).ok_or(EmitError::BadSize)?;
        field.copy_from_slice(&value.to_le_bytes());
    }
    crate::base::emits::push(loader, layout.hash, &bytes)?;
    crate::inventory::item_actions::own_pickup(character.0, item.get(), shown);
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
        let put = |container_item: u32, item: u32| ContainerPut {
            header: Header::of::<ContainerPut>(),
            container: Entity(9),
            container_item: ItemId::new(container_item),
            item: ItemId::new(item),
            count: 1,
            slot: 10,
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
