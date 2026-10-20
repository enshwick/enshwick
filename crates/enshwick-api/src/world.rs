//! Reads of the server's world: components, templates, positions and
//! players. Call them on the server's game thread, from a game event or a
//! [`Scheduler`](crate::schedule::Scheduler) task; elsewhere they return
//! `None` or nothing.

use core::ffi::c_void;

use crate::abi::{HostApi, PlayerInfo, Status, Str};
use crate::ecs::Entity;
use crate::{Host, Load, Off};

/// A reflected ECS component type, resolved once at init.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
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
    fn from_info(info: &PlayerInfo) -> Self {
        let name = &info.name[..usize::from(info.name_len).min(info.name.len())];
        Self {
            entity: Entity(info.entity),
            slot: info.slot,
            key: info.key,
            permissions: info.permissions,
            name: String::from_utf8_lossy(name).into_owned(),
        }
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

/// The world reads; get it from [`Load::world`].
///
/// # Examples
///
/// ```
/// use enshwick_api::events::PlayerJoined;
///
/// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
///     let world = load.world()?;
///     load.on::<PlayerJoined>(move |joined| {
///         let entity = enshwick_api::ecs::Entity(joined.entity);
///         if let Some(player) = world.player(entity) {
///             enshwick_api::info!("{} joined, admin: {}", player.name, player.is_admin());
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
        // SAFETY: a live CurrentTransform starts with x, y and z in 32.32 fixed point.
        let fixed = unsafe { data.cast::<[i64; 3]>().read_unaligned() };
        Some(crate::ecs::metres(fixed))
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
        // SAFETY: plain data, valid when zeroed.
        let mut info: PlayerInfo = unsafe { core::mem::zeroed() };
        let status = (self.host.api().player)(self.host.handle(), entity.0, &raw mut info);
        status.is_ok().then(|| Player::from_info(&info))
    }

    /// Every connected player with a character in the world.
    ///
    /// # Examples
    ///
    /// ```
    /// fn count(world: &enshwick_api::world::World) -> usize {
    ///     world.players().len()
    /// }
    /// ```
    #[must_use]
    pub fn players(&self) -> Vec<Player> {
        const MAX: usize = 64;
        // SAFETY: plain data, valid when zeroed.
        let mut infos: [PlayerInfo; MAX] = unsafe { core::mem::zeroed() };
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
