//! Buffs: their types, the active ones and requests to apply them.

use crate::abi::{BuffTypeId, Entity, Header};
use crate::content::Guid;

/// A buff or debuff type of the running game, written by
/// [`HostApi::buff_types`](super::HostApi::buff_types).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuffTypeInfo, Header};
///
/// let rested = enshwick::abi::headed!(BuffTypeInfo {
///     id: enshwick::buffs::BuffTypeId::new(0xcdbe_3e80),
///     slot: BuffTypeInfo::RESTED,
///     flags: BuffTypeInfo::SAVED,
///     name: 1_237_250_642,
///     duration_ns: 5_000_000_000,
/// });
/// assert_eq!(rested.slot, BuffTypeInfo::RESTED);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct BuffTypeInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The type's id: FNV-1a 32 of its GUID's bytes.
    pub id: BuffTypeId,
    /// Where an entity holds it: [`BuffTypeInfo::FOOD`],
    /// [`BuffTypeInfo::BUFF`], [`BuffTypeInfo::DEBUFF`],
    /// [`BuffTypeInfo::RESTED`] or [`BuffTypeInfo::TERRAIN`].
    pub slot: u16,
    /// [`BuffTypeInfo::SAVED`], [`BuffTypeInfo::HIDDEN`] or none.
    pub flags: u16,
    /// The `LocaTag` of its name, which a client shows.
    pub name: u32,
    /// How long it lasts by default, in game nanoseconds.
    pub duration_ns: i64,
}

impl_headed!(BuffTypeInfo);

impl BuffTypeInfo {
    /// A food buff.
    pub const FOOD: u16 = 0;
    /// A buff.
    pub const BUFF: u16 = 1;
    /// A debuff.
    pub const DEBUFF: u16 = 2;
    /// Rested.
    pub const RESTED: u16 = 3;
    /// A debuff of the ground the entity stands in.
    pub const TERRAIN: u16 = 4;
    /// The game saves it with the character.
    pub const SAVED: u16 = 1;
    /// The game shows it nowhere.
    pub const HIDDEN: u16 = 2;
}

/// A buff an entity has, written by [`HostApi::buffs`](super::HostApi::buffs).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ActiveBuff, Header};
///
/// let fed = enshwick::abi::headed!(ActiveBuff {
///     id: enshwick::buffs::BuffTypeId::new(7),
///     entity: enshwick::Entity::from_engine(4100),
///     slot: 0,
///     level: 0,
///     duration_ns: 600_000_000_000,
///     remaining_ns: 120_000_000_000,
/// });
/// assert!(fed.remaining_ns < fed.duration_ns);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct ActiveBuff {
    /// Size and version of this struct.
    pub header: Header,
    /// Its type's id.
    pub id: BuffTypeId,
    /// The buff's own entity, which carries its effect.
    pub entity: Entity,
    /// Where the entity holds it, as [`BuffTypeInfo::slot`].
    pub slot: u32,
    /// Its level; 0 for none.
    pub level: u32,
    /// How long it lasts in all, in game nanoseconds.
    pub duration_ns: i64,
    /// How long it has left, in game nanoseconds.
    pub remaining_ns: i64,
}

impl_headed!(ActiveBuff);

/// A buff to give an entity, for [`HostApi::apply_buff`](super::HostApi::apply_buff).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuffRequest, Header};
///
/// let request = enshwick::abi::headed!(BuffRequest {
///     entity: enshwick::Entity::from_engine(2),
///     id: enshwick::buffs::BuffTypeId::new(7),
///     level: 0,
///     reserved: 0,
///     duration_ns: 0,
/// });
/// assert_eq!(request.duration_ns, 0, "the type's own duration");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct BuffRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity to give it.
    pub entity: Entity,
    /// The type's id.
    pub id: BuffTypeId,
    /// Its level, 1 to 255; 0 for none.
    pub level: u32,
    /// Zero.
    pub reserved: u32,
    /// How long it lasts, in game nanoseconds; 0 for the type's own.
    pub duration_ns: i64,
}

impl_headed!(BuffRequest);

/// A buff type of a mod's own, for
/// [`HostApi::buff_type_add`](super::HostApi::buff_type_add): a copy of the
/// vanilla type `like` with the values `changes` names set from this struct.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuffTypeAdd, Header};
/// use enshwick::content::{Guid, Id};
///
/// let lucky = enshwick::abi::headed!(BuffTypeAdd {
///     guid: Id::new("you.lucky:lucky").guid(),
///     name: Guid::NIL,
///     description: Guid::NIL,
///     icon: Guid::NIL,
///     life_time_ns: 300_000_000_000,
///     like: enshwick::buffs::BuffTypeId::new(0xf9b3_64f6),
///     changes: BuffTypeAdd::LIFE_TIME,
///     slot: 0,
///     flags: BuffTypeAdd::PLAIN,
/// });
/// assert_eq!(lucky.changes & BuffTypeAdd::NAME, 0, "the vanilla name stays");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct BuffTypeAdd {
    /// Size and version of this struct.
    pub header: Header,
    /// Its content id's GUID, which the calling mod declared with
    /// `declare_content`; its game id is the FNV-1a 32 of these bytes.
    pub guid: Guid,
    /// The tag of its name, with [`BuffTypeAdd::NAME`].
    pub name: Guid,
    /// The tag of its description, with [`BuffTypeAdd::DESCRIPTION`].
    pub description: Guid,
    /// The UI image of its icon, with [`BuffTypeAdd::ICON`]. A client names
    /// it only once its UI knows that image, a game one or a served one, and
    /// shows the copied type's icon until then.
    pub icon: Guid,
    /// How long it lasts, in game nanoseconds, 0 or more, with
    /// [`BuffTypeAdd::LIFE_TIME`].
    pub life_time_ns: i64,
    /// The game's buff type it copies.
    pub like: BuffTypeId,
    /// The values it sets: the change bits of this struct, or none.
    pub changes: u16,
    /// Where an entity holds it, as [`BuffTypeInfo::slot`], with
    /// [`BuffTypeAdd::SLOT`].
    pub slot: u8,
    /// [`BuffTypeAdd::HIDE_IN_UI`], [`BuffTypeAdd::HIDE_DURATION_IN_UI`] and
    /// [`BuffTypeAdd::SAVED`] for the values `changes` sets, and
    /// [`BuffTypeAdd::PLAIN`], or none.
    pub flags: u8,
}

impl_headed!(BuffTypeAdd);

impl BuffTypeAdd {
    /// Sets its name.
    pub const NAME: u16 = 1;
    /// Sets its description.
    pub const DESCRIPTION: u16 = 1 << 1;
    /// Sets its icon.
    pub const ICON: u16 = 1 << 2;
    /// Sets how long it lasts.
    pub const LIFE_TIME: u16 = 1 << 3;
    /// Sets its slot.
    pub const SLOT: u16 = 1 << 4;
    /// Sets whether the game shows it nowhere, from
    /// [`BuffTypeAdd::HIDE_IN_UI`].
    pub const SET_HIDE_IN_UI: u16 = 1 << 5;
    /// Sets whether the HUD hides how long it has left, from
    /// [`BuffTypeAdd::HIDE_DURATION_IN_UI`].
    pub const SET_HIDE_DURATION_IN_UI: u16 = 1 << 6;
    /// Sets whether the game saves it, from [`BuffTypeAdd::SAVED`].
    pub const SET_SAVED: u16 = 1 << 7;
    /// Every change it may set.
    pub const ALL_CHANGES: u16 = (1 << 8) - 1;
    /// The game shows it nowhere.
    pub const HIDE_IN_UI: u8 = 1;
    /// The HUD hides how long it has left.
    pub const HIDE_DURATION_IN_UI: u8 = 1 << 1;
    /// The game saves it with a character.
    pub const SAVED: u8 = 1 << 2;
    /// It drops what the vanilla type runs: its impact program and values,
    /// tooltip lines, helpers, follow-up and effects.
    pub const PLAIN: u8 = 1 << 3;
    /// Every flag it may hold.
    pub const ALL_FLAGS: u8 = (1 << 4) - 1;
}
