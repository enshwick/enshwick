//! The server's world: components, templates, positions and players,
//! component writes, entity queries, labels, chat lines, notices, destroy
//! requests and saves. Most calls
//! belong on the server's game thread, from a game event or a
//! [`Scheduler`](crate::schedule::Scheduler) task, and each says what it does
//! elsewhere. On a client the server is the built-in one, while it hosts a
//! game.

mod stats;

use core::ffi::c_void;

use crate::abi::{EntityPlace, Header, PlayerInfo, Status, Str};
use crate::ecs::Entity;
use crate::game_events::{Field, Scalar};
use crate::{Host, Load, Off};

pub use stats::{Meter, Progress, Skill, Vitals};

/// A reflected ECS component type, resolved once at init.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
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

/// A reflected component type a mod may write, resolved once at init with
/// [`World::writable_type`]: one the server owns, not a copy the game keeps
/// for clients.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let world = load.world()?;
///     let flags = world.writable_type(load, "keen::ecs::CombatFlags")?;
///     assert_eq!(flags.name(), "keen::ecs::CombatFlags");
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WritableType {
    ty: ComponentType,
    size: u32,
    name: &'static str,
}

impl WritableType {
    /// The component type, for reads.
    ///
    /// # Examples
    ///
    /// ```
    /// fn readable(world: &enshwick::world::World, flags: enshwick::world::WritableType, entity: enshwick::ecs::Entity) -> bool {
    ///     world.component(entity, flags.ty()).is_some()
    /// }
    /// ```
    #[must_use]
    pub const fn ty(self) -> ComponentType {
        self.ty
    }

    /// The reflected size: the bytes [`World::update`] hands out.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let flags = load.world()?.writable_type(load, "keen::ecs::CombatFlags")?;
    ///     enshwick::debug!("{} is {} bytes", flags.name(), flags.size());
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub const fn size(self) -> u32 {
        self.size
    }

    /// The reflected name.
    ///
    /// # Examples
    ///
    /// See [`WritableType`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The field `name` of this type, read and written as a `T` with
    /// [`World::get`] and [`World::set`].
    ///
    /// # Errors
    ///
    /// [`Off`] when the field is not reflected on this build, or a `T` at
    /// its offset would reach past the type's end.
    ///
    /// # Examples
    ///
    /// See [`World::set`].
    pub fn field<T: Scalar>(self, load: &Load<'_>, name: &str) -> Result<Field<T>, Off> {
        let offset = load.field_offset(self.name, name)?;
        Field::within(self.ty.0, self.size, offset).ok_or_else(|| {
            Off::new(format!(
                "field {}.{name}: past the end of the type",
                self.name
            ))
        })
    }
}

/// Name prefixes of the components the game derives for clients from the
/// server's own, overwriting them every step.
const DERIVED: [&str; 2] = ["keen::ecs::Network", "keen::ecs::Client"];

/// A connected player, as the server sees them.
///
/// # Examples
///
/// See [`World`].
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
    /// use enshwick::ecs::Entity;
    /// use enshwick::world::Player;
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

/// An entity with a place in the world, from [`World::entities`] and
/// [`World::near`].
///
/// # Examples
///
/// See [`World::near`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// The entity.
    pub entity: Entity,
    /// Where it is, in metres.
    pub position: [f64; 3],
}

impl Placed {
    fn from_abi(place: &EntityPlace) -> Self {
        Self {
            entity: Entity(place.entity),
            position: crate::ecs::metres(place.position),
        }
    }

    /// The distance from `point` in metres.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::Entity;
    /// use enshwick::world::Placed;
    ///
    /// let chest = Placed { entity: Entity(7), position: [3.0, 0.0, 4.0] };
    /// assert_eq!(chest.distance([0.0, 0.0, 0.0]), 5.0);
    /// ```
    #[must_use]
    pub fn distance(&self, point: [f64; 3]) -> f64 {
        let [x, y, z] = [0, 1, 2].map(|i| self.position[i] - point[i]);
        (x * x + y * y + z * z).sqrt()
    }
}

/// How a notice reached a player, from [`World::notify`].
///
/// # Examples
///
/// See [`World::notify`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Notice {
    /// On screen, as the game shows its own messages: the player's client
    /// runs Enshwick.
    OnScreen,
    /// As a chat line: the player's client lacks Enshwick.
    Chat,
}

/// Why the world refused a request.
///
/// # Examples
///
/// ```
/// use enshwick::world::WorldError;
///
/// assert_eq!(WorldError::WrongThread.to_string(), "not on the server's game thread");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WorldError {
    /// Called off the server's game thread, inside a step (from a filter
    /// or a hook on a system), or while no server runs.
    #[error("not on the server's game thread")]
    WrongThread,
    /// The entity is not in the world.
    #[error("the entity is not in the world")]
    Gone,
    /// This game build lacks the piece.
    #[error("not available on this loader or game build")]
    Missing,
    /// The world refused: a player's character, a save before the world
    /// has loaded, a write that would kill, or a copy the game keeps for
    /// clients.
    #[error("refused by the world")]
    Refused,
    /// The entity is in the world but lacks the component, such as a chest
    /// asked for its stamina.
    #[error("the entity lacks the component")]
    Lacks,
}

impl WorldError {
    /// The error the loader's `status` stands for, with `missing` for
    /// [`Status::MISSING`]: [`Self::Gone`] where the call names an entity,
    /// [`Self::Lacks`] where it names a component.
    pub(crate) fn check(status: Status, missing: Self) -> Result<(), Self> {
        match status {
            Status::OK => Ok(()),
            Status::WRONG_THREAD => Err(Self::WrongThread),
            Status::REFUSED | Status::BAD_ARGS => Err(Self::Refused),
            Status::MISSING => Err(missing),
            _ => Err(Self::Missing),
        }
    }

    /// [`Self::check`] for a call whose missing piece is the feature itself.
    pub(crate) fn from_status(status: Status) -> Result<(), Self> {
        Self::check(status, Self::Missing)
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
/// use enshwick::events::PlayerJoined;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let world = load.world()?;
///     load.on::<PlayerJoined>(move |joined| {
///         let entity = enshwick::ecs::Entity(joined.entity);
///         if let Some(player) = world.player(entity) {
///             enshwick::info!("{} joined, admin: {}", player.name, player.is_admin());
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct World {
    host: Host,
    transform: ComponentType,
    used_item: ComponentType,
    label: Option<ComponentType>,
    stats: Option<stats::Stats>,
}

impl World {
    pub(crate) fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let host = load.host();
        let mut world = Self {
            host,
            transform: ComponentType(0),
            used_item: ComponentType(0),
            label: None,
            stats: None,
        };
        world.transform = world.component_type(load, "keen::ecs::CurrentTransform")?;
        world.used_item = world.component_type(load, "keen::ecs::UsedItem")?;
        world.label = world
            .component_type(load, "keen::ecs::CustomPlayerString")
            .ok();
        world.stats = stats::Stats::resolve(load, &world)
            .inspect_err(|off| crate::debug!("stat reads off: {off}"))
            .ok();
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
        crate::load::ok(status, ComponentType(hash), || {
            format!("component type {name}")
        })
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

    /// The component type named `name` for writes, such as
    /// `keen::ecs::CombatFlags`: one the server owns. The `Network*` and
    /// `Client*` copies the game keeps for clients are refused, since the
    /// game overwrites them from the server's own every step.
    ///
    /// # Errors
    ///
    /// [`Off`] for a copy the game keeps for clients, or a type not
    /// reflected on this build.
    ///
    /// # Examples
    ///
    /// See [`WritableType`].
    pub fn writable_type(&self, load: &Load<'_>, name: &'static str) -> Result<WritableType, Off> {
        if DERIVED.iter().any(|prefix| name.starts_with(prefix)) {
            return Err(Off::new(format!(
                "component type {name}: a copy the game keeps for clients; write the server's own"
            )));
        }
        Ok(WritableType {
            ty: self.component_type(load, name)?,
            size: load.type_size(name)?,
            name,
        })
    }

    /// Lets `write` change `entity`'s component of type `ty`, its reflected
    /// bytes, and marks it changed the game's way, so the game sends it to
    /// clients and saves it; returns what `write` returns. On the game
    /// thread only. `write` changes a copy, written back when it returns, so
    /// a panic in it leaves the component as it was.
    ///
    /// # Errors
    ///
    /// [`WorldError::Gone`] for an entity not in the world,
    /// [`WorldError::Lacks`] when it lacks the component,
    /// [`WorldError::Refused`] for a type the loader will not write,
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Missing`] on a build without writes.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     let flags = world.writable_type(load, "keen::ecs::CombatFlags")?;
    ///     load.add_command("immune", "makes your character immune", move |ctx, _args| {
    ///         let Some(me) = ctx.character() else {
    ///             return "the console has no character".into();
    ///         };
    ///         // `immune` is the first byte of `CombatFlags`.
    ///         match world.update(me, flags, |bytes| bytes[0] = 1) {
    ///             Ok(()) => "immune".into(),
    ///             Err(why) => format!("not immune: {why}").into(),
    ///         }
    ///     })
    /// }
    /// ```
    pub fn update<R>(
        &self,
        entity: Entity,
        ty: WritableType,
        write: impl FnOnce(&mut [u8]) -> R,
    ) -> Result<R, WorldError> {
        let len = usize::try_from(ty.size).map_err(|_| WorldError::Missing)?;
        let data = self.component_mut(entity, ty.ty)?;
        // `write` works on a copy: it may call the world itself, which a
        // borrow of the component would alias, and a panic in it leaves the
        // component whole.
        let mut small = [0u8; 256];
        let mut large = Vec::new();
        let copy = if len <= small.len() {
            &mut small[..len]
        } else {
            large.resize(len, 0);
            &mut large[..]
        };
        // SAFETY: the entity's live component of the reflected size, which
        // the loader handed out for writing, on the game thread.
        unsafe { core::ptr::copy_nonoverlapping(data.cast::<u8>(), copy.as_mut_ptr(), len) };
        let result = write(copy);
        // Asked again: `write` may have moved or removed the component.
        let data = self.component_mut(entity, ty.ty)?;
        // SAFETY: as above.
        unsafe { core::ptr::copy_nonoverlapping(copy.as_ptr(), data.cast::<u8>(), len) };
        Ok(result)
    }

    /// The value of `field` in `entity`'s component; `None` when the entity
    /// lacks it or off the game thread.
    ///
    /// # Examples
    ///
    /// See [`World::set`].
    #[must_use]
    pub fn get<T: Scalar>(&self, entity: Entity, field: Field<T>) -> Option<T> {
        let data = self.component(entity, ComponentType(field.type_hash()))?;
        let at = usize::try_from(field.offset()).ok()?;
        let mut bytes = [0u8; 8];
        let len = size_of::<T>().min(bytes.len());
        // SAFETY: the field lies inside the component, as resolution checked,
        // and the component is live until the game thread moves on.
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.cast::<u8>().wrapping_add(at),
                bytes.as_mut_ptr(),
                len,
            );
        };
        T::from_bytes(&bytes[..len])
    }

    /// Writes `value` to `field` in `entity`'s component, marked changed the
    /// game's way. On the game thread only.
    ///
    /// # Errors
    ///
    /// As for [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     let faction = world.writable_type(load, "keen::ecs::Faction")?;
    ///     let value = faction.field::<u8>(load, "faction")?;
    ///     load.add_command("befriend", "makes an entity a player ally", move |_ctx, args| {
    ///         let Some(id) = args.first().and_then(|id| id.parse().ok()) else {
    ///             return "/befriend <entity>".into();
    ///         };
    ///         let entity = enshwick::ecs::Entity(id);
    ///         let before = world.get(entity, value);
    ///         match world.set(entity, value, 11) {
    ///             Ok(()) => format!("faction {before:?} -> 11").into(),
    ///             Err(why) => format!("{why}").into(),
    ///         }
    ///     })
    /// }
    /// ```
    pub fn set<T: Scalar>(
        &self,
        entity: Entity,
        field: Field<T>,
        value: T,
    ) -> Result<(), WorldError> {
        let data = self.component_mut(entity, ComponentType(field.type_hash()))?;
        let at = usize::try_from(field.offset()).map_err(|_| WorldError::Missing)?;
        let mut bytes = [0u8; 8];
        let len = size_of::<T>().min(bytes.len());
        if !value.to_bytes(&mut bytes[..len]) {
            return Err(WorldError::Missing);
        }
        // SAFETY: the field lies inside the component, as resolution checked;
        // the loader handed the component out for writing.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast::<u8>().wrapping_add(at), len);
        };
        Ok(())
    }

    /// The writable data of `entity`'s component of type `ty`, marked
    /// changed.
    pub(crate) fn component_mut(
        &self,
        entity: Entity,
        ty: ComponentType,
    ) -> Result<*mut c_void, WorldError> {
        let mut out: *mut c_void = core::ptr::null_mut();
        let status =
            (self.host.api().component_write)(self.host.handle(), entity.0, ty.0, &raw mut out);
        match WorldError::check(status, WorldError::Gone) {
            Ok(()) if !out.is_null() => Ok(out),
            Ok(()) => Err(WorldError::Missing),
            Err(WorldError::Gone) if self.template(entity).is_some() => Err(WorldError::Lacks),
            Err(why) => Err(why),
        }
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

    /// Whether `entity` carries a name a player or [`World::set_label`] gave
    /// it; `None` when the world cannot tell.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::Entity;
    ///
    /// fn unnamed(world: &enshwick::world::World, chest: Entity) -> bool {
    ///     world.has_label(chest) == Some(false)
    /// }
    /// ```
    #[must_use]
    pub fn has_label(&self, entity: Entity) -> Option<bool> {
        let label = self.label?;
        let Some(data) = self.component(entity, label) else {
            return Some(false);
        };
        // SAFETY: a live CustomPlayerString is its string id; 0 for none.
        Some(unsafe { data.cast::<u64>().read_unaligned() } != 0)
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
    /// fn owner_name(world: &enshwick::world::World, owner: u64) -> Option<String> {
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
    /// fn find(world: &enshwick::world::World, name: &str) -> Result<enshwick::world::Player, String> {
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
    pub fn set_label(&self, entity: Entity, text: &str) -> bool {
        (self.host.api().set_label)(self.host.handle(), entity.0, Str::new(text)).is_ok()
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
    pub fn broadcast(&self, text: &str) {
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
    pub fn tell(&self, player: &Player, text: &str) {
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
    pub fn notify(&self, player: &Player, text: &str) -> Notice {
        if (self.host.api().notify)(self.host.handle(), player.key, Str::new(text)).is_ok() {
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
    pub fn notify_all(&self, text: &str) {
        for player in self.players() {
            self.notify(&player, text);
        }
    }

    fn chat(&self, to: u64, text: &str) {
        let _queued = (self.host.api().chat)(self.host.handle(), to, Str::new(text));
    }

    /// `entity`'s health, stamina and mana; `None` when it has no health,
    /// off the game thread, or on a build without stat reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events::PlayerJoined;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     load.on::<PlayerJoined>(move |joined| {
    ///         if let Some(vitals) = world.vitals(enshwick::ecs::Entity(joined.entity)) {
    ///             enshwick::info!("joined with {} of {} health", vitals.health.current, vitals.health.max);
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub fn vitals(&self, entity: Entity) -> Option<Vitals> {
        self.stats?.vitals(self, entity)
    }

    /// Sets `entity`'s health to `value`, kept between 1 and its maximum, so
    /// a write never kills; returns the health it now has. The game sends it
    /// to clients and regenerates from it as from any other value. On the
    /// game thread only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an entity already dead: death and revival
    /// stay the game's; otherwise as for [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     load.add_command("full", "restores your health", move |ctx, _args| {
    ///         let Some(me) = ctx.character() else {
    ///             return "the console has no character".into();
    ///         };
    ///         let max = world.vitals(me).map_or(0, |v| v.health.max);
    ///         match world.set_health(me, max) {
    ///             Ok(health) => format!("{} of {} health", health.current, health.max).into(),
    ///             Err(why) => format!("{why}").into(),
    ///         }
    ///     })
    /// }
    /// ```
    pub fn set_health(&self, entity: Entity, value: i32) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Health, |_| value)
    }

    /// Adds `amount` to `entity`'s health, kept between 1 and its maximum: a
    /// negative amount takes health but never kills. Returns the health it
    /// now has. On the game thread only.
    ///
    /// # Errors
    ///
    /// As for [`World::set_health`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::game_events::LevelGained;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     load.on_game::<LevelGained>(move |gained| {
    ///         let _healed = world.heal(gained.player, 50);
    ///     })
    /// }
    /// ```
    pub fn heal(&self, entity: Entity, amount: i32) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Health, |current| {
            current.saturating_add(amount)
        })
    }

    /// Sets `entity`'s stamina to `value`, kept between 0 and its maximum;
    /// returns the stamina it now has. On the game thread only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Lacks`] for an entity without stamina; otherwise as for
    /// [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn refresh(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     let max = world.vitals(player.entity).and_then(|v| v.stamina).map_or(0, |s| s.max);
    ///     let _refreshed = world.set_stamina(player.entity, max);
    /// }
    /// ```
    pub fn set_stamina(&self, entity: Entity, value: i32) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Stamina, |_| value)
    }

    /// Sets `entity`'s mana to `value`, kept between 0 and its maximum;
    /// returns the mana it now has. On the game thread only.
    ///
    /// # Errors
    ///
    /// As for [`World::set_stamina`], for mana.
    ///
    /// # Examples
    ///
    /// ```
    /// fn drain(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     let _drained = world.set_mana(player.entity, 0);
    /// }
    /// ```
    pub fn set_mana(&self, entity: Entity, value: i32) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Mana, |_| value)
    }

    /// The meter of an item's own entity, its PIDE (see
    /// [`Meter`](crate::content::items::Meter)): the count it holds and
    /// its maximum. `None` for an entity without one, off the game thread,
    /// or on a build without the reads.
    ///
    /// # Examples
    ///
    /// ```
    /// fn charges(world: &enshwick::world::World, slot: &enshwick::inventory::Slot) -> i32 {
    ///     slot.pide.and_then(|pide| world.item_meter(pide)).map_or(0, |m| m.current)
    /// }
    /// ```
    #[must_use]
    pub fn item_meter(&self, pide: Entity) -> Option<Meter> {
        self.stats?.meter(self, pide)
    }

    /// Sets the meter of an item's own entity to what `change` makes of it,
    /// kept between 0 and its maximum, and returns it. The game saves it
    /// with the world and shows it on the item's slot. On the game thread
    /// only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Lacks`] for an entity without a meter;
    /// [`WorldError::Missing`] on a build without the writes; otherwise as
    /// for [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn spend(world: &enshwick::world::World, pide: enshwick::ecs::Entity) -> bool {
    ///     world.change_item_meter(pide, |charges| charges - 1).is_ok()
    /// }
    /// ```
    pub fn change_item_meter(
        &self,
        pide: Entity,
        change: impl FnOnce(i32) -> i32,
    ) -> Result<Meter, WorldError> {
        self.write_stat(pide, stats::Pool::Meter, change)
    }

    /// How long `entity`, a character, holds out in the shroud before it
    /// drains health, in seconds, and the most it holds; `None` for an
    /// entity without it, off the game thread, or on a build without it.
    /// It runs down in the shroud and refills outside.
    ///
    /// # Examples
    ///
    /// ```
    /// fn left(world: &enshwick::world::World, player: &enshwick::world::Player) -> i32 {
    ///     world.shroud_time(player.entity).map_or(0, |time| time.current)
    /// }
    /// ```
    #[must_use]
    pub fn shroud_time(&self, entity: Entity) -> Option<Meter> {
        self.stats?.shroud(self, entity)
    }

    /// Sets how long `entity` holds out in the shroud to what `change` makes
    /// of it, in seconds, kept between 0 and its most, and returns it;
    /// clients see it from the next step. Not saved: the game refills it
    /// outside the shroud. On the game thread only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Lacks`] for an entity without it;
    /// [`WorldError::Missing`] on a build without it; otherwise as for
    /// [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn refill(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     let _full = world.change_shroud_time(player.entity, |_| i32::MAX);
    /// }
    /// ```
    pub fn change_shroud_time(
        &self,
        entity: Entity,
        change: impl FnOnce(i32) -> i32,
    ) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Shroud, change)
    }

    /// Whether `entity`, a character, stands in the shroud where it drains
    /// them, outside the altars' reach; `None` for an entity that is no
    /// actor, off the game thread, or on a build without it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn exposed(world: &enshwick::world::World, player: &enshwick::world::Player) -> bool {
    ///     world.in_shroud(player.entity) == Some(true)
    /// }
    /// ```
    #[must_use]
    pub fn in_shroud(&self, entity: Entity) -> Option<bool> {
        self.stats?.in_shroud(self, entity)
    }

    /// The skill nodes `entity`, a character, unlocked, with their levels;
    /// `None` for an entity without skills, off the game thread, or on a
    /// build without them. The game saves them with the character.
    ///
    /// # Examples
    ///
    /// ```
    /// fn skilled(world: &enshwick::world::World, player: &enshwick::world::Player) -> usize {
    ///     world.skills(player.entity).map_or(0, |skills| skills.len())
    /// }
    /// ```
    #[must_use]
    pub fn skills(&self, entity: Entity) -> Option<Vec<Skill>> {
        self.stats?;
        stats::Stats::skills(self, entity)
    }

    fn write_stat(
        &self,
        entity: Entity,
        pool: stats::Pool,
        change: impl FnOnce(i32) -> i32,
    ) -> Result<Meter, WorldError> {
        self.stats
            .ok_or(WorldError::Missing)?
            .write(self, entity, pool, change)
    }

    /// `entity`'s level and experience; `None` for an entity without them,
    /// off the game thread, or on a build without stat reads.
    ///
    /// # Examples
    ///
    /// ```
    /// fn level(world: &enshwick::world::World, player: &enshwick::world::Player) -> u32 {
    ///     world.progress(player.entity).map_or(0, |progress| progress.level)
    /// }
    /// ```
    #[must_use]
    pub fn progress(&self, entity: Entity) -> Option<Progress> {
        self.stats?.progress(self, entity)
    }

    /// Every entity with a component of type `ty` and a place in the world,
    /// such as every bed or every chest; with [`World::transform_type`],
    /// every placed entity. The engine walks every such archetype, so call
    /// it now and then, not every step. On the game thread only; empty
    /// elsewhere.
    ///
    /// # Examples
    ///
    /// ```
    /// fn count(load: &enshwick::Load<'_>, world: &enshwick::world::World) -> Result<usize, enshwick::Off> {
    ///     let beds = world.component_type(load, "keen::ecs::NpcBed")?;
    ///     Ok(world.entities(beds).len())
    /// }
    /// ```
    #[must_use]
    pub fn entities(&self, ty: ComponentType) -> Vec<Placed> {
        let entities = self.host.api().entities;
        let mut capacity = 1024u32;
        loop {
            let mut places = vec![EntityPlace::default(); capacity as usize];
            let mut count = 0u32;
            let status = entities(
                self.host.handle(),
                ty.0,
                places.as_mut_ptr(),
                capacity,
                &raw mut count,
            );
            if !status.is_ok() {
                return Vec::new();
            }
            if count <= capacity {
                places.truncate(count as usize);
                return places.iter().map(Placed::from_abi).collect();
            }
            capacity = count;
        }
    }

    /// The entities with a component of type `ty` within `radius` metres of
    /// `point`, nearest first; with [`World::transform_type`], every placed
    /// entity there. On the game thread only; empty elsewhere.
    ///
    /// # Examples
    ///
    /// ```
    /// fn nearby(world: &enshwick::world::World, player: enshwick::ecs::Entity) -> usize {
    ///     let Some(at) = world.position(player) else { return 0 };
    ///     world.near(at, 20.0, world.transform_type()).len()
    /// }
    /// ```
    #[must_use]
    pub fn near(&self, point: [f64; 3], radius: f64, ty: ComponentType) -> Vec<Placed> {
        let mut found: Vec<(f64, Placed)> = self
            .entities(ty)
            .into_iter()
            .map(|placed| (placed.distance(point), placed))
            .filter(|&(distance, _)| distance <= radius)
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.into_iter().map(|(_, placed)| placed).collect()
    }

    /// The component type every placed entity has, `CurrentTransform`.
    ///
    /// # Examples
    ///
    /// See [`World::near`].
    #[must_use]
    pub const fn transform_type(&self) -> ComponentType {
        self.transform
    }

    /// Asks the server to destroy `entity` after the systems of the next
    /// step, as the game removes things itself: no loot and no death.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a player's character,
    /// [`WorldError::Gone`] for an entity not in the world,
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Missing`] on any other refusal.
    ///
    /// # Examples
    ///
    /// ```
    /// fn clear(world: &enshwick::world::World, drop: enshwick::ecs::Entity) {
    ///     if let Err(why) = world.destroy(drop) {
    ///         enshwick::warn!("entity {} stays: {why}", drop.0);
    ///     }
    /// }
    /// ```
    pub fn destroy(&self, entity: Entity) -> Result<(), WorldError> {
        WorldError::check(
            (self.host.api().destroy)(self.host.handle(), entity.0),
            WorldError::Gone,
        )
    }

    /// Asks the server to save the world now, as its timed saves do;
    /// [`WorldSaved`](crate::events::WorldSaved) follows once the save is on
    /// disk. A save already under way counts as asked.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] while the world may not be saved, as before
    /// it has loaded; [`WorldError::WrongThread`] off the game thread;
    /// [`WorldError::Missing`] on any other refusal.
    ///
    /// # Examples
    ///
    /// ```
    /// fn checkpoint(world: &enshwick::world::World) {
    ///     if let Err(why) = world.save() {
    ///         enshwick::warn!("no save now: {why}");
    ///     }
    /// }
    /// ```
    pub fn save(&self) -> Result<(), WorldError> {
        WorldError::from_status((self.host.api().save_world)(self.host.handle()))
    }

    /// The key of the world the server runs: its save file's name and a hash
    /// of its folder, such as `world0-1a2b3c4d`, the same in every session;
    /// `None` before the server has read the world. On a client it is the key of the game it hosts,
    /// and `None` once that game ends. Any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn folder(world: &enshwick::world::World) -> Option<String> {
    ///     world.key().map(|key| format!("worlds/{key}"))
    /// }
    /// ```
    #[must_use]
    pub fn key(&self) -> Option<String> {
        world_key(self.host)
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
    pub fn players(&self) -> Vec<Player> {
        online(self.host)
    }
}

/// The players connected now, from the loader.
pub(crate) fn online(host: Host) -> Vec<Player> {
    const MAX: usize = 64;
    let mut infos = [blank_info(); MAX];
    let mut count = 0u32;
    let status = (host.api().players)(
        host.handle(),
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

/// The key of the world the server runs, from the loader.
pub(crate) fn world_key(host: Host) -> Option<String> {
    let api = host.api();
    let mut out = Str::EMPTY;
    if !(api.world_key)(host.handle(), &raw mut out).is_ok() {
        return None;
    }
    // SAFETY: the loader keeps the key for the process.
    unsafe { out.as_str() }.map(str::to_owned)
}
