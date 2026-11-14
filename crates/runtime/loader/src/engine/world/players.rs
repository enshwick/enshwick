//! The players of the server's world: who is in game, their characters
//! and machines, read from the server's player slots.

use arrayvec::ArrayVec;
use enshwick_core::Entity;
use enshwick_core::abi::{Header, PlayerInfo};

use super::{Layout, MAX_NAME, SLOT_IN_GAME, WorldError, layout, reader};
use crate::engine::loader::Loader;
use crate::engine::memory::read;
use crate::engine::players::{CharacterId, MAX_PLAYERS};

/// Every player in game, in slot order.
pub(crate) fn players(loader: &Loader) -> Result<Vec<PlayerInfo>, WorldError> {
    Ok(in_game(loader, |_| true)?.collect())
}

/// The player in game whose character is `entity`, as [`players`] lists it,
/// read without the others.
pub(crate) fn player(loader: &Loader, entity: Entity) -> Result<PlayerInfo, WorldError> {
    in_game(loader, |found| found == entity)?
        .next()
        .ok_or(WorldError::Absent)
}

/// The players in game whose character `wanted` takes, in slot order, each
/// read only when the iterator reaches it.
fn in_game(
    loader: &Loader,
    mut wanted: impl FnMut(Entity) -> bool,
) -> Result<impl Iterator<Item = PlayerInfo>, WorldError> {
    let layout = layout(loader).ok_or(WorldError::Unread)?;
    let server = reader()?;
    // SAFETY: `server` is the server `reader` returned on the game thread,
    // which alone writes it; the table's `server.player_infos` offset holds
    // its player infos pointer.
    let infos: usize = unsafe { read(server, layout.server_players) };
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table places its slots.
    Ok(unsafe { occupied(layout.player_slots(), server) }
        .filter(move |&(_, _, state, found)| state == SLOT_IN_GAME && wanted(Entity(found)))
        .map(move |(index, slot, _, found)| player_at(layout, slot, index, Entity(found), infos)))
}

/// The character entity of each player in game, as [`players`] lists them,
/// without their names and permissions.
pub(crate) fn player_entities(loader: &Loader) -> Result<ArrayVec<u32, MAX_PLAYERS>, WorldError> {
    let layout = layout(loader).ok_or(WorldError::Unread)?;
    let server = reader()?;
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table places its slots.
    Ok(unsafe { occupied(layout.player_slots(), server) }
        .filter(|&(_, _, state, _)| state == SLOT_IN_GAME)
        .map(|(_, _, _, entity)| entity)
        .take(MAX_PLAYERS)
        .collect())
}

/// The character entity of every player slot that names one, in any state:
/// a leaving player's counts until its slot clears.
pub(crate) fn character_entities(
    loader: &Loader,
) -> Result<ArrayVec<u32, MAX_PLAYERS>, WorldError> {
    let layout = layout(loader).ok_or(WorldError::Unread)?;
    let server = reader()?;
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table places its slots.
    Ok(unsafe { occupied(layout.player_slots(), server) }
        .map(|(_, _, _, entity)| entity)
        .take(MAX_PLAYERS)
        .collect())
}

/// The character `entity` plays, as [`characters`] reads it.
pub(crate) fn character_of(loader: &Loader, entity: Entity) -> Option<CharacterId> {
    character_in(&characters(loader), entity)
}

/// The character `entity` plays among `characters`, as [`characters`]
/// lists them.
pub(crate) fn character_in(
    characters: &[(Entity, CharacterId)],
    entity: Entity,
) -> Option<CharacterId> {
    characters
        .iter()
        .find_map(|&(e, character)| (e == entity).then_some(character))
}

/// The character of each player slot that names a character entity, a
/// leaving player's included until its slot clears. Empty when the build
/// table does not pin where a slot holds the GUID, or off the game thread.
pub(crate) fn characters(loader: &Loader) -> ArrayVec<(Entity, CharacterId), MAX_PLAYERS> {
    let Some(layout) = layout(loader) else {
        return ArrayVec::new();
    };
    let (Some(at), Ok(server)) = (
        layout
            .slot_character
            .filter(|&at| at + size_of::<CharacterId>() <= layout.slot_size),
        reader(),
    ) else {
        return ArrayVec::new();
    };
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table places its slots, and its
    // `server.player_slot_character` offset holds the GUID inside a slot
    // (`at + 16 <= slot_size`, checked above).
    unsafe { guids(layout.player_slots(), server, at) }
        .take(MAX_PLAYERS)
        .collect()
}

/// The index, address, state byte and character entity of each player slot
/// of `server` with a handle and an entity, in slot order.
///
/// # Safety
///
/// - `server` holds `slots.count` player slots of `slots.size` bytes from
///   `slots.first`, each with its `u32` handle at 0, its state byte at
///   `slots.state` and its `u32` entity at `slots.entity`.
/// - Nothing writes them while the iterator runs: on the game thread.
unsafe fn occupied(slots: Slots, server: usize) -> impl Iterator<Item = (usize, usize, u8, u32)> {
    (0..slots.count).filter_map(move |index| {
        let slot = server + slots.first + index * slots.size;
        // SAFETY: `index < count`, so `slot` is one of the caller's player
        // slots, which starts with its `u32` handle and has no writer.
        let handle: u32 = unsafe { read(slot, 0) };
        // SAFETY: `slot` is one of the caller's player slots, its state byte
        // at `slots.state`.
        let state: u8 = unsafe { read(slot, slots.state) };
        // SAFETY: `slot` is one of the caller's player slots, its `u32` entity
        // at `slots.entity`.
        let entity: u32 = unsafe { read(slot, slots.entity) };
        (handle != 0 && entity != 0).then_some((index, slot, state, entity))
    })
}

/// The character entity and GUID of each player slot of `server` that names
/// both, in any state: a leaving player's slot counts until it clears.
///
/// # Safety
///
/// As [`occupied`]; each slot also holds a [`CharacterId`] at `at`, with
/// `at + size_of::<CharacterId>() <= slots.size`.
pub(super) unsafe fn guids(
    slots: Slots,
    server: usize,
    at: usize,
) -> impl Iterator<Item = (Entity, CharacterId)> {
    // SAFETY: the caller's guarantees are `occupied`'s.
    unsafe { occupied(slots, server) }.filter_map(move |(_, slot, _, entity)| {
        // SAFETY: `slot` is one of the caller's player slots, which holds the
        // GUID's bytes at `at`, inside the slot.
        let guid: [u8; size_of::<CharacterId>()] = unsafe { read(slot, at) };
        (guid != [0; size_of::<CharacterId>()]).then_some((Entity(entity), CharacterId(guid)))
    })
}

/// Where the server holds its player slots, from the build table.
#[derive(Clone, Copy, Debug)]
pub(super) struct Slots {
    pub(super) first: usize,
    pub(super) size: usize,
    pub(super) count: usize,
    pub(super) state: usize,
    pub(super) entity: usize,
}

impl Layout {
    const fn player_slots(&self) -> Slots {
        Slots {
            first: self.slots,
            size: self.slot_size,
            count: self.slot_count,
            state: self.slot_state,
            entity: self.slot_entity,
        }
    }
}

/// The machine handle of each player in game, by account key: the handle
/// the server's network code names a connection by.
pub(crate) fn machines(loader: &Loader) -> Result<Vec<(u64, u32)>, WorldError> {
    let layout = layout(loader).ok_or(WorldError::Unread)?;
    let server = reader()?;
    Ok((0..layout.slot_count)
        .filter_map(|index| {
            let slot = server + layout.slots + index * layout.slot_size;
            // SAFETY: the table's `server.player_slots`, `player_slot_size`
            // and `player_slot_count` place `slot_count` slots in the server,
            // `index < slot_count`, and each slot starts with its `u32`
            // handle; read on the game thread (checked by `reader`).
            let handle: u32 = unsafe { read(slot, 0) };
            // SAFETY: `slot` is a player slot of the server (`index <
            // slot_count`, slots from the table); the table's
            // `server.player_slot_machine` offset holds its `u32` machine
            // handle; read on the game thread (checked by `reader`).
            let machine: u32 = unsafe { read(slot, layout.slot_machine) };
            // SAFETY: `slot` is a player slot of the server (`index <
            // slot_count`, slots from the table); the table's
            // `server.player_slot_key` offset holds its `u64` account key;
            // read on the game thread (checked by `reader`).
            let key: u64 = unsafe { read(slot, layout.slot_key) };
            (handle != 0 && machine != 0).then_some((key, machine))
        })
        .collect())
}

/// The account keys of the live player slots, as [`machines`] reads them;
/// `None` off the game thread or when the slots do not read.
pub(crate) fn live_keys() -> Option<Vec<u64>> {
    let machines = machines(crate::engine::loader::loader()?).ok()?;
    Some(machines.iter().map(|&(key, _)| key).collect())
}

fn player_at(
    layout: &Layout,
    slot: usize,
    index: usize,
    entity: Entity,
    infos: usize,
) -> PlayerInfo {
    let mut name = [0u8; 64];
    // SAFETY: `slot` is a player slot `players` found in the server on the
    // game thread; the table's `server.player_slot_name` offset holds the
    // name's data pointer.
    let data: usize = unsafe { read(slot, layout.slot_name) };
    // SAFETY: `slot` is a player slot `players` found in the server on the
    // game thread; the name's `u64` byte length follows its data pointer at
    // the table's `server.player_slot_name` offset.
    let len: u64 = unsafe { read(slot, layout.slot_name + size_of::<usize>()) };
    let len = usize::try_from(len).unwrap_or(usize::MAX);
    let mut name_len = 0;
    if data != 0 && len <= MAX_NAME {
        let kept = len.min(name.len());
        // SAFETY: `data` is non-zero (the `data != 0` condition) and the name
        // buffer holds `len` bytes, `kept <= len`; read on the game thread.
        unsafe { crate::engine::memory::read_bytes(data, 0, &mut name[..kept]) };
        name_len = kept;
    }
    let permissions = if infos == 0 {
        0
    } else {
        let table = infos + layout.permission_table;
        let index = (entity.0 as usize).wrapping_sub(1);
        if index < layout.permission_entries {
            let entry = table + index * layout.permission_entry;
            // SAFETY: `infos` is non-zero (`infos == 0` gives 0 first); the
            // table's `server.permission_table`, `permission_entries` and
            // `permission_entry_size` place that many entries there, `index`
            // is below their count (the enclosing condition), and each starts
            // with its `u32` entity; read on the game thread.
            let owner: u32 = unsafe { read(entry, 0) };
            if owner == entity.0 {
                // SAFETY: `entry` is a permission entry inside the table
                // (below `permission_entries`, the enclosing condition); the
                // table's `server.permission_mask` offset holds its mask byte;
                // read on the game thread.
                unsafe { read::<u8>(entry, layout.permission_mask) }
            } else {
                0
            }
        } else {
            0
        }
    };
    PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: u32::try_from(index).unwrap_or(u32::MAX),
        entity,
        // SAFETY: `slot` is a player slot `players` found in the server on
        // the game thread; the table's `server.player_slot_key` offset holds
        // its `u64` account key.
        key: unsafe { read(slot, layout.slot_key) },
        permissions,
        name_len: u8::try_from(name_len).unwrap_or(0),
        name,
    }
}
