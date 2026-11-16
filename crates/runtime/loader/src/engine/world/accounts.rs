//! The account of each character in the server's world, for filters that run
//! off the game thread inside a step, such as the build filters on the
//! cursor check's worker threads. The server rebuilds its player slots and
//! permission table in its frame, before the world step (server 26430253,
//! see [`super::permissions`]), so the loader copies each slot's character
//! and account just before each step of the server's world, while a feature
//! wants them, and a read during the step finds the characters the step's
//! systems see. A character made inside a step is found from the next one.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering, fence};

use enshwick_core::{Entity, PlayerKey};

use super::players::occupied;
use super::{STATE, layout, reader};
use crate::engine::fuse::Fuse;
use crate::engine::memory::read;
use crate::engine::players::MAX_PLAYERS;

/// The copy, without a lock: a sequence number that is odd while the copy
/// is written, so a read that overlaps a write finds nothing rather than a
/// character paired with another's account.
#[derive(Debug)]
pub(super) struct Accounts {
    wanted: AtomicBool,
    sequence: AtomicU32,
    len: AtomicUsize,
    entities: [AtomicU32; MAX_PLAYERS],
    keys: [AtomicU64; MAX_PLAYERS],
}

impl Accounts {
    pub(super) const fn new() -> Self {
        Self {
            wanted: AtomicBool::new(false),
            sequence: AtomicU32::new(0),
            len: AtomicUsize::new(0),
            entities: [const { AtomicU32::new(0) }; MAX_PLAYERS],
            keys: [const { AtomicU64::new(0) }; MAX_PLAYERS],
        }
    }

    /// Replaces the copy with `pairs`, at most [`MAX_PLAYERS`] of them.
    fn write(&self, pairs: impl IntoIterator<Item = (u32, u64)>) {
        self.sequence.fetch_add(1, Ordering::Relaxed);
        fence(Ordering::Release);
        let mut len = 0;
        for ((entity, key), (to_entity, to_key)) in
            pairs.into_iter().zip(self.entities.iter().zip(&self.keys))
        {
            to_entity.store(entity, Ordering::Relaxed);
            to_key.store(key, Ordering::Relaxed);
            len += 1;
        }
        self.len.store(len, Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
    }

    /// The account of the character `entity` in the copy; `None` for a
    /// character it does not hold, or while it is written.
    fn of(&self, entity: Entity) -> Option<PlayerKey> {
        let before = self.sequence.load(Ordering::Acquire);
        if before % 2 == 1 || entity.0 == 0 {
            return None;
        }
        let len = self.len.load(Ordering::Relaxed).min(MAX_PLAYERS);
        let key = self.entities[..len]
            .iter()
            .position(|e| e.load(Ordering::Relaxed) == entity.0)
            .map(|index| self.keys[index].load(Ordering::Relaxed));
        fence(Ordering::Acquire);
        let after = self.sequence.load(Ordering::Relaxed);
        key.filter(|&key| before == after && key != 0)
            .map(PlayerKey::new)
    }
}

/// The copy's feature: a panic in it stops the copy alone, and every read
/// then finds no account.
pub(crate) static FUSE: Fuse = Fuse::new("accounts off the game thread");

/// Starts the copy before each step of the server's world, for a feature
/// that reads [`account_of`]; once wanted, it stays for the run.
pub(crate) fn want() {
    STATE.accounts.wanted.store(true, Ordering::Release);
}

/// Whether a feature wants the copy, the step's one read while none does.
#[inline]
pub(super) fn wanted() -> bool {
    STATE.accounts.wanted.load(Ordering::Relaxed)
}

/// The account of the character `entity` in the server's world as of the
/// start of the current step, on any thread; `None` for a character that
/// is no player's, before the first step, or when the copy is off.
pub(crate) fn account_of(entity: Entity) -> Option<PlayerKey> {
    if FUSE.is_off() {
        return None;
    }
    STATE.accounts.of(entity)
}

/// Copies each player slot's character and account just before `world`
/// steps, when it is the server's world on its game thread. The caller
/// checks [`wanted`] first and runs this under [`FUSE`].
#[cold]
pub(super) fn refresh(world: usize) {
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
    // SAFETY: `reader` returned the live server on its game thread, which
    // alone writes it; the table places its slots.
    let slots = unsafe { occupied(layout.player_slots(), server) };
    STATE.accounts.write(slots.map(|(_, slot, _, entity)| {
        // SAFETY: `slot` is a player slot `occupied` found in the server; the
        // table's `server.player_slot_key` offset holds its `u64` account key.
        (entity, unsafe { read::<u64>(slot, layout.slot_key) })
    }));
}

/// Forgets every account, as the server's world ends.
pub(super) fn clear() {
    STATE.accounts.write([]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_in_the_copy_names_its_account_and_any_other_none() {
        let accounts = Accounts::new();
        accounts.write([(7, 0x5eed), (9, 0xbeef)]);
        assert_eq!(accounts.of(Entity(9)), Some(PlayerKey::new(0xbeef)));
        assert_eq!(accounts.of(Entity(8)), None);
        assert_eq!(accounts.of(Entity(0)), None);
    }

    #[test]
    fn a_new_copy_drops_the_characters_the_last_one_held() {
        let accounts = Accounts::new();
        accounts.write([(7, 0x5eed), (9, 0xbeef)]);
        accounts.write([(11, 0x5eed)]);
        assert_eq!(accounts.of(Entity(7)), None);
        assert_eq!(accounts.of(Entity(11)), Some(PlayerKey::new(0x5eed)));
    }

    #[test]
    fn a_read_while_the_copy_is_written_finds_nothing() {
        let accounts = Accounts::new();
        accounts.write([(7, 0x5eed)]);
        accounts.sequence.fetch_add(1, Ordering::Relaxed);
        assert_eq!(accounts.of(Entity(7)), None);
    }

    #[test]
    fn a_slot_without_an_account_names_none() {
        let accounts = Accounts::new();
        accounts.write([(7, 0)]);
        assert_eq!(accounts.of(Entity(7)), None);
    }

    #[test]
    fn more_slots_than_the_copy_holds_keep_the_first() {
        let accounts = Accounts::new();
        let many = (1..=u32::try_from(MAX_PLAYERS + 3).unwrap_or(u32::MAX))
            .map(|entity| (entity, u64::from(entity)));
        accounts.write(many);
        let last = u32::try_from(MAX_PLAYERS).unwrap_or(u32::MAX);
        assert_eq!(
            accounts.of(Entity(last)),
            Some(PlayerKey::new(u64::from(last)))
        );
        assert_eq!(accounts.of(Entity(last + 1)), None);
    }
}
