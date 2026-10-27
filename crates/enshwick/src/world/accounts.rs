//! The game's account actions: kick, ban and unban by account.

use crate::abi::AccountActionType;

use super::{World, WorldError};

/// What the game does to an account, as its account actions name it.
///
/// # Examples
///
/// See [`World::account_action`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AccountAction {
    /// Removes a player online from the game.
    Kick,
    /// Puts the account on the server's ban list, online or not, and
    /// removes its player when online.
    Ban,
    /// Takes the account off the server's ban list.
    UnBan,
}

impl AccountAction {
    const fn abi(self) -> AccountActionType {
        match self {
            Self::Kick => AccountActionType::KICK,
            Self::Ban => AccountActionType::BAN,
            Self::UnBan => AccountActionType::UN_BAN,
        }
    }
}

impl World {
    /// Asks the game to kick, ban or unban the account `key`, as its
    /// administrators do; the mod decides who may ask. A ban of an account
    /// not online goes straight onto the ban list, which a dedicated server
    /// keeps in `bannedAccounts` of `enshrouded_server.json`; a hosted
    /// game keeps it until it ends.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a kick or a ban of an administrator
    /// online, whom the game never removes, or of a hosted game's host, a
    /// kick of an account not online, a ban the game's list does not take
    /// and account 0; [`WorldError::WrongThread`] off the server's game
    /// thread or inside a step; [`WorldError::Missing`] while no server
    /// runs or on a game build that lacks the action.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::{AccountAction, World};
    ///
    /// fn ban_for_cheating(world: &World, key: u64) {
    ///     if let Err(why) = world.account_action(AccountAction::Ban, key) {
    ///         enshwick::warn!("no ban for {key:016x}: {why}");
    ///     }
    /// }
    /// ```
    pub fn account_action(self, action: AccountAction, key: u64) -> Result<(), WorldError> {
        WorldError::from_status((self.0.host.api().account_action)(
            self.0.host.handle(),
            action.abi(),
            key,
        ))
    }
}
