//! The game's account actions, kick, ban and unban by account, and the
//! loader's chat mutes.

use crate::PlayerKey;
use crate::abi::AccountActionType;

use super::{World, WorldError};

/// What the game does to an account, as its account actions name it.
///
/// # Examples
///
/// See [`World::account_action`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum AccountAction {
    /// Removes a player online from the game.
    Kick,
    /// Puts the account of a player online on the server's ban list and
    /// removes the player.
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

#[crate::script(accounts)]
impl World {
    /// Asks the game to kick, ban or unban the account `key`, as its
    /// administrators do; the mod decides who may ask. A dedicated server
    /// keeps its ban list in `bannedAccounts` of `enshrouded_server.json`;
    /// a hosted game keeps it until it ends.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a kick or a ban of an administrator
    /// online, whom the game never removes, or of a hosted game's host, a
    /// kick or a ban of an account not online, and account 0; [`WorldError::WrongThread`] off the server's game
    /// thread or inside a step; [`WorldError::Missing`] while no server
    /// runs or on a game build that lacks the action.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::{AccountAction, World};
    ///
    /// fn ban_for_cheating(world: &World, key: enshwick::PlayerKey) {
    ///     if let Err(why) = world.account_action(AccountAction::Ban, key) {
    ///         enshwick::warn!("no ban for {key:016x}: {why}");
    ///     }
    /// }
    /// ```
    #[script]
    pub fn account_action(self, action: AccountAction, key: PlayerKey) -> Result<(), WorldError> {
        WorldError::from_status((self.0.host.api().account_action)(
            self.0.host.handle(),
            action.abi(),
            key,
        ))
    }

    /// Mutes the account `key` in chat, or lifts its mute when `muted` is
    /// false. The server then drops every line that player types before
    /// anyone else reads it, and tells the player now and then with one of
    /// the game's notices. A mute belongs to the world: it is kept beside
    /// its save and lasts across restarts until it is lifted. The mod
    /// decides who may ask. Any thread.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for account 0, for a mute of an administrator
    /// online or of a hosted game's host, and when the world's file is not
    /// written; [`WorldError::Missing`] outside a server, the dedicated one
    /// or the one a client hosts, and while no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::World;
    ///
    /// fn quiet(world: &World, key: enshwick::PlayerKey) {
    ///     if let Err(why) = world.set_muted(key, true) {
    ///         enshwick::warn!("{key:016x} not muted: {why}");
    ///     }
    /// }
    /// ```
    #[script]
    pub fn set_muted(self, key: PlayerKey, muted: bool) -> Result<(), WorldError> {
        WorldError::from_status((self.0.host.api().chat_mute_set)(
            self.0.host.handle(),
            key,
            u8::from(muted),
        ))
    }

    /// Whether the account `key` is muted in chat. Any thread.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for account 0; [`WorldError::Missing`]
    /// outside a server, the dedicated one or the one a client hosts.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::World;
    ///
    /// fn may_talk(world: &World, key: enshwick::PlayerKey) -> bool {
    ///     !world.muted(key).unwrap_or(false)
    /// }
    /// ```
    #[script]
    pub fn muted(self, key: PlayerKey) -> Result<bool, WorldError> {
        let mut muted = 0u8;
        WorldError::from_status((self.0.host.api().chat_mute_has)(
            self.0.host.handle(),
            key,
            &raw mut muted,
        ))?;
        Ok(muted == 1)
    }
}
