//! Game events: the ECS events the server's systems raise, such as a hit,
//! a death or a level gained, observed typed as a [`Kind`] or by reflected
//! type.
//!
//! Every game event passes the event streams the engine moves at the
//! start of each simulation step. The loader copies the ones a mod asked for
//! and hands them over on the server's game thread, before the systems of
//! the step run, so the entities an event names are still in the world and
//! [`World`](crate::world::World) reads work in the handler. On a client they
//! come from the built-in server while it hosts a game.
//!
//! An event arrives as raw bytes in the engine's layout; a [`Kind`] reads
//! them for the running build, as [`Field`]s resolved by name do at the low
//! level.

mod kinds;

use core::marker::PhantomData;

use crate::abi::{EventTypeInfo, GameEventData, Header, Str};
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
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
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
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let died = load.event_type("keen::ecs::EntityDiedEvent")?.hash();
    ///     load.on_every_raw_game_event(move |event| {
    ///         if event.type_hash() == died {
    ///             enshwick::debug!("an entity died");
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
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
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
    /// use enshwick::ecs::Entity;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let died = load.event_type("keen::ecs::EntityDiedEvent")?;
    ///     let dead = died.field::<Entity>(load, "deadEntityId")?;
    ///     load.on_raw_game_event(died, move |event| {
    ///         if let Some(entity) = event.get(dead) {
    ///             enshwick::info!("entity {} died", entity.0);
    ///         }
    ///     })
    /// }
    /// ```
    pub fn field<T: Scalar>(self, load: &Load<'_>, name: &str) -> Result<Field<T>, Off> {
        self.inherited_field(load, self.name, name)
    }

    /// The field `name` that this type inherits from its base type
    /// `declared_in`: reflection lists a field under the type that declares
    /// it.
    ///
    /// # Errors
    ///
    /// As for [`EventType::field`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::Entity;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
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

impl Load<'_> {
    /// Every game event type of the running build, read from the game's
    /// reflection, for tools that name what they see: the event types that
    /// exist, not only those the server raises.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader cannot list them.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let types = load.event_types()?;
    ///     enshwick::info!("{} game event types on this build", types.len());
    ///     Ok(())
    /// }
    /// ```
    pub fn event_types(&self) -> Result<Vec<EventType>, Off> {
        let api = self.host().api();
        let me = self.host().handle();
        let mut count = 0;
        let status = (api.event_types)(me, core::ptr::null_mut(), 0, &raw mut count);
        crate::load::ok(status, (), || "event types".to_owned())?;
        let empty = EventTypeInfo {
            header: Header::of::<EventTypeInfo>(),
            name: Str::new(""),
            hash: 0,
            size: 0,
        };
        let mut infos = vec![empty; usize::try_from(count).unwrap_or(0)];
        let capacity = u32::try_from(infos.len()).unwrap_or(0);
        let mut written = 0;
        let status = (api.event_types)(me, infos.as_mut_ptr(), capacity, &raw mut written);
        crate::load::ok(status, (), || "event types".to_owned())?;
        infos.truncate(usize::try_from(written).unwrap_or(0));
        Ok(infos
            .iter()
            .filter_map(|info| {
                // SAFETY: every entry left after the truncation above was
                // written by `HostApi::event_types`, whose names stay
                // readable and unchanged for the rest of the process, so
                // `'static` holds.
                let name: &'static str = unsafe { info.name.as_str() }?;
                Some(EventType {
                    name,
                    hash: info.hash,
                    size: info.size,
                })
            })
            .collect())
    }
}

impl EventType {
    /// The `T` at `offset` of this type, inside its size.
    pub(crate) fn field_at<T: Scalar>(self, offset: u64) -> Option<Field<T>> {
        Field::within(self.hash, self.size, offset)
    }

    /// The `T` at the offset the loader resolves for the engine id `id`,
    /// inside this type's size.
    pub(crate) fn field_by_id<T: Scalar>(self, load: &Load<'_>, id: &str) -> Result<Field<T>, Off> {
        let offset = load.constant(id)?;
        self.field_at(offset)
            .ok_or_else(|| Off::new(format!("field {id}: past the end of {}", self.name)))
    }
}

/// A plain value a [`Field`] reads from an event: an integer, a float, a
/// `bool` or an [`Entity`]; implemented by those types only.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::Scalar;
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

    /// Writes the value to the first `size_of::<Self>()` bytes of `bytes`;
    /// false when they are too few.
    #[doc(hidden)]
    fn to_bytes(self, bytes: &mut [u8]) -> bool;
}

macro_rules! scalars {
    ($($ty:ty),*) => {$(
        impl sealed::Sealed for $ty {}

        impl Scalar for $ty {
            fn from_bytes(bytes: &[u8]) -> Option<Self> {
                Some(<$ty>::from_le_bytes(bytes.get(..size_of::<$ty>())?.try_into().ok()?))
            }

            fn to_bytes(self, bytes: &mut [u8]) -> bool {
                bytes
                    .get_mut(..size_of::<$ty>())
                    .map(|out| out.copy_from_slice(&self.to_le_bytes()))
                    .is_some()
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

    fn to_bytes(self, bytes: &mut [u8]) -> bool {
        u8::from(self).to_bytes(bytes)
    }
}

impl sealed::Sealed for Entity {}

impl Scalar for Entity {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        u32::from_bytes(bytes).map(Entity)
    }

    fn to_bytes(self, bytes: &mut [u8]) -> bool {
        self.0.to_bytes(bytes)
    }
}

/// A field of one reflected type, an event or a component, resolved by name
/// for the running build.
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

impl<T: Scalar> Field<T> {
    /// The `T` at `offset` of the type `type_hash` whose values are `size`
    /// bytes, when it lies inside them.
    pub(crate) fn within(type_hash: u32, size: u32, offset: u64) -> Option<Self> {
        let end = offset.checked_add(size_of::<T>() as u64)?;
        (end <= u64::from(size)).then_some(Self {
            type_hash,
            offset: u32::try_from(offset).ok()?,
            _value: PhantomData,
        })
    }

    /// The field's value in `bytes`, a value of its type.
    pub(crate) fn read(self, bytes: &[u8]) -> Option<T> {
        T::from_bytes(bytes.get(usize::try_from(self.offset).ok()?..)?)
    }

    /// The hash of the type the field belongs to.
    pub(crate) const fn type_hash(self) -> u32 {
        self.type_hash
    }
}

impl<T> Field<T> {
    /// The field's offset in its type.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
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
    /// use enshwick::game_events::RawGameEvent;
    ///
    /// let bytes = [0u8; 16];
    /// let event = RawGameEvent::new(0x0042, &bytes);
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
        field.read(self.bytes)
    }

    /// The `T` at `offset`, for layouts reflection does not name, such as an
    /// element of an array field; `None` past the event's bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::game_events::RawGameEvent;
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
    /// - `raw` is null, or points at an aligned [`GameEventData`] the loader
    ///   wrote, readable and unchanged for the call.
    /// - Unless the event's `data` is null or its `size` zero, `data` is
    ///   readable for `size` bytes, alive and unchanged for `'a`.
    pub(crate) unsafe fn from_raw(raw: *const GameEventData) -> Option<Self> {
        // SAFETY: `raw` is null or an aligned event readable for the call by
        // the first bullet of this function's `# Safety`; `as_ref` maps null
        // to `None`.
        let data = unsafe { raw.as_ref() }?;
        let size = usize::try_from(data.size).ok()?;
        let bytes = if size == 0 || data.data.is_null() {
            &[][..]
        } else {
            // SAFETY: `data` is non-null and `size` non-zero, checked just
            // above, so the second bullet of this function's `# Safety` makes
            // `data` readable for `size` bytes, alive and unchanged for `'a`.
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

    #[test]
    fn scalars_write_little_endian_and_only_inside_the_bytes() {
        let mut bytes = [0u8; 6];
        assert!(0x0403_0201u32.to_bytes(&mut bytes));
        assert!(true.to_bytes(&mut bytes[4..]));
        assert!(!1u16.to_bytes(&mut bytes[5..]));
        assert_eq!(bytes, [1, 2, 3, 4, 1, 0]);
        let field = Field::<i16>::within(9, 6, 4).unwrap();
        assert!((-2i16).to_bytes(&mut bytes[4..]));
        assert_eq!(field.read(&bytes), Some(-2));
        assert!(Field::<u32>::within(9, 6, 4).is_none());
    }
}
