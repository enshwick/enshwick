//! The server's world: components, entities and players, their queries and
//! writes, chat, saves, build rights and [`Transaction`]s.
//!
//! Most calls belong on the server's game thread, from a game event or a
//! [`Scheduler`](crate::schedule::Scheduler) task, and each says what it does
//! elsewhere. On a client the server is the built-in one, while it hosts a
//! game.

mod accounts;
mod build;
mod capture;
mod containers;
mod entities;
mod geometry;
mod players;
mod prop_state;
mod props;
mod rename;
mod stats;
mod transactions;
mod vitals;

use core::ffi::c_void;

use crate::abi::{EntityPlace, Header, PlayerInfo, Status, Str};
use crate::engine_ids as ids;
use crate::game_events::{Field, Scalar};
use crate::{Entity, ItemId, PlayerKey};
use crate::{Host, Load, Off};

pub use accounts::AccountAction;
pub use build::{BuildRefusal, PlayerPermissions};
pub use capture::{CAPTURE_TILES, Capture};
pub use containers::ContainerKinds;
pub use geometry::near;
pub use prop_state::{HeldState, PropState};
pub(crate) use props::span;
pub use props::{CapturedProp, Prop, WorldTransform};
pub use rename::Rename;
pub use stats::{Meter, Progress, Skill, SkillNode, Vitals};
pub use transactions::{Transaction, TransactionId, TransactionStatus};

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
    /// fn readable(world: &enshwick::world::World, flags: enshwick::world::WritableType, entity: enshwick::Entity) -> bool {
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

    /// The `T` at the offset the loader resolves for the engine id `id`,
    /// inside this type's size.
    pub(crate) fn field_by_id<T: Scalar>(self, load: &Load<'_>, id: &str) -> Result<Field<T>, Off> {
        let offset = load.constant(id)?;
        Field::within(self.ty.0, self.size, offset)
            .ok_or_else(|| Off::new(format!("field {id}: past the end of {}", self.name)))
    }
}

/// A connected player, as the server sees them.
///
/// # Examples
///
/// See [`World`].
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct Player {
    /// The player character's entity.
    pub entity: Entity,
    /// The server's player slot.
    pub slot: u32,
    /// The same for the same account in every session.
    pub key: PlayerKey,
    /// The server's permission bits for the player, as
    /// [player events](crate::abi::PlayerInfo::permissions) list them.
    pub permissions: u8,
    /// The name the client sent; untrusted, may repeat between players.
    pub name: String,
}

impl Player {
    pub(crate) fn from_info(info: &PlayerInfo) -> Self {
        let name = &info.name[..usize::from(info.name_len).min(info.name.len())];
        Self {
            entity: info.entity,
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
    /// use enshwick::Entity;
    /// use enshwick::world::Player;
    ///
    /// let player = Player { entity: Entity(2), slot: 0, key: enshwick::PlayerKey::new(7), permissions: 0, name: "Émile".into() };
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
            entity: place.entity,
            position: crate::ecs::metres(place.position),
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
    /// let chest = Placed { entity: Entity(7), position: [3.0, 0.0, 4.0] };
    /// assert_eq!(chest.distance([0.0, 0.0, 0.0]), 5.0);
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
    /// The part of the world asked about is not loaded yet; asking starts
    /// loading it, so try again in a second or so.
    #[error("that part of the world is not loaded")]
    NotLoaded,
    /// The box asked about leaves the world: past its edge across, or
    /// above the height the loader takes.
    #[error("outside the world")]
    OutsideWorld,
    /// The component type is the static half of its ECS type, such as
    /// `FuelSetup`: the set-up values every entity of the template shares,
    /// which the world reads but never writes.
    #[error("the template's set-up data, which is never written")]
    StaticHalf,
    /// The prop holds state a player would lose: items, growth, a
    /// production queue, assigned survivors, written text, fuel, logic
    /// wiring or a logic trigger's settings; [`HeldState`] names each
    /// kind.
    #[error("the prop holds {0}")]
    HoldsState(PropState),
    /// The entity is no prop: not placed from an item, or a creature such
    /// as an animal, a pet or a survivor, which prop calls never touch.
    #[error("not a prop")]
    NotAProp,
}

impl WorldError {
    /// The error the loader's `status` stands for, with `missing` for
    /// [`Status::MISSING`]: [`Self::Gone`] where the call names an entity,
    /// [`Self::Lacks`] where it names a component.
    pub(crate) fn check(status: Status, missing: Self) -> Result<(), Self> {
        match status {
            Status::OK => Ok(()),
            Status::WRONG_THREAD => Err(Self::WrongThread),
            Status::NOT_LOADED => Err(Self::NotLoaded),
            Status::OUTSIDE_WORLD => Err(Self::OutsideWorld),
            Status::STATIC_HALF => Err(Self::StaticHalf),
            Status::NOT_A_PROP => Err(Self::NotAProp),
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
        entity: Entity::NONE,
        key: PlayerKey::new(0),
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

impl crate::script::Bound for World {
    const NAME: &'static str = "world";
    const DOC: &'static str = "The server's world: players, chat, notices and positions.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] =
        &[Self::SCRIPT_PLAYERS, Self::SCRIPT_THINGS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.world()
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
    orientation: Option<usize>,
    /// The components that mark what is placed from an item but is no
    /// prop: an animal, a survivor, a flame altar.
    no_props: Option<[ComponentType; 3]>,
    label: Option<ComponentType>,
    stats: Option<stats::Stats>,
    containers: Option<containers::Containers>,
}

static RESOLVED: crate::state::ModLocal<Resolved> = crate::state::ModLocal::new();

/// The offset of the orientation inside a `CurrentTransform`, when the
/// loader resolves it and the template an item places, which the loader
/// reads into item data from the same reflection.
fn orientation_offset(load: &Load<'_>) -> Option<usize> {
    load.constant(ids::ITEM_INFO_PLACED_ENTITY).ok()?;
    let transform = load.constant(ids::CURRENT_TRANSFORM_TRANSFORM).ok()?;
    let orientation = load.constant(ids::WORLD_TRANSFORM_ORIENTATION).ok()?;
    usize::try_from(transform.checked_add(orientation)?).ok()
}

/// The component type named `name` on the host's build.
fn type_named(host: Host, name: &str) -> Result<ComponentType, Off> {
    let mut hash = 0;
    let status = (host.api().type_hash)(Str::new(name), &raw mut hash);
    crate::load::ok(status, ComponentType(hash), || {
        format!("component type {name}")
    })
}

#[crate::script(things)]
impl World {
    pub(crate) fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        if let Some(resolved) = RESOLVED.get() {
            return Ok(Self(resolved));
        }
        let host = load.host();
        let resolved = Resolved {
            host,
            transform: type_named(host, ids::CURRENT_TRANSFORM)?,
            used_item: type_named(host, ids::USED_ITEM)?,
            orientation: orientation_offset(load),
            no_props: [
                ids::ANIMAL_SAVED,
                ids::ACTIVE_NPC_STATE_SAVED,
                ids::BASE_ID_COMPONENT,
            ]
            .map(|name| type_named(host, name).ok())
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|types| types.try_into().ok()),
            label: type_named(host, ids::LABEL).ok(),
            containers: containers::Containers::resolve(host),
            stats: stats::Stats::resolve(load, host)
                .inspect_err(|off| crate::debug!("stat reads off: {off}"))
                .ok(),
        };
        Ok(Self(RESOLVED.get_or_init(|| resolved)))
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
    pub fn component_type(self, _load: &Load<'_>, name: &str) -> Result<ComponentType, Off> {
        type_named(self.0.host, name)
    }

    /// A pointer to `entity`'s component of type `ty`, valid until the game
    /// thread returns to the game; `None` when the entity has none. For the
    /// static half of an ECS type, such as `FuelSetup` beside its dynamic
    /// `DynamicFuelSetup`, it is the set-up data the entity's template
    /// holds, shared by every entity of the template and read-only.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub fn component(&self, entity: Entity, ty: ComponentType) -> Option<*const c_void> {
        let mut out: *const c_void = core::ptr::null();
        let status =
            (self.0.host.api().component)(self.0.host.handle(), entity, ty.0, &raw mut out);
        (status == Status::OK && !out.is_null()).then_some(out)
    }

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
    pub fn writable_type(self, load: &Load<'_>, name: &'static str) -> Result<WritableType, Off> {
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
    /// [`WorldError::StaticHalf`] for the static half of an ECS type, the
    /// template's set-up data, [`WorldError::WrongThread`] off the game
    /// thread and
    /// [`WorldError::Missing`] on a build without writes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Outcome};
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     let flags = world.writable_type(load, "keen::ecs::CombatFlags")?;
    ///     let actions = load.actions(enshwick::i18n::Texts::default())?;
    ///     actions.add(Action::new("immune", "Make your character immune").admins(), move |request| {
    ///         // `immune` is the first byte of `CombatFlags`.
    ///         match world.update(request.player.entity, flags, |bytes| bytes[0] = 1) {
    ///             Ok(()) => Outcome::done("Immune"),
    ///             Err(why) => Outcome::refused(format!("Not immune: {why}")),
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
        let data = self.component(entity, ComponentType(field.type_hash()))?;
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
    /// use enshwick::actions::{Action, Arg, Outcome, Value};
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     let faction = world.writable_type(load, "keen::ecs::Faction")?;
    ///     let value = faction.field::<u8>(load, "faction")?;
    ///     let actions = load.actions(enshwick::i18n::Texts::default())?;
    ///     let befriend = Action::new("befriend", "Make an entity a player ally")
    ///         .admins()
    ///         .arg(Arg::number("Entity", 0.0..=f64::from(u32::MAX), 1.0));
    ///     actions.add(befriend, move |request| {
    ///         let [Value::Number(id)] = request.args else {
    ///             return Outcome::refused("Name an entity");
    ///         };
    ///         let entity = enshwick::Entity(*id as u32);
    ///         let before = world.get(entity, value);
    ///         match world.set(entity, value, 11) {
    ///             Ok(()) => Outcome::done(format!("Faction {before:?} -> 11")),
    ///             Err(why) => Outcome::refused(format!("{why}")),
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
        let status =
            (self.0.host.api().component_write)(self.0.host.handle(), entity, ty.0, &raw mut out);
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
    #[script]
    pub fn position(&self, entity: Entity) -> Option<[f64; 3]> {
        let data = self.component(entity, self.0.transform)?;
        // SAFETY: `component` returned the entity's live CurrentTransform,
        // valid until the game thread moves on, and read at once.
        Some(unsafe { crate::ecs::transform_position(data.cast()) })
    }

    /// The item `entity` was placed from, which it keeps across saves.
    ///
    /// # Examples
    ///
    /// See [`World`].
    #[must_use]
    pub fn used_item(&self, entity: Entity) -> Option<ItemId> {
        let data = self.component(entity, self.0.used_item)?;
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
    pub fn has_label(&self, entity: Entity) -> Option<bool> {
        let label = self.0.label?;
        let Some(data) = self.component(entity, label) else {
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
    pub fn template(&self, entity: Entity) -> Option<[u8; 16]> {
        let mut out = [0u8; 16];
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
    pub fn save(self) -> Result<(), WorldError> {
        WorldError::from_status((self.0.host.api().save_world)(self.0.host.handle()))
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
    pub fn key(self) -> Option<String> {
        world_key(self.0.host)
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
    // SAFETY: `world_key` returned OK, and the loader keeps the key's bytes
    // alive and unchanged for the process, which outlives the copy here.
    unsafe { out.as_str() }.map(str::to_owned)
}
