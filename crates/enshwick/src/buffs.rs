//! Buffs and debuffs in the server's world: the types the game knows, the
//! buffs an entity has, giving one and removing one.
//!
//! A buff type is named by its id, the FNV-1a 32 hash of its GUID, the way
//! the game's content names it; [`BuffTypeId::from_guid`] makes one from a GUID
//! and [`Buffs::types`] lists every type of the running game. A buff given
//! by a mod starts at once and lasts the time asked; one the entity has
//! already is renewed. Clients see the change as the game's own, and the
//! game saves the buffs it saves with a character.
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or an action; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick::actions::{Action, Arg, Outcome, Value};
//! use core::time::Duration;
//! use enshwick::buffs::{Apply, BuffTypeId};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let buffs = load.buffs();
//!     let actions = load.actions(enshwick::i18n::Texts::default())?;
//!     let bless = Action::new("bless", "A buff for a minute")
//!         .admins()
//!         .arg(Arg::number("Buff id", 0.0..=f64::from(u32::MAX), 1.0));
//!     actions.add(bless, move |request| {
//!         let [Value::Number(id)] = request.args else {
//!             return Outcome::refused("Name a buff");
//!         };
//!         let how = Apply::new().lasting(Duration::from_mins(1));
//!         Outcome::done(format!("{:?}", buffs.apply(request.player.entity, BuffTypeId(*id as u32), how)))
//!     })
//! }
//! ```

use core::time::Duration;

use crate::Entity;
use crate::WorldError;
use crate::abi::{ActiveBuff, BuffRequest, BuffTypeInfo, Header, Status};
use crate::{Host, Load};

pub use crate::abi::BuffTypeId;

/// Where an entity holds a buff.
///
/// # Examples
///
/// See [`BuffType`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BuffSlotType {
    /// A food buff.
    Food,
    /// A buff.
    Buff,
    /// A debuff.
    Debuff,
    /// Rested.
    Rested,
    /// A debuff of the ground the entity stands in.
    TerrainDebuff,
    /// A kind this API does not name.
    Other(u16),
}

impl BuffSlotType {
    const fn from_slot(slot: u16) -> Self {
        match slot {
            BuffTypeInfo::FOOD => Self::Food,
            BuffTypeInfo::BUFF => Self::Buff,
            BuffTypeInfo::DEBUFF => Self::Debuff,
            BuffTypeInfo::RESTED => Self::Rested,
            BuffTypeInfo::TERRAIN => Self::TerrainDebuff,
            other => Self::Other(other),
        }
    }
}

/// A buff type of the running game, from [`Buffs::types`].
///
/// # Examples
///
/// ```
/// use enshwick::buffs::BuffSlotType;
///
/// fn food(buffs: &enshwick::buffs::Buffs) -> usize {
///     buffs.types().iter().filter(|ty| ty.slot == BuffSlotType::Food).count()
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[expect(
    clippy::struct_field_names,
    reason = "the fields take the game's names, `buffTypeId` among them"
)]
pub struct BuffType {
    /// Its id.
    pub buff_type_id: BuffTypeId,
    /// Where an entity holds it.
    pub slot: BuffSlotType,
    /// How long it lasts by default, in game time.
    pub default_life_time: Duration,
    /// Whether the game saves it with a character.
    pub is_saved: bool,
    /// Whether the game shows it nowhere.
    pub hide_in_ui: bool,
    /// The `LocaTag` of its name, which a client shows.
    pub name: u32,
}

/// A buff an entity has, from [`Buffs::of`].
///
/// # Examples
///
/// ```
/// fn longest(buffs: &enshwick::buffs::Buffs, entity: enshwick::Entity) -> Option<u32> {
///     let on = buffs.of(entity).ok()?;
///     on.iter().max_by_key(|buff| buff.remaining).map(|buff| buff.type_id.0)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[expect(
    clippy::struct_field_names,
    reason = "the fields take the game's names, `buffEntityId` among them"
)]
pub struct Buff {
    /// Its type.
    pub type_id: BuffTypeId,
    /// The buff's own entity, which carries its effect.
    pub buff_entity_id: Entity,
    /// Where the entity holds it, its type's slot.
    pub slot: BuffSlotType,
    /// Its level, when it has one.
    pub level: Option<u32>,
    /// How long it lasts in all, in game time.
    pub life_time: Duration,
    /// How long it has left, in game time; the name is enshwick's.
    pub remaining: Duration,
}

/// How a buff is given, for [`Buffs::apply`].
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick::buffs::Apply;
///
/// let strong = Apply::new().level(3).lasting(Duration::from_mins(5));
/// assert_eq!(strong.level, Some(3));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Apply {
    /// How long it lasts, in game time; `None` for the type's own.
    pub duration: Option<Duration>,
    /// Its level, for a new buff; `None` for none.
    pub level: Option<u8>,
}

impl Apply {
    /// The type's own duration, no level.
    ///
    /// # Examples
    ///
    /// See [`Apply`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            duration: None,
            level: None,
        }
    }

    /// Lasting `duration` of game time.
    ///
    /// # Examples
    ///
    /// See [`Apply`].
    #[must_use]
    pub const fn lasting(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// At `level`, from 1.
    ///
    /// # Examples
    ///
    /// See [`Apply`].
    #[must_use]
    pub const fn level(mut self, level: u8) -> Self {
        self.level = Some(level);
        self
    }
}

/// The buffs of the server's world; get it from [`Load::buffs`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Buffs {
    host: Host,
}

fn nanos(duration: i64) -> Duration {
    Duration::from_nanos(u64::try_from(duration).unwrap_or(0))
}

impl Buffs {
    /// Every buff and debuff type of the running game; empty off the game
    /// thread or while no world runs.
    ///
    /// # Examples
    ///
    /// See [`BuffType`].
    #[must_use]
    pub fn types(&self) -> Vec<BuffType> {
        let blank = BuffTypeInfo {
            header: Header::of::<BuffTypeInfo>(),
            id: BuffTypeId::default(),
            slot: 0,
            flags: 0,
            name: 0,
            duration: 0,
        };
        let fill = |out: *mut BuffTypeInfo, capacity: u32, count: *mut u32| {
            (self.host.api().buff_types)(self.host.handle(), out, capacity, count)
        };
        list(blank, fill)
            .unwrap_or_default()
            .iter()
            .map(|info| BuffType {
                buff_type_id: info.id,
                slot: BuffSlotType::from_slot(info.slot),
                default_life_time: nanos(info.duration),
                is_saved: info.flags & BuffTypeInfo::SAVED != 0,
                hide_in_ui: info.flags & BuffTypeInfo::HIDDEN != 0,
                name: info.name,
            })
            .collect()
    }

    /// The buffs `entity` has.
    ///
    /// # Errors
    ///
    /// [`WorldError::Lacks`] for an entity that holds no buffs or is not in
    /// the world, [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Missing`] on a build without buffs.
    ///
    /// # Examples
    ///
    /// See [`Buff`].
    pub fn of(&self, entity: Entity) -> Result<Vec<Buff>, WorldError> {
        let blank = ActiveBuff {
            header: Header::of::<ActiveBuff>(),
            id: BuffTypeId::default(),
            entity: Entity::NONE,
            slot: 0,
            level: 0,
            duration: 0,
            remaining: 0,
        };
        let fill = |out: *mut ActiveBuff, capacity: u32, count: *mut u32| {
            (self.host.api().buffs)(self.host.handle(), entity, out, capacity, count)
        };
        Ok(list(blank, fill)?
            .iter()
            .map(|buff| Buff {
                type_id: buff.id,
                buff_entity_id: buff.entity,
                slot: BuffSlotType::from_slot(u16::try_from(buff.slot).unwrap_or(u16::MAX)),
                level: (buff.level != 0).then_some(buff.level),
                life_time: nanos(buff.duration),
                remaining: nanos(buff.remaining),
            })
            .collect())
    }

    /// Whether `entity` has a buff of type `id`; `false` when it holds no
    /// buffs or off the game thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::buffs::BuffTypeId;
    ///
    /// fn starving(buffs: &enshwick::buffs::Buffs, player: enshwick::Entity) -> bool {
    ///     buffs.has(player, BuffTypeId(0x6ad7_cfc4))
    /// }
    /// ```
    #[must_use]
    pub fn has(&self, entity: Entity, id: BuffTypeId) -> bool {
        self.of(entity)
            .is_ok_and(|on| on.iter().any(|buff| buff.type_id == id))
    }

    /// Gives `entity` a buff of type `id`, at once, as the game restores a
    /// character's saved buffs; one it has already is renewed to the
    /// duration asked and keeps its level. The game checks no faction or
    /// death: the mod decides who gets it.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a type the game does not know, or one
    /// removed from the entity in this step (the game frees its place in
    /// the next); [`WorldError::Lacks`] for an entity that holds no buffs or
    /// is not in the world; [`WorldError::WrongThread`] off the game thread;
    /// [`WorldError::Missing`] on a build without buffs.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn apply(&self, entity: Entity, id: BuffTypeId, how: Apply) -> Result<(), WorldError> {
        let duration = how.duration.map_or(0, |duration| {
            i64::try_from(duration.as_nanos())
                .unwrap_or(i64::MAX)
                .max(1)
        });
        let request = BuffRequest {
            header: Header::of::<BuffRequest>(),
            entity,
            id,
            level: how.level.map_or(0, u32::from),
            reserved: 0,
            duration,
        };
        WorldError::check(
            (self.host.api().apply_buff)(self.host.handle(), &raw const request),
            WorldError::Lacks,
        )
    }

    /// Removes every buff of type `id` from `entity`, as the game cleanses
    /// one: it goes in the next step, without what the type does when it
    /// runs out. Returns whether the entity had it.
    ///
    /// # Errors
    ///
    /// As for [`Buffs::of`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::buffs::BuffTypeId;
    ///
    /// fn cure(buffs: &enshwick::buffs::Buffs, player: enshwick::Entity, poison: BuffTypeId) -> bool {
    ///     buffs.remove(player, poison).unwrap_or(false)
    /// }
    /// ```
    pub fn remove(&self, entity: Entity, id: BuffTypeId) -> Result<bool, WorldError> {
        let mut removed = 0u8;
        WorldError::check(
            (self.host.api().remove_buff)(self.host.handle(), entity, id, &raw mut removed),
            WorldError::Lacks,
        )?;
        Ok(removed != 0)
    }
}

/// A list the loader fills: asked once with room for a guess, again with
/// room for all when it was too small.
fn list<T: Copy>(
    blank: T,
    fill: impl Fn(*mut T, u32, *mut u32) -> Status,
) -> Result<Vec<T>, WorldError> {
    let mut items = vec![blank; 64];
    loop {
        let mut count = 0u32;
        let capacity = u32::try_from(items.len()).unwrap_or(u32::MAX);
        WorldError::check(
            fill(items.as_mut_ptr(), capacity, &raw mut count),
            WorldError::Lacks,
        )?;
        let count = count as usize;
        if count <= items.len() {
            items.truncate(count);
            return Ok(items);
        }
        items = vec![blank; count];
    }
}

impl Load<'_> {
    /// The [`Buffs`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn buffs(&mut self) -> Buffs {
        Buffs { host: self.host() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guid_hashes_as_the_game_hashes_it() {
        // FNV-1a 32 of 16 zero bytes.
        let zero = crate::content::Guid([0; 16]);
        assert_eq!(BuffTypeId::from_guid(zero), BuffTypeId::new(0x6969_1905));
    }
}
