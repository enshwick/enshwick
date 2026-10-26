//! Stats and affixes: definitions, modifiers and changes.

use core::ffi::c_void;

use crate::abi::{Header, Str};

/// A stat kind's definition, for [`HostApi::stat_open`](super::HostApi::stat_open) and
/// [`HostApi::stat_info`](super::HostApi::stat_info).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, StatDefData, Str};
///
/// let crit = StatDefData {
///     header: Header::of::<StatDefData>(),
///     ty: StatDefData::ADDITIVE,
///     format: StatDefData::PERCENT,
///     reserved: 0,
///     default: 0.0,
///     min: 0.0,
///     max: 1.0,
///     text: Str::new("stat.crit"),
/// };
/// assert!(crit.min <= crit.default && crit.default <= crit.max);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct StatDefData {
    /// Size and version of this struct.
    pub header: Header,
    /// [`StatDefData::ADDITIVE`], [`StatDefData::MULTIPLICATIVE`] or
    /// [`StatDefData::FLAG`].
    pub ty: u8,
    /// How the value reads: [`StatDefData::NUMBER`],
    /// [`StatDefData::SIGNED`] or [`StatDefData::PERCENT`].
    pub format: u8,
    /// Zero.
    pub reserved: u16,
    /// The value without modifiers.
    pub default: f32,
    /// The least value.
    pub min: f32,
    /// The greatest value.
    pub max: f32,
    /// The text key that names the stat, in its mod's texts.
    pub text: Str,
}

impl_headed!(StatDefData);

impl StatDefData {
    /// A sum of points, such as extra health.
    pub const ADDITIVE: u8 = 0;
    /// A factor, 1 without modifiers, such as a damage multiplier.
    pub const MULTIPLICATIVE: u8 = 1;
    /// On or off: any value but 0 is on.
    pub const FLAG: u8 = 2;
    /// A plain number.
    pub const NUMBER: u8 = 0;
    /// A number with its sign.
    pub const SIGNED: u8 = 1;
    /// A fraction shown as a percentage.
    pub const PERCENT: u8 = 2;
}

/// A stat kind as the loader numbers it: its index for calls and its key,
/// the FNV-1a 32 hash of its full id, for what is stored.
///
/// # Examples
///
/// ```
/// use enshwick::abi::StatRef;
///
/// let health = StatRef { index: 0, key: 0x1234 };
/// assert_eq!(health.index, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct StatRef {
    /// The index in this process.
    pub index: u32,
    /// The FNV-1a 32 hash of the full id, the same in every process.
    pub key: u32,
}

/// One modifier of a stat, by index; written as arrays.
///
/// # Examples
///
/// ```
/// use enshwick::abi::StatModifierData;
///
/// let more = StatModifierData { stat: 0, op: StatModifierData::ADD, reserved: [0; 3], value: 50.0 };
/// assert_eq!(more.op, StatModifierData::ADD);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Zeroable)]
pub struct StatModifierData {
    /// The stat's index.
    pub stat: u32,
    /// [`StatModifierData::ADD`], [`StatModifierData::MULTIPLY`],
    /// [`StatModifierData::MAX`] or [`StatModifierData::OVERRIDE`].
    pub op: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// The modifier's value.
    pub value: f32,
}

impl StatModifierData {
    /// Adds the value.
    pub const ADD: u8 = 0;
    /// Multiplies by the value.
    pub const MULTIPLY: u8 = 1;
    /// Raises the result to at least the value.
    pub const MAX: u8 = 2;
    /// Sets the result.
    pub const OVERRIDE: u8 = 3;
}

/// Sets or removes a mod's modifier on an entity, for
/// [`HostApi::stat_modify`](super::HostApi::stat_modify).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, StatModifierData, StatModify};
///
/// let blessing = StatModify {
///     header: Header::of::<StatModify>(),
///     entity: 2,
///     key: 7,
///     modifier: StatModifierData { stat: 0, op: StatModifierData::ADD, reserved: [0; 3], value: 50.0 },
///     flags: 0,
///     duration_ms: 60_000,
/// };
/// assert_eq!(blessing.flags & StatModify::REMOVE, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct StatModify {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity.
    pub entity: u32,
    /// The mod's own key; a modifier with the same mod, key and stat
    /// replaces the one before.
    pub key: u32,
    /// The modifier.
    pub modifier: StatModifierData,
    /// [`StatModify::REMOVE`], [`StatModify::SAVED`].
    pub flags: u32,
    /// How long it lasts in milliseconds; 0 until removed.
    pub duration_ms: u64,
}

impl_headed!(StatModify);

impl StatModify {
    /// Removes the modifier instead.
    pub const REMOVE: u32 = 1;
    /// Kept with the world for the player's account, and given back when
    /// the player joins again; players only.
    pub const SAVED: u32 = 1 << 1;
}

/// A stat whose value changed on an entity, for the handlers registered
/// with [`HostApi::stats_changed`](super::HostApi::stats_changed).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, StatChange, StatRef};
///
/// let change = StatChange { header: Header::of::<StatChange>(), entity: 2, stat: StatRef { index: 0, key: 0x1234 }, old: 0.0, new: 50.0, reserved: 0 };
/// assert!(change.new > change.old);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct StatChange {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity.
    pub entity: u32,
    /// The stat.
    pub stat: StatRef,
    /// The value before.
    pub old: f32,
    /// The value now.
    pub new: f32,
    /// Zero.
    pub reserved: u32,
}

impl_headed!(StatChange);

/// A handler of stat changes: the change and the pointer given at
/// registration. It runs on the game thread, after a step.
pub type StatChangeFn = unsafe extern "system" fn(change: *const StatChange, user: *mut c_void);

/// One modifier that counts toward an entity's stat, and where it comes
/// from, written by [`HostApi::stat_sources`](super::HostApi::stat_sources) as an array.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{StatModifierData, StatSource};
///
/// let ring = StatSource { kind: StatSource::ITEM, op: StatModifierData::ADD, reserved: 0, stat: 0, value: 20.0, id: 900, owner: 0x5e01 };
/// assert_eq!(ring.kind, StatSource::ITEM);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatSource {
    /// [`StatSource::ITEM`], [`StatSource::BUFF`] or
    /// [`StatSource::ENTITY`].
    pub kind: u8,
    /// The operation, as [`StatModifierData::op`].
    pub op: u8,
    /// Zero.
    pub reserved: u16,
    /// The stat's index.
    pub stat: u32,
    /// The modifier's value.
    pub value: f32,
    /// The item's entity, the buff id or the modifier's key.
    pub id: u32,
    /// The item id, 0, or the index of the mod that set the modifier.
    pub owner: u32,
}

/// An affix's definition, for [`HostApi::affix_open`](super::HostApi::affix_open).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{AffixDefData, AffixModifierData, Header, StatModifierData, Str};
///
/// let ranges = [AffixModifierData { stat: 0, op: StatModifierData::ADD, reserved: [0; 3], min: 10.0, max: 30.0, step: 1.0 }];
/// let sturdy = AffixDefData {
///     header: Header::of::<AffixDefData>(),
///     text: Str::new("affix.sturdy"),
///     group: Str::EMPTY,
///     modifiers: ranges.as_ptr(),
///     modifier_count: 1,
///     weight: 100,
///     categories: 1 << 1,
///     rarity: 0,
///     reserved: [0; 3],
/// };
/// assert_eq!(sturdy.modifier_count, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct AffixDefData {
    /// Size and version of this struct.
    pub header: Header,
    /// The text key that names the affix, in its mod's texts.
    pub text: Str,
    /// Affixes of one group never roll together on an item; empty for none.
    pub group: Str,
    /// Its modifiers' ranges.
    pub modifiers: *const AffixModifierData,
    /// How many.
    pub modifier_count: u32,
    /// How likely it rolls against the others.
    pub weight: u32,
    /// The item categories it rolls on, a bit per `ItemCategory`.
    pub categories: u32,
    /// Its rarity, 0 (common) to 4 (legendary).
    pub rarity: u8,
    /// Zero.
    pub reserved: [u8; 3],
}

impl_headed!(AffixDefData);

/// One modifier of an affix with the range its value rolls in; read as
/// arrays.
///
/// # Examples
///
/// See [`AffixDefData`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AffixModifierData {
    /// The stat's index.
    pub stat: u32,
    /// The operation, as [`StatModifierData::op`].
    pub op: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// The least value.
    pub min: f32,
    /// The greatest value.
    pub max: f32,
    /// The value rolls in steps of this from `min`; 0 for any value.
    pub step: f32,
}

/// Rolls affixes onto an item, for [`HostApi::affix_roll`](super::HostApi::affix_roll).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{AffixRoll, Header};
///
/// let roll = AffixRoll { header: Header::of::<AffixRoll>(), pide: 900, item: 7, min: 1, max: 2, max_rarity: 4, flags: AffixRoll::ONCE, seed: 42 };
/// assert!(roll.min <= roll.max);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct AffixRoll {
    /// Size and version of this struct.
    pub header: Header,
    /// The item's entity.
    pub pide: u32,
    /// The item id it holds.
    pub item: u32,
    /// The fewest affixes.
    pub min: u8,
    /// The most affixes.
    pub max: u8,
    /// The rarest an affix may be, 0 to 4.
    pub max_rarity: u8,
    /// [`AffixRoll::ONCE`].
    pub flags: u8,
    /// The seed of the roll: the same seed, item and affixes give the same
    /// result.
    pub seed: u64,
}

impl_headed!(AffixRoll);

impl AffixRoll {
    /// Rolls only an item that never rolled; others answer
    /// [`Status::REFUSED`](super::Status::REFUSED).
    pub const ONCE: u8 = 1;
}

/// One line of what an item gives, written by [`HostApi::item_lines`](super::HostApi::item_lines) as an
/// array.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ItemLine, StatModifierData};
///
/// let line = ItemLine { modifier: StatModifierData { stat: 0, op: StatModifierData::ADD, reserved: [0; 3], value: 20.0 }, affix: 0 };
/// assert_eq!(line.affix, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemLine {
    /// The modifier.
    pub modifier: StatModifierData,
    /// The index of the affix it comes from, plus one; 0 for the item's own.
    pub affix: u32,
}

impl StatSource {
    /// An item the entity wears.
    pub const ITEM: u8 = 0;
    /// A buff the entity has.
    pub const BUFF: u8 = 1;
    /// A modifier a mod set on the entity.
    pub const ENTITY: u8 = 2;
}
