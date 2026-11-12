//! What the game's player slots hold, as every layer above reads them.

/// Players the loader follows at most, in the gate and the player reads;
/// the game has 16 slots.
pub(crate) const MAX_PLAYERS: usize = 64;

/// The GUID of a player's character, `keen::CharacterId` (server 399): one
/// account may have several characters on a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CharacterId(pub(crate) [u8; 16]);
