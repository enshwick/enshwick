//! Mods' changes to players' permissions, kept per mod and account: each
//! call replaces the mod's own change, and a change ends when its mod ends
//! it, the player leaves or the world ends. The step applies what this
//! module publishes ([`crate::engine::world::permissions`]), skipping the
//! changes of a mod that is off.

use std::sync::Mutex;

use enshwick_base::Relock;
use enshwick_core::abi::PlayerInfo;

use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::world::WorldError;
use crate::engine::world::permissions::{Change, in_game, publish, writes};

/// The mods' changes, in the order made.
#[derive(Debug)]
struct Changes(Mutex<Vec<Change>>);

impl Changes {
    const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    /// Changes the list with `edit` and hands the step the result.
    fn edit(&self, edit: impl FnOnce(&mut Vec<Change>)) {
        let mut changes = self.0.relock();
        edit(&mut changes);
        publish(changes.clone());
    }

    /// Replaces the change of `change.owner` to `change.key`; an empty one
    /// ends it.
    fn set(&self, change: Change) {
        self.edit(|changes| {
            changes.retain(|c| (c.owner, c.key) != (change.owner, change.key));
            if change.deny != 0 || change.grant != 0 {
                changes.push(change);
            }
        });
    }

    /// Ends every change to the account `key`.
    fn left(&self, key: u64) {
        self.edit(|changes| changes.retain(|c| c.key != key));
    }

    #[cfg(test)]
    fn all(&self) -> Vec<Change> {
        self.0.relock().clone()
    }
}

static CHANGES: Changes = Changes::new();

/// Checks a change's account and bits.
///
/// # Errors
///
/// [`PermissionError::BadBits`] for account 0 and a bit past the game's
/// permissions.
fn checked(owner: ModIndex, key: u64, deny: u32, grant: u32) -> Result<Change, PermissionError> {
    let (Ok(deny), Ok(grant)) = (u8::try_from(deny), u8::try_from(grant)) else {
        return Err(PermissionError::BadBits);
    };
    if key == 0 || u32::from(deny | grant) & !PlayerInfo::ALL_PERMISSIONS != 0 {
        return Err(PermissionError::BadBits);
    }
    Ok(Change {
        owner,
        key,
        deny,
        grant,
    })
}

/// Replaces mod `owner`'s change to the permissions of the player with
/// account `key`; `deny` and `grant` both 0 ends it, whether or not the
/// player is in game.
///
/// # Errors
///
/// [`PermissionError::BadBits`] for key 0 and a bit past the game's
/// permissions; [`PermissionError::NotInGame`] for a new change to an
/// account no player in game has; [`WorldError::Unread`] on a build without
/// the step write or the player removal link, [`WorldError::NoWorld`] while
/// no server runs and [`WorldError::WrongThread`] off its game thread.
pub(crate) fn change(
    loader: &Loader,
    owner: ModIndex,
    key: u64,
    deny: u32,
    grant: u32,
) -> Result<(), PermissionError> {
    let change = checked(owner, key, deny, grant)?;
    if change.deny == 0 && change.grant == 0 {
        CHANGES.set(change);
        return Ok(());
    }
    if !writes() {
        return Err(WorldError::Unread.into());
    }
    if !in_game(loader, key)? {
        return Err(PermissionError::NotInGame);
    }
    CHANGES.set(change);
    Ok(())
}

/// Why a mod's change to a player's permissions was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PermissionError {
    /// Key 0, or a bit past the game's permissions.
    #[error("no account, or a bit past the game's permissions")]
    BadBits,
    /// No player in game has the account.
    #[error("no player in game has the account")]
    NotInGame,
    /// The world cannot take the change now.
    #[error(transparent)]
    World(#[from] WorldError),
}

/// Ends every mod's change to the account `key`, whose player left.
pub(crate) fn left(key: u64) {
    CHANGES.left(key);
}

/// Ends every change, as the server's world ends.
pub(crate) fn forget() {
    CHANGES.edit(Vec::clear);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANN: u64 = 7;
    const BOB: u64 = 9;

    fn change(owner: usize, key: u64, deny: u32, grant: u32) -> Change {
        checked(ModIndex::new(owner), key, deny, grant).unwrap()
    }

    #[test]
    fn a_mod_s_new_change_replaces_its_own_and_an_empty_one_ends_it() {
        let changes = Changes::new();
        changes.set(change(0, ANN, PlayerInfo::CAN_EDIT_WORLD, 0));
        changes.set(change(0, ANN, PlayerInfo::CAN_EDIT_BASE, 0));
        changes.set(change(1, ANN, PlayerInfo::CAN_KICK_BAN, 0));
        assert_eq!(
            changes.all(),
            [
                change(0, ANN, PlayerInfo::CAN_EDIT_BASE, 0),
                change(1, ANN, PlayerInfo::CAN_KICK_BAN, 0)
            ]
        );
        changes.set(change(0, ANN, 0, 0));
        changes.set(change(1, ANN, 0, 0));
        assert!(changes.all().is_empty());
    }

    #[test]
    fn a_player_who_leaves_takes_every_mod_s_change_with_them() {
        let changes = Changes::new();
        changes.set(change(0, ANN, PlayerInfo::CAN_EDIT_WORLD, 0));
        changes.set(change(1, ANN, 0, PlayerInfo::CAN_EDIT_BASE));
        changes.set(change(1, BOB, PlayerInfo::CAN_EDIT_WORLD, 0));
        changes.left(ANN);
        assert_eq!(
            changes.all(),
            [change(1, BOB, PlayerInfo::CAN_EDIT_WORLD, 0)]
        );
    }

    #[test]
    fn a_change_names_an_account_and_only_the_game_s_six_bits() {
        let owner = ModIndex::new(0);
        assert_eq!(checked(owner, 0, 0, 0), Err(PermissionError::BadBits));
        for bad in [0x40, 0x80, 0x100] {
            assert_eq!(checked(owner, ANN, bad, 0), Err(PermissionError::BadBits));
            assert_eq!(checked(owner, ANN, 0, bad), Err(PermissionError::BadBits));
        }
        assert!(
            checked(
                owner,
                ANN,
                PlayerInfo::ALL_PERMISSIONS,
                PlayerInfo::ALL_PERMISSIONS
            )
            .is_ok()
        );
    }
}
