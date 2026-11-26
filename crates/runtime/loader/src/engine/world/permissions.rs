//! The step write of mods' changes to players' permissions. The server
//! rebuilds each joined player's permission byte from the player's user
//! group in every frame, before its world step (`FUN_14023b000` from the
//! server frame, seen live with x64dbg on server 26430253), so a byte
//! written once is gone by the next step. The loader therefore applies the
//! changes [`crate::world::permissions`] publishes on top of the game's fresh
//! byte just before each step of the server's world, and a change lasts
//! exactly as long as it stays published: the next frame brings the group's
//! bits back by itself.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwapOption;
use enshwick_core::Entity;

use super::players::occupied;
use super::{Layout, SLOT_IN_GAME, STATE, WorldError, layout, reader};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, is_off};
use crate::engine::memory::{read, write};
use crate::engine::mod_index::ModIndex;

/// One mod's change to one account's permissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub(crate) owner: ModIndex,
    pub(crate) key: u64,
    pub(crate) deny: u8,
    pub(crate) grant: u8,
}

/// The byte the server holds the account `key` to under `changes`, from
/// its group's `group`: a bit any mod that is on takes is gone, else a bit
/// any such mod gives is there.
pub(crate) fn held(changes: &[Change], key: u64, group: u8, off: impl Fn(ModIndex) -> bool) -> u8 {
    let (deny, grant) = changes
        .iter()
        .filter(|c| c.key == key && !off(c.owner))
        .fold((0, 0), |(deny, grant), c| (deny | c.deny, grant | c.grant));
    (group | grant) & !deny
}

/// What the step write reads, without a lock.
#[derive(Debug)]
struct Written {
    /// The changes the step applies, published whole by
    /// [`crate::world::permissions`]; `None` while there are none.
    changes: ArcSwapOption<Vec<Change>>,
    /// Whether `changes` holds any, the step's one read while none does.
    any: AtomicBool,
    /// Whether the parts hear a player leave, so a change ends with its
    /// player.
    leaves: AtomicBool,
}

static WRITTEN: Written = Written {
    changes: ArcSwapOption::const_empty(),
    any: AtomicBool::new(false),
    leaves: AtomicBool::new(false),
};

/// The step write's feature: a panic in it stops the write alone.
pub(crate) static FUSE: Fuse = Fuse::new("permission changes");

/// Hands the step `changes`, which replace every change it applied.
pub(crate) fn publish(changes: Vec<Change>) {
    WRITTEN.any.store(!changes.is_empty(), Ordering::Release);
    WRITTEN
        .changes
        .store((!changes.is_empty()).then(|| Arc::new(changes)));
}

/// Records whether the server's player removal link, which ends a leaving
/// player's changes, is made.
pub(crate) fn hear_leaves(linked: bool) {
    WRITTEN.leaves.store(linked, Ordering::Release);
}

/// Whether a change can run on this build: the world step is linked, the
/// write has not panicked, and a leaving player's changes end with them.
pub(crate) fn writes() -> bool {
    STATE.step_linked.load(Ordering::Acquire)
        && WRITTEN.leaves.load(Ordering::Acquire)
        && !FUSE.is_off()
}

/// Whether any change waits for the step, one atomic read.
#[inline]
pub(super) fn pending() -> bool {
    WRITTEN.any.load(Ordering::Acquire)
}

/// The permission byte `group` that the account `key` read in the table,
/// with the published changes applied as the step applies them, so a read
/// at any moment of the frame gives the bits in force.
pub(super) fn in_force(key: u64, group: u8) -> u8 {
    if !pending() {
        return group;
    }
    WRITTEN
        .changes
        .load()
        .as_deref()
        .map_or(group, |changes| held(changes, key, group, is_off))
}

/// Whether a player with account `key` is in game on the server, read from
/// the slots alone.
///
/// # Errors
///
/// [`WorldError::NoWorld`] while no server runs, [`WorldError::WrongThread`]
/// off its game thread, [`WorldError::Unread`] on a build without the slots.
pub(crate) fn in_game(loader: &Loader, key: u64) -> Result<bool, WorldError> {
    let server = reader()?;
    let layout = layout(loader).ok_or(WorldError::Unread)?;
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table places its slots.
    let mut slots = unsafe { occupied(layout.player_slots(), server) };
    Ok(slots.any(|(_, slot, state, _)| {
        // SAFETY: `slot` is a player slot `occupied` found in the server; the
        // table's `server.player_slot_key` offset holds its `u64` account key.
        state == SLOT_IN_GAME && unsafe { read::<u64>(slot, layout.slot_key) } == key
    }))
}

/// Ends every change the step applies, as the server's world ends.
pub(super) fn clear() {
    publish(Vec::new());
}

/// Applies the published changes to the permission bytes of the players in
/// `world` just before it steps, when it is the server's world on its game
/// thread. The caller checks [`pending`] first and runs this under
/// [`FUSE`].
#[cold]
pub(super) fn apply(world: usize) {
    let (Some(loader), Ok(server)) = (crate::engine::loader::loader(), reader()) else {
        return;
    };
    let Some(layout) = layout(loader) else {
        return;
    };
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table's `server.world` offset holds its world
    // pointer.
    let own: usize = unsafe { read(server, layout.server_world) };
    if own == 0 || own != world {
        return;
    }
    let published = WRITTEN.changes.load();
    let Some(changes) = published.as_deref() else {
        return;
    };
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table's `server.player_infos` offset holds the
    // pointer to the world object that owns the permission table.
    let infos: usize = unsafe { read(server, layout.server_players) };
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table places its slots.
    for (_, slot, state, entity) in unsafe { occupied(layout.player_slots(), server) } {
        if state != SLOT_IN_GAME {
            continue;
        }
        // SAFETY: `slot` is a player slot `occupied` found in the server; the
        // table's `server.player_slot_key` offset holds its `u64` account key.
        let key: u64 = unsafe { read(slot, layout.slot_key) };
        if !changes.iter().any(|c| c.key == key) {
            continue;
        }
        let Some(entry) = entry(layout, infos, Entity::from_engine(entity)) else {
            continue;
        };
        // SAFETY: `entry` is `entity`'s own entry of the permission table
        // (checked by `entry`), its mask byte at the table's
        // `server.permission_mask` offset; read on the game thread.
        let group: u8 = unsafe { read(entry, layout.permission_mask) };
        // SAFETY: the same byte, writable inside the game's table. The frame
        // rebuilt it before this step and no system of the step runs yet, so
        // nothing else reads or writes it now (server 26430253).
        unsafe {
            write(
                entry,
                layout.permission_mask,
                held(changes, key, group, is_off),
            );
        }
    }
}

/// The permission table entry of `entity` in the world object `infos`, when
/// the entry is the entity's own: the game's lookup checks the same.
pub(super) fn entry(layout: &Layout, infos: usize, entity: Entity) -> Option<usize> {
    if infos == 0 {
        return None;
    }
    let index = (entity.get() as usize).wrapping_sub(1);
    if index >= layout.permission_entries {
        return None;
    }
    let entry = infos + layout.permission_table + index * layout.permission_entry;
    // SAFETY: `infos` is non-zero (`infos == 0` returns first); the table's
    // `server.permission_table`, `permission_entries` and
    // `permission_entry_size` place that many entries there, `index` is
    // below their count (`index >= permission_entries` returns first), and
    // each starts with its `u32` entity; read on the game thread.
    let owner: u32 = unsafe { read(entry, 0) };
    (owner == entity.get()).then_some(entry)
}

#[cfg(test)]
mod tests {
    use enshwick_core::abi::PlayerInfo;

    use super::*;

    const ANN: u64 = 7;
    const BOB: u64 = 9;

    fn bits(value: u32) -> u8 {
        u8::try_from(value).unwrap()
    }

    fn change(owner: usize, key: u64, deny: u32, grant: u32) -> Change {
        Change {
            owner: ModIndex::new(owner),
            key,
            deny: bits(deny),
            grant: bits(grant),
        }
    }

    #[test]
    fn a_bit_any_mod_takes_is_gone_even_when_another_mod_gives_it() {
        let changes = [
            change(0, ANN, PlayerInfo::CAN_EDIT_WORLD, 0),
            change(
                1,
                ANN,
                0,
                PlayerInfo::CAN_EDIT_WORLD | PlayerInfo::CAN_EDIT_BASE,
            ),
            change(1, BOB, PlayerInfo::CAN_ACCESS_INVENTORIES, 0),
        ];
        let group = bits(PlayerInfo::CAN_EDIT_WORLD | PlayerInfo::CAN_RECEIVE_EXP);
        assert_eq!(
            u32::from(held(&changes, ANN, group, |_| false)),
            PlayerInfo::CAN_EDIT_BASE | PlayerInfo::CAN_RECEIVE_EXP
        );
        assert_eq!(
            held(&changes, BOB, group, |_| false),
            group,
            "a bit the group lacks stays off"
        );
    }

    #[test]
    fn a_change_of_a_mod_that_is_off_no_longer_applies() {
        let changes = [
            change(0, ANN, PlayerInfo::CAN_EDIT_WORLD, 0),
            change(1, ANN, 0, PlayerInfo::CAN_KICK_BAN),
        ];
        let group = bits(PlayerInfo::CAN_EDIT_WORLD);
        let first_off = |owner: ModIndex| owner == ModIndex::new(0);
        assert_eq!(
            u32::from(held(&changes, ANN, group, first_off)),
            PlayerInfo::CAN_EDIT_WORLD | PlayerInfo::CAN_KICK_BAN
        );
    }

    #[test]
    fn a_read_gives_the_bits_in_force_while_a_change_is_published() {
        let group = bits(PlayerInfo::CAN_EDIT_WORLD);
        // Without a loader every owner but the loader counts as off.
        publish(vec![Change {
            owner: ModIndex::LOADER,
            ..change(0, ANN, PlayerInfo::CAN_EDIT_WORLD, 0)
        }]);
        assert_eq!(in_force(ANN, group), 0);
        assert_eq!(
            in_force(BOB, group),
            group,
            "another account keeps its bits"
        );
        clear();
        assert!(!pending());
        assert_eq!(in_force(ANN, group), group);
    }
}
