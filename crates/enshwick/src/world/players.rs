use crate::abi::Str;
use crate::ecs::Entity;

use super::{Notice, Player, World, blank_info, online};

impl World {
    /// The connected player whose character is `entity`.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub fn player(&self, entity: Entity) -> Option<Player> {
        let mut info = blank_info();
        let status = (self.0.host.api().player)(self.0.host.handle(), entity.0, &raw mut info);
        status.is_ok().then(|| Player::from_info(&info))
    }

    /// The connected player with the account `key`, the same in every
    /// session.
    ///
    /// # Examples
    ///
    /// ```
    /// fn owner_name(world: &enshwick::world::World, owner: u64) -> Option<String> {
    ///     world.player_by_key(owner).map(|player| player.name)
    /// }
    /// ```
    #[must_use]
    pub fn player_by_key(self, key: u64) -> Option<Player> {
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
    pub fn players_named(self, name: &str) -> Vec<Player> {
        let mut players = self.players();
        players.retain(|player| player.is_named(name));
        players
    }

    /// Gives `entity` the name `text`, as a player's rename does: the game
    /// shows it on hover, sends it to every client and saves it with the
    /// world. The game keeps 255 bytes; an empty text removes the name. On
    /// the game thread only; `false` elsewhere or when the entity is gone.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events::BuildingPlaced;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     load.on::<BuildingPlaced>(move |placed| {
    ///         if let Some(player) = world.player(enshwick::ecs::Entity(placed.player)) {
    ///             let label = format!("Built by {}", player.name);
    ///             let _labelled = world.set_label(enshwick::ecs::Entity(placed.entity), &label);
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub fn set_label(self, entity: Entity, text: &str) -> bool {
        (self.0.host.api().set_label)(self.0.host.handle(), entity.0, Str::new(text)).is_ok()
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
    pub fn broadcast(self, text: &str) {
        self.chat(0, text);
    }

    /// Sends `text` to `player`'s chat alone, after the next step; callable
    /// from any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn welcome(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     world.tell(player, "Welcome back");
    /// }
    /// ```
    pub fn tell(self, player: &Player, text: &str) {
        if player.key != 0 {
            self.chat(player.key, text);
        }
    }

    /// Shows `text` on `player`'s screen, as the game shows its own "can't
    /// do that" messages, or tells it in chat when the player's client lacks
    /// Enshwick to show it; says which. The game shows 255 bytes; keep it to
    /// a line. On the game thread; elsewhere it goes to chat.
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
    #[expect(
        clippy::must_use_candidate,
        reason = "shown is the point; how is for the curious"
    )]
    pub fn notify(self, player: &Player, text: &str) -> Notice {
        if (self.0.host.api().notify)(self.0.host.handle(), player.key, Str::new(text)).is_ok() {
            return Notice::OnScreen;
        }
        self.tell(player, text);
        Notice::Chat
    }

    /// [`World::notify`] for every connected player.
    ///
    /// # Examples
    ///
    /// ```
    /// fn storm(world: &enshwick::world::World) {
    ///     world.notify_all("A storm is coming");
    /// }
    /// ```
    pub fn notify_all(self, text: &str) {
        for player in self.players() {
            self.notify(&player, text);
        }
    }

    pub(super) fn chat(self, to: u64, text: &str) {
        let _queued = (self.0.host.api().chat)(self.0.host.handle(), to, Str::new(text));
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
    pub fn players(self) -> Vec<Player> {
        online(self.0.host)
    }
}
