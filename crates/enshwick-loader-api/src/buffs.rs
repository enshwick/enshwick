//! Buffs and debuffs in the server's world: the types the game knows, the
//! buffs an entity has, giving one and removing one.
//!
//! A buff type is named by its id, the FNV-1a 32 hash of its GUID, the way
//! the game's content names it; [`BuffId::from_guid`] makes one from a GUID
//! and [`Buffs::types`] lists every type of the running game. A buff given
//! by a mod starts at once and lasts the time asked; one the entity has
//! already is renewed. Clients see the change as the game's own, and the
//! game saves the buffs it saves with a character.
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or a command; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use core::time::Duration;
//! use enshwick_loader_api::buffs::{Apply, BuffId};
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     let buffs = load.buffs();
//!     load.add_command("bless", "gives you a buff for a minute: /bless <buff id>", move |ctx, args| {
//!         let Some(me) = ctx.character() else {
//!             return "the console has no character".into();
//!         };
//!         let Some(id) = args.first().and_then(|a| a.parse().ok()).map(BuffId) else {
//!             return "/bless <buff id>".into();
//!         };
//!         let how = Apply::new().lasting(Duration::from_mins(1));
//!         format!("{:?}", buffs.apply(me, id, how)).into()
//!     })
//! }
//! ```

use core::time::Duration;

use crate::abi::{ActiveBuff, BuffRequest, BuffTypeInfo, Header, Status};
use crate::ecs::Entity;
use crate::world::WorldError;
use crate::{Host, Load};

/// A buff type's id: the FNV-1a 32 hash of its GUID's bytes.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::buffs::BuffId;
///
/// assert_eq!(BuffId::from_guid([0; 16]), BuffId::from_guid([0; 16]));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BuffId(pub u32);

impl BuffId {
    /// The id of the buff type with GUID `guid`, its 16 bytes as the game
    /// stores them.
    ///
    /// # Examples
    ///
    /// See [`BuffId`].
    #[must_use]
    pub const fn from_guid(guid: [u8; 16]) -> Self {
        Self(crate::content::Guid(guid).id())
    }

    /// The id of a buff type a mod serves under the content id `id`
    /// (`<mod>:<name>`); a malformed literal fails the build when used in a
    /// constant.
    ///
    /// # Panics
    ///
    /// On a malformed id.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::buffs::BuffId;
    /// use enshwick_loader_api::content::Guid;
    ///
    /// const WARMTH: BuffId = BuffId::named("my-mod:warmth");
    /// assert_eq!(WARMTH, BuffId::from_guid(Guid::named("my-mod:warmth").0));
    /// ```
    #[must_use]
    pub const fn named(id: &str) -> Self {
        Self::from_guid(crate::content::Guid::named(id).0)
    }
}

/// Where an entity holds a buff.
///
/// # Examples
///
/// See [`BuffType`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BuffKind {
    /// A food buff.
    Food,
    /// A buff.
    Buff,
    /// A debuff.
    Debuff,
    /// Rested.
    Rested,
    /// A debuff of the ground the entity stands in.
    Terrain,
    /// A kind this API does not name.
    Other(u16),
}

impl BuffKind {
    const fn from_slot(slot: u16) -> Self {
        match slot {
            BuffTypeInfo::FOOD => Self::Food,
            BuffTypeInfo::BUFF => Self::Buff,
            BuffTypeInfo::DEBUFF => Self::Debuff,
            BuffTypeInfo::RESTED => Self::Rested,
            BuffTypeInfo::TERRAIN => Self::Terrain,
            other => Self::Other(other),
        }
    }
}

/// A buff type of the running game, from [`Buffs::types`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::buffs::BuffKind;
///
/// fn food(buffs: &enshwick_loader_api::buffs::Buffs) -> usize {
///     buffs.types().iter().filter(|ty| ty.kind == BuffKind::Food).count()
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BuffType {
    /// Its id.
    pub id: BuffId,
    /// Where an entity holds it.
    pub kind: BuffKind,
    /// How long it lasts by default, in game time.
    pub duration: Duration,
    /// Whether the game saves it with a character.
    pub saved: bool,
    /// Whether the game shows it nowhere.
    pub hidden: bool,
    /// The `LocaTag` of its name, which a client shows.
    pub name: u32,
}

/// A buff an entity has, from [`Buffs::of`].
///
/// # Examples
///
/// ```
/// fn longest(buffs: &enshwick_loader_api::buffs::Buffs, entity: enshwick_loader_api::ecs::Entity) -> Option<u32> {
///     let on = buffs.of(entity).ok()?;
///     on.iter().max_by_key(|buff| buff.remaining).map(|buff| buff.id.0)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Buff {
    /// Its type.
    pub id: BuffId,
    /// The buff's own entity, which carries its effect.
    pub entity: Entity,
    /// Where the entity holds it.
    pub kind: BuffKind,
    /// Its level, when it has one.
    pub level: Option<u32>,
    /// How long it lasts in all, in game time.
    pub duration: Duration,
    /// How long it has left, in game time.
    pub remaining: Duration,
}

/// How a buff is given, for [`Buffs::apply`].
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick_loader_api::buffs::Apply;
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
            id: 0,
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
                id: BuffId(info.id),
                kind: BuffKind::from_slot(info.slot),
                duration: nanos(info.duration),
                saved: info.flags & BuffTypeInfo::SAVED != 0,
                hidden: info.flags & BuffTypeInfo::HIDDEN != 0,
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
            id: 0,
            entity: 0,
            slot: 0,
            level: 0,
            duration: 0,
            remaining: 0,
        };
        let fill = |out: *mut ActiveBuff, capacity: u32, count: *mut u32| {
            (self.host.api().buffs)(self.host.handle(), entity.0, out, capacity, count)
        };
        Ok(list(blank, fill)?
            .iter()
            .map(|buff| Buff {
                id: BuffId(buff.id),
                entity: Entity(buff.entity),
                kind: BuffKind::from_slot(u16::try_from(buff.slot).unwrap_or(u16::MAX)),
                level: (buff.level != 0).then_some(buff.level),
                duration: nanos(buff.duration),
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
    /// use enshwick_loader_api::buffs::BuffId;
    ///
    /// fn starving(buffs: &enshwick_loader_api::buffs::Buffs, player: enshwick_loader_api::ecs::Entity) -> bool {
    ///     buffs.has(player, BuffId(0x6ad7_cfc4))
    /// }
    /// ```
    #[must_use]
    pub fn has(&self, entity: Entity, id: BuffId) -> bool {
        self.of(entity)
            .is_ok_and(|on| on.iter().any(|buff| buff.id == id))
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
    pub fn apply(&self, entity: Entity, id: BuffId, how: Apply) -> Result<(), WorldError> {
        let duration = how.duration.map_or(0, |duration| {
            i64::try_from(duration.as_nanos())
                .unwrap_or(i64::MAX)
                .max(1)
        });
        let request = BuffRequest {
            header: Header::of::<BuffRequest>(),
            entity: entity.0,
            id: id.0,
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
    /// use enshwick_loader_api::buffs::BuffId;
    ///
    /// fn cure(buffs: &enshwick_loader_api::buffs::Buffs, player: enshwick_loader_api::ecs::Entity, poison: BuffId) -> bool {
    ///     buffs.remove(player, poison).unwrap_or(false)
    /// }
    /// ```
    pub fn remove(&self, entity: Entity, id: BuffId) -> Result<bool, WorldError> {
        let mut removed = 0u8;
        WorldError::check(
            (self.host.api().remove_buff)(self.host.handle(), entity.0, id.0, &raw mut removed),
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
        assert_eq!(BuffId::from_guid([0; 16]), BuffId(0x6969_1905));
    }
}
