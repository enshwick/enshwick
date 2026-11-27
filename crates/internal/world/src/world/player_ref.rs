//! One player as a call names them: by account, by character, or both.

use crate::script::{Error, FromScript, IntoScript, Ty, Typed, Value};
use crate::world::Player;
use crate::{Entity, Host, PlayerKey, WorldError};

/// One player as a call takes them: by account key, by character, or a
/// [`Player`] the mod read, which carries both.
///
/// A call that needs the half it was not given finds it among the connected
/// players, so a [`PlayerKey`] reaches a player who is offline only where
/// the call says so, as `Knowledge::grant_account` does, and an [`Entity`]
/// reaches only a connected player.
///
/// In Luau a player is the key as a decimal string, the character as a
/// number, or a `Player` table.
///
/// # Examples
///
/// ```
/// use enshwick::world::PlayerRef;
/// use enshwick::PlayerKey;
///
/// let account = PlayerRef::from(PlayerKey::new(42));
/// assert_eq!((account.key(), account.entity()), (Some(PlayerKey::new(42)), None));
/// assert_eq!(PlayerRef::from(PlayerKey::new(0)).key(), None);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerRef {
    key: Option<PlayerKey>,
    entity: Option<Entity>,
}

impl PlayerRef {
    /// The account's key, when the player was named by it.
    ///
    /// # Examples
    ///
    /// See [`PlayerRef`].
    #[must_use]
    pub const fn key(self) -> Option<PlayerKey> {
        self.key
    }

    /// The character, when the player was named by it.
    ///
    /// # Examples
    ///
    /// See [`PlayerRef`].
    #[must_use]
    pub const fn entity(self) -> Option<Entity> {
        self.entity
    }

    /// The account's key, from the connected player when only the
    /// character was given; never key 0, which names no account.
    pub(crate) fn key_in(self, host: Host) -> Result<PlayerKey, WorldError> {
        match (self.key, self.entity) {
            (Some(key), _) => Ok(key),
            (None, Some(entity)) => connected(host, |player| player.entity == entity)?
                .key
                .ok_or(WorldError::Gone),
            (None, None) => Err(WorldError::Gone),
        }
    }

    /// The character, from the connected player when only the key was
    /// given; never the engine's 0.
    pub(crate) fn entity_in(self, host: Host) -> Result<Entity, WorldError> {
        match (self.entity, self.key) {
            (Some(entity), _) => Ok(entity),
            (None, Some(key)) => connected(host, |player| player.key == key)?
                .entity
                .ok_or(WorldError::Gone),
            (None, None) => Err(WorldError::Gone),
        }
    }
}

/// The connected player `test` picks, with the halves that name no one
/// left out; [`WorldError::Gone`] when none is.
fn connected(host: Host, test: impl Fn(&Player) -> bool) -> Result<PlayerRef, WorldError> {
    let players = crate::world::listed(host).map_err(|status| {
        WorldError::from_status(status)
            .err()
            .unwrap_or(WorldError::Unavailable)
    })?;
    players
        .iter()
        .find(|player| test(player))
        .map(PlayerRef::from)
        .ok_or(WorldError::Gone)
}

impl From<PlayerKey> for PlayerRef {
    fn from(key: PlayerKey) -> Self {
        Self {
            key: (key.get() != 0).then_some(key),
            entity: None,
        }
    }
}

impl From<Entity> for PlayerRef {
    fn from(entity: Entity) -> Self {
        Self {
            key: None,
            entity: (entity != Entity::NONE).then_some(entity),
        }
    }
}

impl From<&Player> for PlayerRef {
    fn from(player: &Player) -> Self {
        Self {
            key: Self::from(player.key).key,
            entity: Self::from(player.entity).entity,
        }
    }
}

impl From<Player> for PlayerRef {
    fn from(player: Player) -> Self {
        Self::from(&player)
    }
}

impl Typed for PlayerRef {
    const TY: &'static Ty = &Ty::Data;
}

impl IntoScript for PlayerRef {
    fn into_value(self) -> Value {
        match (self.key, self.entity) {
            (Some(key), _) => key.into_value(),
            (None, Some(entity)) => entity.into_value(),
            (None, None) => Value::Nil,
        }
    }
}

impl FromScript for PlayerRef {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::String(_) => PlayerKey::from_value(value).map(Self::from),
            Value::Number(_) => Entity::from_value(value).map(Self::from),
            Value::Map(fields) => {
                let mut player = Self {
                    key: None,
                    entity: None,
                };
                for (name, value) in fields {
                    match name.as_str() {
                        "key" => player.key = Self::from(PlayerKey::from_value(value)?).key,
                        "entity" => player.entity = Self::from(Entity::from_value(value)?).entity,
                        _ => {}
                    }
                }
                Ok(player)
            }
            _ => Err(Error::new(
                "a player is a key as a string, a character as a number or a Player",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_a_character_and_a_player_name_what_they_carry() {
        let player = PlayerRef::from(PlayerKey::new(5));
        assert_eq!(
            (player.key(), player.entity()),
            (Some(PlayerKey::new(5)), None)
        );
        let player = PlayerRef::from(Entity::from_engine(9));
        assert_eq!(
            (player.key(), player.entity()),
            (None, Some(Entity::from_engine(9)))
        );
        assert_eq!(PlayerRef::from(PlayerKey::new(0)).key(), None);
        assert_eq!(PlayerRef::from(Entity::NONE).entity(), None);
    }

    #[test]
    fn a_script_names_a_player_by_key_character_or_table() -> Result<(), Error> {
        let key = PlayerRef::from_value(Value::String("42".into()))?;
        assert_eq!(key.key(), Some(PlayerKey::new(42)));
        let character = PlayerRef::from_value(Value::Number(7.0))?;
        assert_eq!(character.entity(), Some(Entity::from_engine(7)));
        let both = PlayerRef::from_value(Value::Map(vec![
            ("entity".into(), Value::Number(7.0)),
            ("key".into(), Value::String("42".into())),
            ("name".into(), Value::String("Ann".into())),
        ]))?;
        assert_eq!((both.key(), both.entity()), (key.key(), character.entity()));
        assert!(PlayerRef::from_value(Value::Bool(true)).is_err());
        Ok(())
    }
}
