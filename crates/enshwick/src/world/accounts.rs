//! The game's account actions, kick, ban and unban by account, and the
//! loader's allowlist and chat mutes.

use crate::PlayerKey;
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
    /// fn ban_for_cheating(world: &World, key: enshwick::PlayerKey) {
    ///     if let Err(why) = world.account_action(AccountAction::Ban, key) {
    ///         enshwick::warn!("no ban for {key:016x}: {why}");
    ///     }
    /// }
    /// ```
    pub fn account_action(self, action: AccountAction, key: PlayerKey) -> Result<(), WorldError> {
        WorldError::from_status((self.0.host.api().account_action)(
            self.0.host.handle(),
            action.abi(),
            key,
        ))
    }

    /// Puts the account `key` on the loader's allowlist, or takes it off
    /// when `listed` is false. With the server's `allowlist` setting on,
    /// only listed accounts and administrators may join; the list keeps
    /// its accounts with the setting off too, and a change applies from
    /// the next join. Any thread.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for account 0 or when the list's file is
    /// not written; [`WorldError::Missing`] outside a dedicated server.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::World;
    ///
    /// fn let_in(world: &World, key: enshwick::PlayerKey) {
    ///     if let Err(why) = world.set_allowlisted(key, true) {
    ///         enshwick::warn!("{key:016x} not listed: {why}");
    ///     }
    /// }
    /// ```
    pub fn set_allowlisted(self, key: PlayerKey, listed: bool) -> Result<(), WorldError> {
        WorldError::from_status((self.0.host.api().allowlist_set)(
            self.0.host.handle(),
            key,
            u8::from(listed),
        ))
    }

    /// Whether the loader's allowlist lists the account `key`, the
    /// server's `allowlist` setting on or not. Any thread.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for account 0; [`WorldError::Missing`]
    /// outside a dedicated server.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::World;
    ///
    /// fn may_join(world: &World, key: enshwick::PlayerKey) -> bool {
    ///     world.allowlisted(key).unwrap_or(false)
    /// }
    /// ```
    pub fn allowlisted(self, key: PlayerKey) -> Result<bool, WorldError> {
        let mut listed = 0u8;
        WorldError::from_status((self.0.host.api().allowlist_has)(
            self.0.host.handle(),
            key,
            &raw mut listed,
        ))?;
        Ok(listed == 1)
    }

    /// Mutes the account `key` in chat, or lifts its mute when `muted` is
    /// false. The server then drops every line that player types before
    /// anyone else reads it, and tells the player now and then with one of
    /// the game's notices; a mute lasts across restarts until it is lifted.
    /// The mod decides who may ask. Any thread.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for account 0, for a mute of an administrator
    /// online or of a hosted game's host, and when the list's file is not
    /// written; [`WorldError::Missing`] outside a server, the dedicated one
    /// or the one a client hosts.
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
