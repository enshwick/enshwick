use crate::abi::Str;
use crate::i18n::Language;
use crate::{Entity, PlayerKey};

use super::{Notice, Player, PlayerRef, World, blank_info, online};
use crate::WorldError;

/// Where a player respawns, from [`World::respawn_point`].
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// See [`World::respawn_point`].
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct RespawnPoint {
    /// The spawn point entity.
    pub entity: Entity,
    /// Where the spawn point stands and which way it faces.
    pub transform: super::WorldTransform,
    /// What kind of spawn point it is.
    pub kind: PlayerSpawnType,
}

/// The kind of a spawn point, the game's `PlayerSpawnType`.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// See [`World::respawn_point`].
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum PlayerSpawnType {
    /// The world's start location.
    StartLocation,
    /// A save point out in the world, such as an Ancient Spire's.
    SavePoint,
    /// A Flame Altar's base.
    Base,
    /// A spawn point nobody respawns at.
    NoRespawn,
    /// A kind this crate does not name.
    Other,
}

#[cfg(feature = "unstable")]
impl PlayerSpawnType {
    /// The kind a [`RespawnPointInfo`](crate::abi::RespawnPointInfo) names.
    const fn from_abi(kind: u32) -> Self {
        use crate::abi::RespawnPointInfo as Info;
        match kind {
            Info::START_LOCATION => Self::StartLocation,
            Info::SAVE_POINT => Self::SavePoint,
            Info::BASE => Self::Base,
            Info::NO_RESPAWN => Self::NoRespawn,
            _ => Self::Other,
        }
    }
}

#[crate::script(players)]
impl World {
    /// The connected player whose character is `entity`.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    #[script]
    pub fn player(&self, entity: Entity) -> Option<Player> {
        let mut info = blank_info();
        let status = (self.0.host.api().player)(self.0.host.handle(), entity, &raw mut info);
        status.is_ok().then(|| Player::from_info(&info))
    }

    /// The connected player with the account `key`, the same in every
    /// session.
    ///
    /// # Examples
    ///
    /// ```
    /// fn owner_name(world: &enshwick::world::World, owner: enshwick::PlayerKey) -> Option<String> {
    ///     world.player_by_key(owner).map(|player| player.name)
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn player_by_key(self, key: PlayerKey) -> Option<Player> {
        self.players().into_iter().find(|player| player.key == key)
    }

    /// The connected players named `name`, ignoring case; names are
    /// untrusted and may repeat, so a command that wants one player refuses
    /// more than one.
    ///
    /// # Examples
    ///
    /// ```
    /// fn find(world: &enshwick::world::World, name: &str) -> Result<enshwick::world::Player, String> {
    ///     match <[_; 1]>::try_from(world.players_named(name)) {
    ///         Ok([player]) => Ok(player),
    ///         Err(found) if found.is_empty() => Err(format!("no player is named {name}")),
    ///         Err(_) => Err(format!("several players are named {name}")),
    ///     }
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn players_named(self, name: &str) -> Vec<Player> {
        let mut players = self.players();
        players.retain(|player| player.is_named(name));
        players
    }

    /// Gives `entity` the name `text`, as a player's rename does: the game
    /// shows it on hover, sends it to every client and saves it with the
    /// world. The game keeps 255 bytes; an empty text removes the name.
    ///
    /// # Errors
    ///
    /// [`WorldError::Gone`] for an entity not in the world,
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Unavailable`] where no server runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events::BuildingPlaced;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     load.on::<BuildingPlaced>(move |placed| {
    ///         if let Some(player) = world.player(placed.character) {
    ///             let label = format!("Built by {}", player.name);
    ///             let _labelled = world.set_label(placed.entity, &label);
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    pub fn set_label(self, entity: Entity, text: &str) -> Result<(), WorldError> {
        let status = (self.0.host.api().set_label)(self.0.host.handle(), entity, Str::new(text));
        WorldError::check(status, WorldError::Gone)
    }

    /// Sends `text` to every player's chat, after the next step; callable
    /// from any thread. Each line becomes a message of its own.
    ///
    /// # Examples
    ///
    /// ```
    /// fn announce(world: &enshwick::world::World) {
    ///     world.broadcast("The server restarts in five minutes");
    /// }
    /// ```
    #[script]
    pub fn broadcast(self, text: &str) {
        let _queued = (self.0.host.api().chat_all)(self.0.host.handle(), Str::new(text));
    }

    /// The language the player of account `key` reads the game in, as their
    /// game named it when they joined, for a mod's own texts in a notice or
    /// line to that player outside an action's request. In a game hosted
    /// from a client, the host's own player reads the game's language. The
    /// fallback for a player whose game runs no Enshwick, for a key no
    /// connected player holds and on a client that joined a server. Any
    /// thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::Texts;
    ///
    /// fn warn_everyone(world: &enshwick::world::World, texts: &Texts) {
    ///     for player in world.players() {
    ///         let text = texts.fill("restart", world.language_of(player.key), &[]);
    ///         let _how = world.notify(&player, &text);
    ///     }
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn language_of(self, key: PlayerKey) -> Language {
        let mut language = 0;
        let _ = (self.0.host.api().language_of)(self.0.host.handle(), key, &raw mut language);
        Language::known_or_fallback(language)
    }

    /// Sends `text` to `player`'s chat alone, after the next step; callable
    /// from any thread for a player named by account. A player named by
    /// character alone is found on the game thread only. The console and a
    /// remote caller have no account, so a line for them goes to no player,
    /// with a log line: answer them in the action's outcome. So does a line
    /// for a character no connected player has, or one named off the game
    /// thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn welcome(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     world.tell(player, "Welcome back");
    /// }
    /// ```
    #[script]
    pub fn tell(self, player: impl Into<PlayerRef>, text: &str) {
        match player.into().key_in(self.0.host) {
            Ok(key) => self.chat(key, text),
            Err(WorldError::WrongThread) => crate::warn_once!(
                "a chat line for a player named by character alone goes to no player off the game thread: a character is found only on the game thread, so name the player by account"
            ),
            Err(_) => crate::warn_once!(
                "a chat line for the console, a remote caller or a player who left goes to no player: answer the console and remote callers in the action's outcome"
            ),
        }
    }

    /// Shows `text` on `player`'s screen, as the game shows its own "can't
    /// do that" messages, or tells it in chat when the player's client lacks
    /// Enshwick to show it; says which. The game shows 255 bytes; keep it to
    /// a line. On the game thread; elsewhere it goes to chat, and to no
    /// player when the player is named by character alone. A notice for
    /// the console or a remote caller goes to no player, as with
    /// [`tell`](World::tell), and answers [`Notice::Nobody`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::Notice;
    ///
    /// fn warn_low_fuel(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     if world.notify(player, "The forge runs out of fuel") == Notice::Chat {
    ///         enshwick::debug!("{} has no Enshwick client", player.name);
    ///     }
    /// }
    /// ```
    #[script]
    pub fn notify(self, player: impl Into<PlayerRef>, text: &str) -> Notice {
        let player = player.into();
        let Ok(key) = player.key_in(self.0.host) else {
            self.tell(player, text);
            return Notice::Nobody;
        };
        if (self.0.host.api().notify)(self.0.host.handle(), key, Str::new(text)).is_ok() {
            return Notice::OnScreen;
        }
        self.chat(key, text);
        Notice::Chat
    }

    /// Sends a notice to every connected player, as [`notify`](World::notify)
    /// does to one.
    ///
    /// # Examples
    ///
    /// ```
    /// fn storm(world: &enshwick::world::World) {
    ///     world.notify_all("A storm is coming");
    /// }
    /// ```
    #[script]
    pub fn notify_all(self, text: &str) {
        for player in self.players() {
            self.notify(&player, text);
        }
    }

    pub(super) fn chat(self, to: PlayerKey, text: &str) {
        let _queued = (self.0.host.api().chat)(self.0.host.handle(), to, Str::new(text));
    }

    /// Where `player` respawns: the spawn point their character holds, its
    /// kind and where it stands; `None` while the point is not in the world,
    /// where the game falls back to the world's start, or the player has
    /// left. The game moves it when the character stands in another spawn
    /// point, and [`PlayerSpawnInfoChangeEvent`](crate::game_events::PlayerSpawnInfoChangeEvent)
    /// tells when.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the server's game thread;
    /// [`WorldError::Unavailable`] where no server runs or on a build
    /// without the layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::{PlayerSpawnType, World};
    ///
    /// fn respawns_at_a_base(world: World, player: &enshwick::world::Player) -> bool {
    ///     matches!(world.respawn_point(player), Ok(Some(point)) if point.kind == PlayerSpawnType::Base)
    /// }
    /// ```
    #[script]
    #[cfg(feature = "unstable")]
    pub fn respawn_point(
        self,
        player: impl Into<PlayerRef>,
    ) -> Result<Option<RespawnPoint>, WorldError> {
        let player = match player.into().entity_in(self.0.host) {
            Err(WorldError::Gone) => return Ok(None),
            found => found?,
        };
        let mut info = enshwick_abi::headed!(crate::abi::RespawnPointInfo {
            entity: Entity::from_engine(0),
            kind: crate::abi::RespawnPointInfo::OTHER,
            position: [0.0; 3],
            orientation: [0.0; 4],
        });
        let status = (self.0.host.api().respawn_point)(self.0.host.handle(), player, &raw mut info);
        if status == crate::abi::Status::MISSING {
            return Ok(None);
        }
        WorldError::from_status(status)?;
        Ok(Some(RespawnPoint {
            entity: info.entity,
            transform: super::WorldTransform {
                position: info.position,
                orientation: info.orientation,
            },
            kind: PlayerSpawnType::from_abi(info.kind),
        }))
    }

    /// Makes `spawn_point` where `player` respawns, as the game does when
    /// their character stands in it, and raises the game's
    /// [`PlayerSpawnInfoChangeEvent`](crate::game_events::PlayerSpawnInfoChangeEvent)
    /// with the point's base, else its scene; a point that names neither
    /// raises none, as in the game. While the character stands in another
    /// spawn point, the game sets that one back at its next step. A query by
    /// the spawn points' component lists them. On the server's game thread
    /// between steps.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`WorldError::NotASpawnPoint`] for an entity that is no spawn point,
    /// or one of kind [`PlayerSpawnType::NoRespawn`];
    /// [`WorldError::Gone`] for a player no longer in game;
    /// [`WorldError::WrongThread`] off the game thread or inside a step;
    /// [`WorldError::Unavailable`] where no server runs or on a build
    /// without the layout.
    ///
    /// # Examples
    ///
    /// ```
    /// fn send_home(world: enshwick::world::World, player: &enshwick::world::Player, altar: enshwick::Entity) {
    ///     if let Err(why) = world.set_respawn_point(player, altar) {
    ///         enshwick::warn!("{} keeps their respawn point: {why}", player.name);
    ///     }
    /// }
    /// ```
    #[script]
    #[cfg(feature = "unstable")]
    pub fn set_respawn_point(
        self,
        player: impl Into<PlayerRef>,
        spawn_point: Entity,
    ) -> Result<(), WorldError> {
        let player = player.into().entity_in(self.0.host)?;
        let status =
            (self.0.host.api().set_respawn_point)(self.0.host.handle(), player, spawn_point);
        if status == crate::abi::Status::BAD_ARGS {
            return Err(WorldError::NotASpawnPoint);
        }
        WorldError::check(status, WorldError::Gone)
    }

    /// Every connected player with a character in the world.
    ///
    /// # Examples
    ///
    /// ```
    /// fn count(world: &enshwick::world::World) -> usize {
    ///     world.players().len()
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn players(self) -> Vec<Player> {
        online(self.0.host)
    }
}
