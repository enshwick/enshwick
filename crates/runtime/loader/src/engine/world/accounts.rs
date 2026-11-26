//! The account of each character in the server's world, for filters that run
//! off the game thread inside a step, such as the build filters on the
//! cursor check's worker threads. The server rebuilds its player slots and
//! permission table in its frame, before the world step (server 26430253,
//! see [`super::permissions`]), so the loader copies each slot's character
//! and account just before each step of the server's world, while a feature
//! wants them, and a read during the step finds the characters the step's
//! systems see. A character made inside a step is found from the next one.

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::{Entity, PlayerKey};

use super::players::occupied;
use super::{SLOT_IN_GAME, STATE, layout, reader};
use crate::engine::copied::Copied;
use crate::engine::fuse::Fuse;
use crate::engine::memory::read;
use crate::engine::players::MAX_PLAYERS;

/// The copy's words: how many players it holds, then each one's character
/// and account.
const WORDS: usize = 1 + 2 * MAX_PLAYERS;

/// The copy, without a lock, and whether a feature wants it.
#[derive(Debug)]
pub(super) struct Accounts {
    wanted: AtomicBool,
    copy: Copied<WORDS>,
}

impl Accounts {
    pub(super) const fn new() -> Self {
        Self {
            wanted: AtomicBool::new(false),
            copy: Copied::new(),
        }
    }

    /// Replaces the copy with `pairs`, at most [`MAX_PLAYERS`] of them.
    fn write(&self, pairs: impl IntoIterator<Item = (Entity, PlayerKey)>) {
        let mut words = [0; WORDS];
        let mut len = 0;
        for ((entity, key), pair) in pairs.into_iter().zip(words[1..].as_chunks_mut::<2>().0) {
            *pair = [u64::from(entity.get()), key.0];
            len += 1;
        }
        words[0] = len;
        self.copy.write(words);
    }

    /// The account of the character `entity` in the copy; `None` for a
    /// character it does not hold, or while a write stays open.
    fn of(&self, entity: Entity) -> Option<PlayerKey> {
        if entity.get() == 0 {
            return None;
        }
        let words = self.copy.load()?;
        let len = usize::try_from(words[0]).unwrap_or(0).min(MAX_PLAYERS);
        words[1..]
            .as_chunks::<2>()
            .0
            .iter()
            .take(len)
            .find(|&&[character, _]| character == u64::from(entity.get()))
            .map(|&[_, key]| PlayerKey(key))
            .filter(|&key| key != PlayerKey::default())
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
/// is no player's in game, before the first step, or when the copy is off.
pub(crate) fn account_of(entity: Entity) -> Option<PlayerKey> {
    if FUSE.is_off() {
        return None;
    }
    STATE.accounts.of(entity)
}

/// Copies the character and account of each player in game just before
/// `world` steps, when it is the server's world on its game thread. The
/// caller checks [`wanted`] first and runs this under [`FUSE`].
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
    STATE.accounts.write(in_game(slots).map(|(slot, entity)| {
        // SAFETY: `slot` is a player slot `occupied` found in the server;
        // the table's `server.player_slot_key` offset holds its `u64`
        // account key.
        (
            entity,
            PlayerKey(unsafe { read::<u64>(slot, layout.slot_key) }),
        )
    }));
}

/// The address and character of each slot `occupied` names whose player is
/// in game, as the world's player reads list them: a joining or leaving
/// slot may still name another occupant's character.
fn in_game(
    slots: impl Iterator<Item = (usize, usize, u8, u32)>,
) -> impl Iterator<Item = (usize, Entity)> {
    slots
        .filter(|&(_, _, state, _)| state == SLOT_IN_GAME)
        .map(|(_, slot, _, entity)| (slot, Entity::from_engine(entity)))
}

/// Forgets every account, as the server's world ends; a hosted game's end
/// runs it on another thread than the step's.
pub(super) fn clear() {
    STATE.accounts.write([]);
}

/// What a build check pays to find the last of [`MAX_PLAYERS`] characters
/// in a full copy, and a server step to write one, for their benchmark.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_accounts() -> (impl FnMut() -> Option<PlayerKey>, impl FnMut()) {
    let most = u32::try_from(MAX_PLAYERS).unwrap_or(u32::MAX);
    let full = move || (1..=most).map(|n| (Entity::from_engine(n), PlayerKey(u64::from(n))));
    let read = Accounts::new();
    read.write(full());
    let last = Entity::from_engine(most);
    let written = Accounts::new();
    (move || read.of(last), move || written.write(full()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(entity: u32, key: u64) -> (Entity, PlayerKey) {
        (Entity::from_engine(entity), PlayerKey(key))
    }

    #[test]
    fn a_character_in_the_copy_names_its_account_and_any_other_none() {
        let accounts = Accounts::new();
        accounts.write([pair(7, 0x5eed), pair(9, 0xbeef)]);
        assert_eq!(accounts.of(Entity::from_engine(9)), Some(PlayerKey(0xbeef)));
        assert_eq!(accounts.of(Entity::from_engine(8)), None);
        assert_eq!(accounts.of(Entity::from_engine(0)), None);
    }

    #[test]
    fn a_new_copy_drops_the_characters_the_last_one_held() {
        let accounts = Accounts::new();
        accounts.write([pair(7, 0x5eed), pair(9, 0xbeef)]);
        accounts.write([pair(11, 0x5eed)]);
        assert_eq!(accounts.of(Entity::from_engine(7)), None);
        assert_eq!(
            accounts.of(Entity::from_engine(11)),
            Some(PlayerKey(0x5eed))
        );
    }

    #[test]
    fn a_read_while_the_copy_is_written_finds_nothing() {
        let accounts = Accounts::new();
        accounts.write([pair(7, 0x5eed)]);
        accounts.copy.begin_write();
        assert_eq!(accounts.of(Entity::from_engine(7)), None);
    }

    #[test]
    fn only_slots_whose_player_is_in_game_reach_the_copy() {
        let slots = [(0, 0x100, SLOT_IN_GAME, 7), (1, 0x200, SLOT_IN_GAME ^ 1, 9)];
        assert_eq!(
            in_game(slots.into_iter()).collect::<Vec<_>>(),
            [(0x100, Entity::from_engine(7))]
        );
    }

    #[test]
    fn a_slot_without_an_account_names_none() {
        let accounts = Accounts::new();
        accounts.write([pair(7, 0)]);
        assert_eq!(accounts.of(Entity::from_engine(7)), None);
    }

    #[test]
    fn more_slots_than_the_copy_holds_keep_the_first() {
        let accounts = Accounts::new();
        let many = (1..=u32::try_from(MAX_PLAYERS + 3).unwrap_or(u32::MAX))
            .map(|entity| pair(entity, u64::from(entity)));
        accounts.write(many);
        let last = u32::try_from(MAX_PLAYERS).unwrap_or(u32::MAX);
        assert_eq!(
            accounts.of(Entity::from_engine(last)),
            Some(PlayerKey(u64::from(last)))
        );
        assert_eq!(accounts.of(Entity::from_engine(last + 1)), None);
    }
}
