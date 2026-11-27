//! The server's world: components, entities and players, their queries and
//! writes, chat, saves, build rights and transactions.
//!
//! Most calls belong on the server's game thread, from a game event or a
//! [`Scheduler`](crate::schedule::Scheduler) task, and each says what it does
//! elsewhere. On a client the server is the built-in one, while it hosts a
//! game.

mod accounts;
#[cfg(feature = "unstable")]
mod build;
#[cfg(feature = "unstable")]
mod capture;
mod containers;
mod entities;
pub(crate) mod fields;
mod geometry;
pub(crate) mod join;
mod player_ref;
mod players;
#[cfg(feature = "unstable")]
mod prop_state;
#[cfg(feature = "unstable")]
mod props;
pub(crate) mod rename;
mod saves;
mod stats;
#[cfg(feature = "unstable")]
mod transactions;
mod vitals;

use core::ffi::c_void;

use crate::__internal::Transform;
use crate::BaseLoad;
use crate::abi::{EntityPlace, PlayerInfo};
use crate::content::Guid;
use crate::ecs::ComponentType;
use crate::engine_ids as ids;
use crate::game_events::{Field, Scalar};
use crate::{Entity, ItemId, PlayerKey};
use crate::{Host, Off, WorldError};

pub use accounts::AccountAction;
#[cfg(feature = "unstable")]
pub use build::{BuildRefusal, PlayerPermissions};
#[cfg(feature = "unstable")]
pub use capture::{CAPTURE_TILES, Capture};
pub use containers::ContainerKinds;
pub use geometry::near;
pub use join::Join;
pub use player_ref::PlayerRef;
#[cfg(feature = "unstable")]
pub use players::{PlayerSpawnType, RespawnPoint};
#[cfg(feature = "unstable")]
pub use prop_state::{HeldState, PropState};
#[cfg(feature = "unstable")]
pub(crate) use props::span;
#[cfg(feature = "unstable")]
pub use props::{CapturedProp, Prop, PropPlacementBox, PropPlacementInfo, WorldTransform};
pub use rename::Rename;
pub use saves::{SaveInfo, WorldGuid};
pub use stats::{Meter, Progress, Skill, Vitals};
#[cfg(feature = "unstable")]
pub use transactions::{Settled, Transaction, TransactionId, TransactionStatus};

/// A reflected component type a mod may write, resolved once at init with
/// [`WritableType::resolve`]: one the server owns, not a copy the game keeps
/// for clients.
///
/// # Examples
///
/// ```
/// use enshwick::world::WritableType;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let flags = WritableType::resolve(load, "keen::ecs::CombatFlags")?;
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
    /// The component type named `name` for writes, such as
    /// `keen::ecs::CombatFlags`: one the server owns. Writes to the copies
    /// the game keeps for clients are refused with [`WorldError::Refused`],
    /// since the game overwrites them from the server's own every step.
    ///
    /// # Errors
    ///
    /// [`Off`] for a type not reflected on this build.
    ///
    /// # Examples
    ///
    /// See [`WritableType`].
    pub fn resolve(load: &impl crate::Loading, name: &'static str) -> Result<Self, Off> {
        let load = load.base();
        Ok(Self {
            ty: ComponentType::resolve(load, name)?,
            size: load.type_size(name)?,
            name,
        })
    }

    /// The component type, for reads.
    ///
    /// # Examples
    ///
    /// ```
    /// fn same(a: enshwick::world::WritableType, b: enshwick::world::WritableType) -> bool {
    ///     a.ty() == b.ty()
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
    ///     let flags = enshwick::world::WritableType::resolve(load, "keen::ecs::CombatFlags")?;
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
    pub fn field<T: Scalar>(self, load: &impl crate::Loading, name: &str) -> Result<Field<T>, Off> {
        let offset = load.base().field_offset(self.name, name)?;
        Field::within(self.ty.hash(), self.size, offset).ok_or_else(|| {
            Off::new(format!(
                "field {}.{name}: past the end of the type",
                self.name
            ))
        })
    }

    /// The `T` at the offset the loader resolves for the engine id `id`,
    /// inside this type's size.
    pub(crate) fn field_by_id<T: Scalar>(self, load: &BaseLoad, id: &str) -> Result<Field<T>, Off> {
        let offset = load.constant(id)?;
        Field::within(self.ty.hash(), self.size, offset)
            .ok_or_else(|| Off::new(format!("field {id}: past the end of {}", self.name)))
    }
}

/// A connected player, as the server sees them.
///
/// # Examples
///
/// See [`World`].
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct Player {
    /// The player character's entity.
    pub entity: Entity,
    /// The server's player slot.
    pub slot: u32,
    /// The same for the same account in every session.
    pub key: PlayerKey,
    /// The server's permission bits for the player, as
    /// [player events](crate::abi::PlayerInfo::permissions) list them.
    pub permissions: u32,
    /// The name the client sent; untrusted, may repeat between players.
    pub name: String,
}

impl Player {
    /// A `Player` of its parts.
    ///
    /// # Examples
    ///
    /// See [`Player`].
    #[must_use]
    pub const fn new(
        entity: Entity,
        slot: u32,
        key: PlayerKey,
        permissions: u32,
        name: String,
    ) -> Self {
        Self {
            entity,
            slot,
            key,
            permissions,
            name,
        }
    }
}

impl Player {
    pub(crate) fn from_info(info: &PlayerInfo) -> Self {
        Self {
            entity: info.entity,
            slot: info.slot,
            key: info.key,
            permissions: info.permissions,
            name: info.name_text().into_owned(),
        }
    }

    /// Whether the player's name is `name`, ignoring case, as a player types
    /// another's name in chat.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::world::Player;
    ///
    /// let player = Player::new(Entity::from_raw(2).ok_or("no entity")?, 0, enshwick::PlayerKey::new(7), 0, "Émile".into());
    /// assert!(player.is_named("éMILE"));
    /// assert!(!player.is_named("Emile"));
    /// # Ok::<(), &str>(())
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
        PlayerInfo::admin(self.permissions)
    }

    /// Whether the server gives the player `permission`, as their user
    /// group in the server's settings grants it.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::abi::PlayerInfo;
    /// use enshwick::world::{Player, PlayerPermissions};
    ///
    /// let friend = Player::new(Entity::from_raw(2).ok_or("no entity")?, 0, enshwick::PlayerKey::new(7), PlayerInfo::CAN_EDIT_BASE, "Ann".into());
    /// assert!(friend.has(PlayerPermissions::CanEditBase));
    /// assert!(!friend.has(PlayerPermissions::CanKickBan));
    /// # Ok::<(), &str>(())
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub const fn has(&self, permission: PlayerPermissions) -> bool {
        self.permissions & permission.bit() != 0
    }
}

/// An entity with a place in the world, from [`World::entities`] and
/// [`World::near`].
///
/// # Examples
///
/// See [`World::near`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Placed {
    /// The entity.
    pub entity: Entity,
    /// Where it is, in metres.
    pub position: [f64; 3],
}

impl Placed {
    /// A `Placed` of its parts.
    ///
    /// # Examples
    ///
    /// See [`Placed`].
    #[must_use]
    pub const fn new(entity: Entity, position: [f64; 3]) -> Self {
        Self { entity, position }
    }
}

impl Placed {
    fn from_abi(place: &EntityPlace) -> Self {
        Self {
            entity: place.entity,
            position: place.position,
        }
    }

    /// The distance from `point` in metres.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::world::Placed;
    ///
    /// let chest = Placed::new(Entity::from_raw(7).ok_or("no entity")?, [3.0, 0.0, 4.0]);
    /// assert_eq!(chest.distance([0.0, 0.0, 0.0]), 5.0);
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub fn distance(&self, point: [f64; 3]) -> f64 {
        let [x, y, z] = [0, 1, 2].map(|i| self.position[i] - point[i]);
        (x * x + y * y + z * z).sqrt()
    }
}

/// How a notice reached a player, from [`notify`](World::notify).
///
/// # Examples
///
/// See [`World::notify`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Notice {
    /// On screen, as the game shows its own messages: the player's client
    /// runs Enshwick.
    OnScreen,
    /// Handed to chat: the player's client lacks Enshwick, or the notice
    /// could not leave now. A full line queue on the server can still drop
    /// the chat line.
    Chat,
    /// Shown to no player: the console or a remote caller, which has no
    /// account; answer it in the action's outcome.
    Nobody,
}

/// A [`PlayerInfo`] for the loader to fill, its header naming this build's
/// layout.
fn blank_info() -> PlayerInfo {
    PlayerInfo::with_name(Entity::NONE, PlayerKey::new(0), 0, "")
}

/// The world reads; get it from [`Load::world`](../struct.Load.html#method.world).
///
/// # Examples
///
/// ```
/// use enshwick::events::PlayerJoined;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let world = load.world()?;
///     load.on::<PlayerJoined>(move |joined| {
///         let entity = joined.entity;
///         if let Some(player) = world.player(entity) {
///             enshwick::info!("{} joined, admin: {}", player.name, player.is_admin());
///         }
///     })
/// }
/// ```
///
/// A `World` is a pointer to what the mod resolved once, so a copy is cheap:
/// pass it by value and move it into closures.
#[derive(Clone, Copy, Debug)]
pub struct World(&'static Resolved);

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::world`.
    #[doc(hidden)]
    fn world(&mut self) -> Result<World, Off>;
}

impl crate::script::Bound for World {
    const NAME: &'static str = "world";
    const DOC: &'static str = "The server's world: players, accounts, entities, props, vitals and edits in transactions. Its props, build rights, captures and transactions are unstable: they may change in any release.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[
        Self::SCRIPT_PLAYERS,
        Self::SCRIPT_THINGS,
        Self::SCRIPT_ACCOUNTS,
        #[cfg(feature = "unstable")]
        Self::SCRIPT_BUILD,
        Self::SCRIPT_ENTITIES,
        #[cfg(feature = "unstable")]
        Self::SCRIPT_PROPS,
        #[cfg(feature = "unstable")]
        Self::SCRIPT_TRANSACTIONS,
        Self::SCRIPT_VITALS,
        Self::SCRIPT_METERS,
        Self::SCRIPT_FIELDS,
        #[cfg(feature = "unstable")]
        Self::SCRIPT_CAPTURE,
        Self::SCRIPT_SAVES,
    ];

    fn resolve(load: &mut impl crate::Loading) -> Result<Self, Off> {
        load.base_mut().world()
    }
}

impl LoadMethods for BaseLoad {
    fn world(&mut self) -> Result<World, Off> {
        World::resolve(self)
    }
}

#[crate::script(meters)]
impl World {
    /// Changes an item's meter on its entity `pide` to what `change` returns
    /// for the value now, as `World::change_item_meter` does; nil keeps it.
    ///
    /// # Errors
    ///
    /// As [`World::change_item_meter`].
    #[script(name = "change_item_meter")]
    pub(crate) fn change_item_meter_script(
        self,
        pide: Entity,
        change: impl Fn(i32) -> Option<i32>,
    ) -> Result<Meter, WorldError> {
        self.change_item_meter(pide, |now| change(now).unwrap_or(now))
    }

    /// Changes `entity`'s fog resistance to what `change` returns for it
    /// now, as `World::change_fog_resistance` does; nil keeps it.
    ///
    /// # Errors
    ///
    /// As [`World::change_fog_resistance`].
    #[script(name = "change_fog_resistance")]
    pub(crate) fn change_fog_resistance_script(
        self,
        entity: Entity,
        change: impl Fn(i32) -> Option<i32>,
    ) -> Result<Meter, WorldError> {
        self.change_fog_resistance(entity, |now| change(now).unwrap_or(now))
    }
}

// A copy of the world is one pointer, whatever it resolves.
const _: () = assert!(size_of::<World>() == size_of::<usize>());

/// The world's component types and readers, resolved once per mod life.
#[derive(Debug)]
struct Resolved {
    host: Host,
    transform: ComponentType,
    used_item: ComponentType,
    /// Where a `CurrentTransform` holds its orientation, on builds that
    /// reflect it and the template an item places, which tells props apart.
    #[cfg(feature = "unstable")]
    orientation: Option<usize>,
    /// The components that mark what is placed from an item but is no
    /// prop: an animal, a survivor, a flame altar.
    #[cfg(feature = "unstable")]
    no_props: Option<[ComponentType; 3]>,
    label: Option<ComponentType>,
    stats: Option<stats::Stats>,
    containers: Option<containers::Containers>,
}

static RESOLVED: crate::state::ModLocal<Resolved> = crate::state::ModLocal::new();

/// The offset of the orientation inside a `CurrentTransform`, when the
/// loader resolves it and the template an item places, which the loader
/// reads into item data from the same reflection.
#[cfg(feature = "unstable")]
fn orientation_offset(load: &BaseLoad) -> Option<usize> {
    load.constant(ids::ITEM_INFO_PLACED_ENTITY).ok()?;
    let transform = load.constant(ids::CURRENT_TRANSFORM_TRANSFORM).ok()?;
    let orientation = load.constant(ids::WORLD_TRANSFORM_ORIENTATION).ok()?;
    usize::try_from(transform.checked_add(orientation)?).ok()
}

#[crate::script(things)]
impl World {
    pub(crate) fn resolve(load: &BaseLoad) -> Result<Self, Off> {
        if let Some(resolved) = RESOLVED.get() {
            return Ok(Self(resolved));
        }
        let host = load.host();
        // Asked once for every mod with a world, as a write cannot ask after
        // `init`; where the loader cannot write, the writes answer Missing.
        let _asked =
            (host.api().progress_write)(host.handle(), Entity::NONE, crate::abi::PROGRESS_OPEN, 0);
        let resolved = Resolved {
            host,
            transform: Transform::named(host)?.0,
            used_item: ComponentType::named(host, ids::USED_ITEM)?,
            #[cfg(feature = "unstable")]
            orientation: orientation_offset(load),
            #[cfg(feature = "unstable")]
            no_props: [
                ids::ANIMAL_SAVED,
                ids::ACTIVE_NPC_STATE_SAVED,
                ids::BASE_ID_COMPONENT,
            ]
            .map(|name| ComponentType::named(host, name).ok())
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|types| types.try_into().ok()),
            label: ComponentType::named(host, ids::LABEL).ok(),
            containers: containers::Containers::resolve(host),
            stats: stats::Stats::resolve(load, host)
                .inspect_err(|off| crate::debug!("stat reads off: {off}"))
                .ok(),
        };
        Ok(Self(RESOLVED.get_or_init(|| resolved)))
    }

    /// A pointer to `entity`'s component of type `ty`, valid until the game
    /// thread returns to the game; `None` when the entity has none. For the
    /// static half of an ECS type, such as `FuelSetup` beside its dynamic
    /// `DynamicFuelSetup`, it is the set-up data the entity's template
    /// holds, shared by every entity of the template and read-only.
    ///
    /// # Examples
    ///
    /// ```
    /// fn has(world: enshwick::world::World, entity: enshwick::Entity, ty: enshwick::ecs::ComponentType) -> bool {
    ///     world.component(entity, ty).is_some()
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn component(self, entity: Entity, ty: ComponentType) -> Option<*const c_void> {
        self.component_at(entity, ty)
    }

    /// The pointer [`World::component`] gives, for this crate's own reads.
    pub(crate) fn component_at(self, entity: Entity, ty: ComponentType) -> Option<*const c_void> {
        enshwick_base::__internal::component(ty, self.0.host, entity)
    }

    /// The host this world calls through.
    pub(crate) const fn host(self) -> Host {
        self.0.host
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
    /// [`WorldError::StaticHalf`] for the static half of an ECS type, the
    /// template's set-up data, [`WorldError::WrongThread`] off the game
    /// thread and
    /// [`WorldError::Unavailable`] where no server runs or on a build without
    /// writes.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     let flags = enshwick::world::WritableType::resolve(load, "keen::ecs::CombatFlags")?;
    ///     // Run on the game thread: `immune` is the first byte of `CombatFlags`.
    ///     let _immune =
    ///         move |entity: enshwick::Entity| world.update(entity, flags, |bytes| bytes[0] = 1);
    ///     Ok(())
    /// }
    /// ```
    pub fn update<R>(
        &self,
        entity: Entity,
        ty: WritableType,
        write: impl FnOnce(&mut [u8]) -> R,
    ) -> Result<R, WorldError> {
        let len = usize::try_from(ty.size).map_err(|_| WorldError::Unavailable)?;
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
        // SAFETY: `component_mut` returned the entity's live component, handed
        // out for writing on the game thread; `len` is `WritableType::size`,
        // the reflected size of its type, and `copy` holds `len` bytes.
        unsafe { core::ptr::copy_nonoverlapping(data.cast::<u8>(), copy.as_mut_ptr(), len) };
        let result = write(copy);
        // Asked again: `write` may have moved or removed the component.
        let data = self.component_mut(entity, ty.ty)?;
        // SAFETY: `component_mut` returned the entity's live component again,
        // handed out for writing on the game thread; `len` is the reflected
        // size of its type, and `copy` holds `len` bytes.
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
        let data = self.component_at(entity, ComponentType::from_hash(field.type_hash()))?;
        let at = usize::try_from(field.offset()).ok()?;
        let mut bytes = [0u8; 8];
        let len = size_of::<T>().min(bytes.len());
        // SAFETY: `component` returned the entity's live component, valid until
        // the game thread moves on; `Field::within` checked at resolution that
        // `offset + size_of::<T>()` lies inside the reflected size of its type,
        // and `len` is at most `size_of::<T>()`.
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
    ///     let faction = enshwick::world::WritableType::resolve(load, "keen::ecs::Faction")?;
    ///     let value = faction.field::<u8>(load, "faction")?;
    ///     // Run on the game thread: makes `entity` a player ally.
    ///     let _befriend = move |entity: enshwick::Entity| world.set(entity, value, 11);
    ///     Ok(())
    /// }
    /// ```
    pub fn set<T: Scalar>(
        &self,
        entity: Entity,
        field: Field<T>,
        value: T,
    ) -> Result<(), WorldError> {
        let data = self.component_mut(entity, ComponentType::from_hash(field.type_hash()))?;
        let at = usize::try_from(field.offset()).map_err(|_| WorldError::Unavailable)?;
        let mut bytes = [0u8; 8];
        let len = size_of::<T>().min(bytes.len());
        if !value.to_bytes(&mut bytes[..len]) {
            return Err(WorldError::Unavailable);
        }
        // SAFETY: `component_mut` returned the entity's live component, handed
        // out for writing on the game thread; `Field::within` checked at
        // resolution that `offset + size_of::<T>()` lies inside the reflected
        // size of its type, and `len` is at most `size_of::<T>()`.
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
        let status = (self.0.host.api().component_write)(
            self.0.host.handle(),
            entity,
            ty.hash(),
            &raw mut out,
        );
        match WorldError::check(status, WorldError::Gone) {
            Ok(()) if !out.is_null() => Ok(out),
            Ok(()) => Err(WorldError::Unavailable),
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
    #[script]
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "the public method takes `&self`, as mods and the Luau layer call it"
    )]
    pub fn position(&self, entity: Entity) -> Option<[f64; 3]> {
        position(Transform(self.0.transform), self.host(), entity)
    }

    /// The item `entity` was placed from, which it keeps across saves.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    #[script]
    pub fn used_item(&self, entity: Entity) -> Option<ItemId> {
        let data = self.component_at(entity, self.0.used_item)?;
        // SAFETY: `component` returned the entity's live UsedItem, valid until
        // the game thread moves on; the component is the u32 item id alone.
        Some(unsafe { crate::raw::read_at::<ItemId>(data.cast(), 0) })
    }

    /// Whether `entity` carries a name a player or [`World::set_label`] gave
    /// it; `None` when the world cannot tell.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    ///
    /// fn unnamed(world: &enshwick::world::World, chest: Entity) -> bool {
    ///     world.has_label(chest) == Some(false)
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn has_label(&self, entity: Entity) -> Option<bool> {
        let label = self.0.label?;
        let Some(data) = self.component_at(entity, label) else {
            return Some(false);
        };
        // SAFETY: `component` returned the entity's live CustomPlayerString,
        // valid until the game thread moves on; the component is its u64
        // string id, 0 for none.
        Some(unsafe { crate::raw::read_at::<u64>(data.cast(), 0) } != 0)
    }

    /// `entity`'s template GUID, in file order.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    #[script]
    pub fn template(&self, entity: Entity) -> Option<Guid> {
        let mut out = Guid::NIL;
        let status =
            (self.0.host.api().entity_template)(self.0.host.handle(), entity, &raw mut out);
        status.is_ok().then_some(out)
    }

    /// Asks the server to save the world now, as its timed saves do;
    /// [`WorldSaved`](crate::events::WorldSaved) follows once the save is on
    /// disk. A save already under way counts as asked.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] while the world may not be saved, as before
    /// it has loaded; [`WorldError::WrongThread`] off the game thread;
    /// [`WorldError::Unavailable`] on any other refusal.
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
    #[script]
    pub fn save(self) -> Result<(), WorldError> {
        WorldError::from_status((self.0.host.api().save_world)(self.0.host.handle()))
    }

    /// The key of the world the server runs: its save file's name without
    /// `.ksd`, such as `world0`, the same in every session; `None` before
    /// the server has read the world. The game gives a deleted world's name
    /// to its next new world, so a key names a slot, not one world for
    /// ever. On a client it is the key of the game it hosts, and `None` once
    /// that game ends. Any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn named(world: &enshwick::world::World) -> String {
    ///     world.key().unwrap_or_else(|| "no world".to_owned())
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn key(self) -> Option<String> {
        crate::state::world_key(self.0.host)
    }

    /// The mod's data folder in the world the server runs,
    /// `<stem>-mod-data/<id>` beside the world's save, made on first use;
    /// `None` while no world runs. The loader renames the folder aside when
    /// a new world takes the save's name, so what a mod keeps there belongs
    /// to this world. Any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn hold(world: &enshwick::world::World) -> Option<enshwick::state::Hold> {
    ///     world.data_dir().map(|dir| enshwick::state::Hold::new(&dir))
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn data_dir(self) -> Option<std::path::PathBuf> {
        crate::state::data_dir(self.0.host)
    }
}

/// The players connected now, from the loader; none when it cannot list
/// them.
pub(crate) fn online(host: Host) -> Vec<Player> {
    listed(host).unwrap_or_default()
}

/// The players connected now, or the loader's status when it cannot list
/// them.
pub(crate) fn listed(host: Host) -> Result<Vec<Player>, crate::abi::Status> {
    const MAX: usize = 64;
    let mut infos = [blank_info(); MAX];
    let mut count = 0u32;
    let status = (host.api().players)(
        host.handle(),
        infos.as_mut_ptr(),
        u32::try_from(size_of::<PlayerInfo>()).unwrap_or(0),
        u32::try_from(MAX).unwrap_or(0),
        &raw mut count,
    );
    if !status.is_ok() {
        return Err(status);
    }
    Ok(infos
        .iter()
        .take(usize::try_from(count).unwrap_or(0).min(MAX))
        .map(Player::from_info)
        .collect())
}

/// The characters of the connected players `keys`, from one list of the
/// players; a player counts only with a character in the world.
pub(crate) fn characters<const N: usize>(
    host: Host,
    keys: [PlayerKey; N],
) -> Result<[Entity; N], WorldError> {
    let players = listed(host).or_else(|status| {
        WorldError::check(status, WorldError::Unavailable).map(|()| Vec::new())
    })?;
    let mut characters = [Entity::NONE; N];
    for (character, key) in characters.iter_mut().zip(keys) {
        *character = players
            .iter()
            .find(|player| player.key == key && player.entity != Entity::NONE)
            .map(|player| player.entity)
            .ok_or(WorldError::Gone)?;
    }
    Ok(characters)
}

/// Where `entity` stands, in metres, from its `CurrentTransform`.
pub(crate) fn position(transform: Transform, host: Host, entity: Entity) -> Option<[f64; 3]> {
    let data = enshwick_base::__internal::component(transform.0, host, entity)?;
    // SAFETY: `component` returned the entity's live component of type
    // `transform`, a `CurrentTransform` (a `Transform` holds only that type),
    // valid until the game thread moves on, and it is read at once.
    Some(unsafe { crate::ecs::transform_position(data.cast()) })
}
