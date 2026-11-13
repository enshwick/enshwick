//! Writes mods ask the server for: a player's level and experience, and items dropped on the ground.

use crate::abi::{Entity, Header, ItemId};

/// A request to drop a stack of new items on the ground, for
/// [`HostApi::drop_items`](super::HostApi::drop_items). ABI 11.24.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_abi as enshwick;
/// use enshwick::abi::{DropRequest, Header};
///
/// let logs = DropRequest {
///     header: Header::of::<DropRequest>(),
///     item: enshwick::ItemId(0x2a),
///     count: 5,
///     position: [100 << 32, 40 << 32, 0],
///     level: 0,
///     rarity: 0,
/// };
/// assert_eq!(logs.count, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct DropRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The item.
    pub item: ItemId,
    /// How many, 1 to 65535: one stack lies as one pickup.
    pub count: u32,
    /// Where, in 32.32 fixed-point metres.
    pub position: [i64; 3],
    /// The item's level, as the game levels weapons and armour; 0 for none.
    pub level: u32,
    /// The game's `ItemRarity` of the item, 0 for common.
    pub rarity: u32,
}

impl_headed!(DropRequest);

/// The `what` of [`HostApi::progress_write`](super::HostApi::progress_write)
/// that asks for the writes, during `init`. ABI 11.24.
pub const PROGRESS_OPEN: u32 = 0;
/// The `what` of [`HostApi::progress_write`](super::HostApi::progress_write)
/// that sets the level. ABI 11.24.
pub const PROGRESS_SET_LEVEL: u32 = 1;
/// The `what` of [`HostApi::progress_write`](super::HostApi::progress_write)
/// that sets the experience inside the level. ABI 11.24.
pub const PROGRESS_SET_EXPERIENCE: u32 = 2;
/// The `what` of [`HostApi::progress_write`](super::HostApi::progress_write)
/// that adds experience for the game to apply. ABI 11.24.
pub const PROGRESS_ADD_EXPERIENCE: u32 = 3;

/// What became of a level or experience write a mod asked for with
/// [`HostApi::progress_write`](super::HostApi::progress_write): the data of
/// the [`ProgressWritten`](../events/struct.ProgressWritten.html) event. The
/// name is enshwick's. ABI 11.25.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_abi as enshwick;
/// use enshwick::abi::{Header, PROGRESS_SET_LEVEL, ProgressWriteData};
///
/// let written = ProgressWriteData {
///     header: Header::of::<ProgressWriteData>(),
///     entity: enshwick::Entity(2),
///     what: PROGRESS_SET_LEVEL,
///     value: 12,
///     outcome: ProgressWriteData::APPLIED,
///     level: 12,
///     experience: 0,
///     gained: 0,
/// };
/// assert_eq!(written.outcome, ProgressWriteData::APPLIED);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, crate::Script)]
pub struct ProgressWriteData {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character the write was for.
    pub entity: Entity,
    /// The `what` the mod passed: 1 to set the level
    /// ([`PROGRESS_SET_LEVEL`]), 2 to set the experience
    /// ([`PROGRESS_SET_EXPERIENCE`]) or 3 to add experience
    /// ([`PROGRESS_ADD_EXPERIENCE`]).
    pub what: u32,
    /// The `value` the mod passed.
    pub value: u32,
    /// 0 when the write was applied ([`APPLIED`](ProgressWriteData::APPLIED)),
    /// else why not: 1 no level step took the character
    /// ([`NO_LEVEL_STEP`](ProgressWriteData::NO_LEVEL_STEP)), 2 its
    /// experience did not read ([`UNREADABLE`](ProgressWriteData::UNREADABLE)),
    /// 3 its level did not take the write
    /// ([`LEVEL_REFUSED`](ProgressWriteData::LEVEL_REFUSED)), 4 an earlier
    /// write for it was not applied
    /// ([`BEHIND_DROPPED`](ProgressWriteData::BEHIND_DROPPED)), 5 written but
    /// not confirmed ([`UNCONFIRMED`](ProgressWriteData::UNCONFIRMED)).
    pub outcome: u32,
    /// The character's level as the applied write left it, read back after
    /// the write; 0 unless the outcome is 0.
    pub level: u32,
    /// The experience inside the level as the applied write left it, read
    /// back after the write; 0 unless the outcome is 0.
    pub experience: u32,
    /// The experience gained and not applied yet as the write left it, which
    /// the game applies in the same step, levelling up from it; 0 unless the
    /// outcome is 0.
    pub gained: u32,
}

impl_headed!(ProgressWriteData);

impl ProgressWriteData {
    /// The server's level step applied the write.
    pub const APPLIED: u32 = 0;
    /// No level step took the character by the second step end after the
    /// write: it left the world, or the step did not run for it.
    pub const NO_LEVEL_STEP: u32 = 1;
    /// The character's experience did not read.
    pub const UNREADABLE: u32 = 2;
    /// The character's level did not take the new level.
    pub const LEVEL_REFUSED: u32 = 3;
    /// An earlier write for the same character could not be applied, and a
    /// character's writes apply in the order given.
    pub const BEHIND_DROPPED: u32 = 4;
    /// The server wrote the character's level or experience, but a part of
    /// the write did not take or what it left did not read back; the mod
    /// reads the character again to know.
    pub const UNCONFIRMED: u32 = 5;
}
