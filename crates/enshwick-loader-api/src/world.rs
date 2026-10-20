//! Reads of the server's world: components, templates, positions and
//! players; entity labels and chat lines. Call them on the server's game thread, from a game event or a
//! [`Scheduler`](crate::schedule::Scheduler) task; elsewhere they return
//! `None` or nothing. On a client the server is the built-in one, while it
//! hosts a game.

use core::ffi::c_void;

use crate::abi::{Header, HostApi, PlayerInfo, Status, Str};
use crate::ecs::Entity;
use crate::{Host, Load, Off};

/// A reflected ECS component type, resolved once at init.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let world = load.world()?;
///     let used_item = world.component_type(load, "keen::ecs::UsedItem")?;
///     let _ = used_item;
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComponentType(u32);

impl ComponentType {
    /// The type's hash, as [`Components`](crate::ecs::Components) takes it.
    ///
    /// # Examples
    ///
    /// See [`ComponentType`].
    #[must_use]
    pub const fn hash(self) -> u32 {
        self.0
    }
}

/// A connected player, as the server sees them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    /// The player character's entity.
    pub entity: Entity,
    /// The server's player slot.
    pub slot: u32,
    /// The same for the same account in every session.
    pub key: u64,
    /// Permission bits, `PlayerInfo::CAN_*`.
    pub permissions: u8,
    /// The name the client sent; untrusted, may repeat between players.
    pub name: String,
}

impl Player {
    pub(crate) fn from_info(info: &PlayerInfo) -> Self {
        let name = &info.name[..usize::from(info.name_len).min(info.name.len())];
        Self {
            entity: Entity(info.entity),
            slot: info.slot,
            key: info.key,
            permissions: info.permissions,
            name: String::from_utf8_lossy(name).into_owned(),
        }
    }

    /// Whether the player's name is `name`, ignoring case, as a player types
    /// another's name in chat.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    /// use enshwick_loader_api::world::Player;
    ///
    /// let player = Player { entity: Entity(2), slot: 0, key: 7, permissions: 0, name: "Émile".into() };
    /// assert!(player.is_named("éMILE"));
    /// assert!(!player.is_named("Emile"));
    /// ```
    #[must_use]
    pub fn is_named(&self, name: &str) -> bool {
        self.name.to_lowercase() == name.to_lowercase()
    }

    /// Whether the server lets the player kick and ban: its administrators.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub const fn is_admin(&self) -> bool {
        self.permissions & PlayerInfo::CAN_KICK_BAN != 0
    }
}

/// A [`PlayerInfo`] for the loader to fill, its header naming this build's
/// layout.
fn blank_info() -> PlayerInfo {
    PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: 0,
        entity: 0,
        key: 0,
        permissions: 0,
        name_len: 0,
        name: [0; 64],
    }
}

/// The world reads; get it from [`Load::world`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::events::PlayerJoined;
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let world = load.world()?;
///     load.on::<PlayerJoined>(move |joined| {
///         let entity = enshwick_loader_api::ecs::Entity(joined.entity);
///         if let Some(player) = world.player(entity) {
///             enshwick_loader_api::info!("{} joined, admin: {}", player.name, player.is_admin());
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct World {
    host: Host,
    transform: ComponentType,
    used_item: ComponentType,
}

impl World {
    pub(crate) fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let host = load.host();
        let end = core::mem::offset_of!(HostApi, players) + size_of::<usize>();
        if !host.api().header.covers(end) {
            return Err(Off::new("world reads need a newer loader"));
        }
        let mut world = Self {
            host,
            transform: ComponentType(0),
            used_item: ComponentType(0),
        };
        world.transform = world.component_type(load, "keen::ecs::CurrentTransform")?;
        world.used_item = world.component_type(load, "keen::ecs::UsedItem")?;
        Ok(world)
    }

    /// The component type named `name`, such as `keen::ecs::UsedItem`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the type is not reflected on this build.
    ///
    /// # Examples
    ///
    /// See [`ComponentType`].
    pub fn component_type(&self, _load: &Load<'_>, name: &str) -> Result<ComponentType, Off> {
        let mut hash = 0;
        let status = (self.host.api().type_hash)(Str::new(name), &raw mut hash);
        if status.is_ok() {
            Ok(ComponentType(hash))
        } else {
            Err(Off::new(format!(
                "component type {name}: status {}",
                status.0
            )))
        }
    }

    /// A pointer to `entity`'s component of type `ty`, valid until the game
    /// thread returns to the game; `None` when the entity has none.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub fn component(&self, entity: Entity, ty: ComponentType) -> Option<*const c_void> {
        let mut out: *const c_void = core::ptr::null();
        let status = (self.host.api().component)(self.host.handle(), entity.0, ty.0, &raw mut out);
        (status == Status::OK && !out.is_null()).then_some(out)
    }

    /// `entity`'s position in metres.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub fn position(&self, entity: Entity) -> Option<[f64; 3]> {
        let data = self.component(entity, self.transform)?;
        // SAFETY: the entity's live CurrentTransform, read at once.
        Some(unsafe { crate::ecs::transform_position(data.cast()) })
    }

    /// The item `entity` was placed from, which it keeps across saves.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub fn used_item(&self, entity: Entity) -> Option<u32> {
        let data = self.component(entity, self.used_item)?.cast::<u32>();
        // SAFETY: UsedItem is the item id alone.
        Some(unsafe { data.read_unaligned() })
    }

    /// `entity`'s template GUID, in file order.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub fn template(&self, entity: Entity) -> Option<[u8; 16]> {
        let mut out = [0u8; 16];
        let status = (self.host.api().entity_template)(self.host.handle(), entity.0, &raw mut out);
        status.is_ok().then_some(out)
    }

    /// The connected player whose character is `entity`.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub fn player(&self, entity: Entity) -> Option<Player> {
        let mut info = blank_info();
        let status = (self.host.api().player)(self.host.handle(), entity.0, &raw mut info);
        status.is_ok().then(|| Player::from_info(&info))
    }

    /// The connected player with the account `key`, the same in every
    /// session.
    ///
    /// # Examples
    ///
    /// ```
    /// fn owner_name(world: &enshwick_loader_api::world::World, owner: u64) -> Option<String> {
    ///     world.player_by_key(owner).map(|player| player.name)
    /// }
    /// ```
    #[must_use]
    pub fn player_by_key(&self, key: u64) -> Option<Player> {
        self.players().into_iter().find(|player| player.key == key)
    }

    /// The connected players named `name`, ignoring case; names are
    /// untrusted and may repeat, so a command that wants one player refuses
    /// more than one.
    ///
    /// # Examples
    ///
    /// ```
    /// fn find(world: &enshwick_loader_api::world::World, name: &str) -> Result<enshwick_loader_api::world::Player, String> {
    ///     match <[_; 1]>::try_from(world.players_named(name)) {
    ///         Ok([player]) => Ok(player),
    ///         Err(found) if found.is_empty() => Err(format!("no player is named {name}")),
    ///         Err(_) => Err(format!("several players are named {name}")),
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn players_named(&self, name: &str) -> Vec<Player> {
        let mut players = self.players();
        players.retain(|player| player.is_named(name));
        players
    }

    /// Whether the loader offers what was added to the table at `end`.
    fn offers(&self, end: usize) -> bool {
        self.host.api().header.covers(end + size_of::<usize>())
    }

    /// Gives `entity` the name `text`, as a player's rename does: the game
    /// shows it on hover, sends it to every client and saves it with the
    /// world. The game keeps 255 bytes; an empty text removes the name. On
    /// the game thread only; `false` elsewhere or when the entity is gone.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::events::BuildingPlaced;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let world = load.world()?;
    ///     load.on::<BuildingPlaced>(move |placed| {
    ///         if let Some(player) = world.player(enshwick_loader_api::ecs::Entity(placed.player)) {
    ///             let label = format!("Built by {}", player.name);
    ///             let _labelled = world.set_label(enshwick_loader_api::ecs::Entity(placed.entity), &label);
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub fn set_label(&self, entity: Entity, text: &str) -> bool {
        if !self.offers(core::mem::offset_of!(HostApi, set_label)) {
            return false;
        }
        (self.host.api().set_label)(self.host.handle(), entity.0, Str::new(text)).is_ok()
    }

    /// Sends `text` to every player's chat, after the next step; callable
    /// from any thread. Each line becomes a message of its own.
    ///
    /// # Examples
    ///
    /// ```
    /// fn announce(world: &enshwick_loader_api::world::World) {
    ///     world.broadcast("The server restarts in five minutes");
    /// }
    /// ```
    pub fn broadcast(&self, text: &str) {
        self.chat(0, text);
    }

    /// Sends `text` to `player`'s chat alone, after the next step; callable
    /// from any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn welcome(world: &enshwick_loader_api::world::World, player: &enshwick_loader_api::world::Player) {
    ///     world.tell(player, "Welcome back");
    /// }
    /// ```
    pub fn tell(&self, player: &Player, text: &str) {
        if player.key != 0 {
            self.chat(player.key, text);
        }
    }

    fn chat(&self, to: u64, text: &str) {
        if self.offers(core::mem::offset_of!(HostApi, chat)) {
            let _queued = (self.host.api().chat)(self.host.handle(), to, Str::new(text));
        }
    }

    /// Every connected player with a character in the world.
    ///
    /// # Examples
    ///
    /// ```
    /// fn count(world: &enshwick_loader_api::world::World) -> usize {
    ///     world.players().len()
    /// }
    /// ```
    #[must_use]
    pub fn players(&self) -> Vec<Player> {
        const MAX: usize = 64;
        let mut infos = [blank_info(); MAX];
        let mut count = 0u32;
        let status = (self.host.api().players)(
            self.host.handle(),
            infos.as_mut_ptr(),
            u32::try_from(MAX).unwrap_or(0),
            &raw mut count,
        );
        if !status.is_ok() {
            return Vec::new();
        }
        infos
            .iter()
            .take(usize::try_from(count).unwrap_or(0).min(MAX))
            .map(Player::from_info)
            .collect()
    }
}
