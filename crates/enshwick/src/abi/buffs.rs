//! Buffs: their types, the active ones and requests to apply them.

use crate::abi::{Entity, Header};

/// A buff or debuff type of the running game, written by
/// [`HostApi::buff_types`](super::HostApi::buff_types).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuffTypeInfo, Header};
///
/// let rested = BuffTypeInfo {
///     header: Header::of::<BuffTypeInfo>(),
///     id: 0xcdbe_3e80,
///     slot: BuffTypeInfo::RESTED,
///     flags: BuffTypeInfo::SAVED,
///     name: 1_237_250_642,
///     duration: 5_000_000_000,
/// };
/// assert_eq!(rested.slot, BuffTypeInfo::RESTED);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct BuffTypeInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The type's id: FNV-1a 32 of its GUID's bytes.
    pub id: u32,
    /// Where an entity holds it: [`BuffTypeInfo::FOOD`],
    /// [`BuffTypeInfo::BUFF`], [`BuffTypeInfo::DEBUFF`],
    /// [`BuffTypeInfo::RESTED`] or [`BuffTypeInfo::TERRAIN`].
    pub slot: u16,
    /// [`BuffTypeInfo::SAVED`], [`BuffTypeInfo::HIDDEN`] or none.
    pub flags: u16,
    /// The `LocaTag` of its name, which a client shows.
    pub name: u32,
    /// How long it lasts by default, in game nanoseconds.
    pub duration: i64,
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
/// let fed = ActiveBuff {
///     header: Header::of::<ActiveBuff>(),
///     id: 7,
///     entity: enshwick::Entity(4100),
///     slot: 0,
///     level: 0,
///     duration: 600_000_000_000,
///     remaining: 120_000_000_000,
/// };
/// assert!(fed.remaining < fed.duration);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct ActiveBuff {
    /// Size and version of this struct.
    pub header: Header,
    /// Its type's id.
    pub id: u32,
    /// The buff's own entity, which carries its effect.
    pub entity: Entity,
    /// Where the entity holds it, as [`BuffTypeInfo::slot`].
    pub slot: u32,
    /// Its level; 0 for none.
    pub level: u32,
    /// How long it lasts in all, in game nanoseconds.
    pub duration: i64,
    /// How long it has left, in game nanoseconds.
    pub remaining: i64,
}

impl_headed!(ActiveBuff);

/// A buff to give an entity, for [`HostApi::apply_buff`](super::HostApi::apply_buff).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuffRequest, Header};
///
/// let request = BuffRequest {
///     header: Header::of::<BuffRequest>(),
///     entity: enshwick::Entity(2),
///     id: 7,
///     level: 0,
///     reserved: 0,
///     duration: 0,
/// };
/// assert_eq!(request.duration, 0, "the type's own duration");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct BuffRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity to give it.
    pub entity: Entity,
    /// The type's id.
    pub id: u32,
    /// Its level, 1 to 255; 0 for none.
    pub level: u32,
    /// Zero.
    pub reserved: u32,
    /// How long it lasts, in game nanoseconds; 0 for the type's own.
    pub duration: i64,
}

impl_headed!(BuffRequest);
