//! Game events: the ECS events the server's systems raise, such as a hit,
//! a death or a level gained, observed by reflected type.
//!
//! Every game event passes the event streams the engine moves at the
//! start of each simulation step. The loader copies the ones a mod asked for
//! and hands them over on the server's game thread, before the systems of
//! the step run, so the entities an event names are still in the world and
//! [`World`](crate::world::World) reads work in the handler. On a client they
//! come from the built-in server while it hosts a game.
//!
//! An event arrives as raw bytes in the engine's layout; [`Field`]s resolved
//! by name read them for the running build.

mod kinds;

use core::marker::PhantomData;

use crate::abi::{GameEventData, Str};
use crate::ecs::Entity;
use crate::{Load, Off};

pub use kinds::{
    AltarAction, AltarUsed, AttackCategories, Block, Built, CraftRequested, Crafted, DamageTypes,
    Died, EquipmentChanged, ExperienceGained, ExperienceSource, Hit, Interacted, Kind, LevelGained,
    LootOpened, PickedUp, Revived, TornDown,
};

mod sealed {
    pub trait Sealed {}
}

/// A reflected game event type, resolved once at init.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let died = load.event_type("keen::ecs::EntityDiedEvent")?;
///     assert_eq!(died.name(), "keen::ecs::EntityDiedEvent");
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventType {
    name: &'static str,
    hash: u32,
    size: u32,
}

impl EventType {
    pub(crate) fn resolve(load: &Load<'_>, name: &'static str) -> Result<Self, Off> {
        let api = load.host().api();
        let mut hash = 0;
        let status = (api.type_hash)(Str::new(name), &raw mut hash);
        crate::load::ok(status, (), || format!("event type {name}"))?;
        let size = load.type_size(name)?;
        Ok(Self { name, hash, size })
    }

    /// The reflected name.
    ///
    /// # Examples
    ///
    /// See [`EventType`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The reflected type hash, which [`RawGameEvent::type_hash`] carries.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let died = load.event_type("keen::ecs::EntityDiedEvent")?.hash();
    ///     load.on_every_raw_game_event(move |event| {
    ///         if event.type_hash() == died {
    ///             enshwick_loader_api::debug!("an entity died");
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub const fn hash(self) -> u32 {
        self.hash
    }

    /// The reflected size: every event of the type carries this many bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     assert_eq!(load.event_type("keen::ecs::EntityDiedEvent")?.size(), 16);
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub const fn size(self) -> u32 {
        self.size
    }

    /// The field `name` of this type, read as a `T`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the field is not reflected on this build, or a `T` at
    /// its offset would reach past the type's end.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let died = load.event_type("keen::ecs::EntityDiedEvent")?;
    ///     let dead = died.field::<Entity>(load, "deadEntityId")?;
    ///     load.on_raw_game_event(died, move |event| {
    ///         if let Some(entity) = event.get(dead) {
    ///             enshwick_loader_api::info!("entity {} died", entity.0);
    ///         }
    ///     })
    /// }
    /// ```
    pub fn field<T: Scalar>(self, load: &Load<'_>, name: &str) -> Result<Field<T>, Off> {
        self.inherited_field(load, self.name, name)
    }

    /// The field `name` that this type inherits from its base type
    /// `declared_in`, such as `rootSourceId` of `keen::ecs::BaseHitEvent` in
    /// `keen::ecs::HitEvent`: reflection lists a field under the type that
    /// declares it.
    ///
    /// # Errors
    ///
    /// As for [`EventType::field`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let hit = load.event_type("keen::ecs::HitEvent")?;
    ///     let attacker = hit.inherited_field::<Entity>(load, "keen::ecs::BaseHitEvent", "rootSourceId")?;
    ///     let _ = attacker;
    ///     Ok(())
    /// }
    /// ```
    pub fn inherited_field<T: Scalar>(
        self,
        load: &Load<'_>,
        declared_in: &str,
        name: &str,
    ) -> Result<Field<T>, Off> {
        let offset = load.field_offset(declared_in, name)?;
        self.field_at(offset).ok_or_else(|| {
            Off::new(format!(
                "field {declared_in}.{name}: past the end of {}",
                self.name
            ))
        })
    }
}

impl EventType {
    /// The `T` at `offset` of this type, inside its size.
    pub(crate) fn field_at<T: Scalar>(self, offset: u64) -> Option<Field<T>> {
        let end = offset.checked_add(size_of::<T>() as u64)?;
        (end <= u64::from(self.size)).then_some(Field {
            type_hash: self.hash,
            offset: u32::try_from(offset).ok()?,
            _value: PhantomData,
        })
    }
}

/// A plain value a [`Field`] reads from an event: an integer, a float, a
/// `bool` or an [`Entity`]; implemented by those types only.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::game_events::Scalar;
///
/// fn width<T: Scalar>() -> usize {
///     size_of::<T>()
/// }
/// assert_eq!(width::<u16>(), 2);
/// ```
pub trait Scalar: sealed::Sealed + Copy + 'static {
    /// The value from the first `size_of::<Self>()` bytes of `bytes`.
    #[doc(hidden)]
    fn from_bytes(bytes: &[u8]) -> Option<Self>;
}

macro_rules! scalars {
    ($($ty:ty),*) => {$(
        impl sealed::Sealed for $ty {}

        impl Scalar for $ty {
            fn from_bytes(bytes: &[u8]) -> Option<Self> {
                Some(<$ty>::from_le_bytes(bytes.get(..size_of::<$ty>())?.try_into().ok()?))
            }
        }
    )*};
}

scalars!(u8, i8, u16, i16, u32, i32, u64, i64, f32, f64);

impl sealed::Sealed for bool {}

impl Scalar for bool {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bytes.first().map(|&b| b != 0)
    }
}

impl sealed::Sealed for Entity {}

impl Scalar for Entity {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        u32::from_bytes(bytes).map(Entity)
    }
}

/// A field of one event type, resolved by name for the running build.
///
/// # Examples
///
/// See [`EventType::field`].
#[derive(Debug)]
pub struct Field<T> {
    type_hash: u32,
    offset: u32,
    _value: PhantomData<fn() -> T>,
}

impl<T> Clone for Field<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Field<T> {}

impl<T> Field<T> {
    /// The field's offset in its event.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let died = load.event_type("keen::ecs::EntityDiedEvent")?;
    ///     assert_eq!(died.field::<u32>(load, "deadEntityId")?.offset(), 8);
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
}

/// One game event, valid for the handler's call.
///
/// # Examples
///
/// See [`EventType::field`].
#[derive(Clone, Copy, Debug)]
pub struct RawGameEvent<'a> {
    type_hash: u32,
    bytes: &'a [u8],
}

impl<'a> RawGameEvent<'a> {
    /// An event of `type_hash` with `bytes`, as a mod's tests make one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::game_events::RawGameEvent;
    ///
    /// let bytes = [0u8; 16];
    /// let event = RawGameEvent::new(0xd8ec_982a, &bytes);
    /// assert_eq!(event.bytes().len(), 16);
    /// ```
    #[must_use]
    pub const fn new(type_hash: u32, bytes: &'a [u8]) -> Self {
        Self { type_hash, bytes }
    }

    /// The event's reflected type hash; see [`EventType::hash`].
    ///
    /// # Examples
    ///
    /// See [`EventType::hash`].
    #[must_use]
    pub const fn type_hash(&self) -> u32 {
        self.type_hash
    }

    /// The event as the engine wrote it.
    ///
    /// # Examples
    ///
    /// See [`RawGameEvent::new`].
    #[must_use]
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The value of `field`; `None` when the field belongs to another event
    /// type or lies past this event's bytes.
    ///
    /// # Examples
    ///
    /// See [`EventType::field`].
    #[must_use]
    pub fn get<T: Scalar>(&self, field: Field<T>) -> Option<T> {
        if field.type_hash != self.type_hash {
            return None;
        }
        T::from_bytes(self.bytes.get(usize::try_from(field.offset).ok()?..)?)
    }

    /// The `T` at `offset`, for layouts reflection does not name, such as an
    /// element of an array field; `None` past the event's bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::game_events::RawGameEvent;
    ///
    /// let mut bytes = [0u8; 16];
    /// bytes[8] = 7;
    /// assert_eq!(RawGameEvent::new(1, &bytes).read::<u32>(8), Some(7));
    /// assert_eq!(RawGameEvent::new(1, &bytes).read::<u64>(12), None);
    /// ```
    #[must_use]
    pub fn read<T: Scalar>(&self, offset: usize) -> Option<T> {
        T::from_bytes(self.bytes.get(offset..)?)
    }

    /// The event the loader handed over.
    ///
    /// # Safety
    ///
    /// `raw` is null or the loader's event, valid for the call.
    pub(crate) unsafe fn from_raw(raw: *const GameEventData) -> Option<Self> {
        // SAFETY: forwarded from the caller.
        let data = unsafe { raw.as_ref() }?;
        let size = usize::try_from(data.size).ok()?;
        let bytes = if size == 0 || data.data.is_null() {
            &[][..]
        } else {
            // SAFETY: the loader's event holds `size` bytes, valid for the call.
            unsafe { core::slice::from_raw_parts(data.data, size) }
        };
        Some(Self {
            type_hash: data.type_hash,
            bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_reads_only_its_own_type_inside_the_bytes() {
        let field = Field::<u32> {
            type_hash: 5,
            offset: 8,
            _value: PhantomData,
        };
        let mut bytes = [0u8; 12];
        bytes[8..].copy_from_slice(&42u32.to_le_bytes());
        assert_eq!(RawGameEvent::new(5, &bytes).get(field), Some(42));
        assert_eq!(RawGameEvent::new(6, &bytes).get(field), None);
        assert_eq!(RawGameEvent::new(5, &bytes[..10]).get(field), None);
    }

    #[test]
    fn scalars_read_little_endian() {
        let bytes = [0x01, 0x02, 0x03, 0x04, 0, 0, 0x80, 0x3f];
        assert_eq!(u16::from_bytes(&bytes), Some(0x0201));
        assert_eq!(Entity::from_bytes(&bytes), Some(Entity(0x0403_0201)));
        assert_eq!(f32::from_bytes(&bytes[4..]), Some(1.0));
        assert_eq!(bool::from_bytes(&bytes[4..]), Some(false));
        assert_eq!(u64::from_bytes(&bytes[1..]), None);
    }
}
