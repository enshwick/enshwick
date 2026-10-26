//! Game rates a mod multiplies.

/// A game rate of [`HostApi::rate`](super::HostApi::rate) and
/// [`HostApi::set_rate`](super::HostApi::set_rate), by the loader's stable
/// number; the safe layer names them with [`crate::rates::Rate`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::RateId;
///
/// assert_ne!(RateId::EXPERIENCE_COMBAT, RateId::WORKSHOP_SPEED);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RateId(pub u32);

impl RateId {
    /// Experience from combat.
    pub const EXPERIENCE_COMBAT: Self = Self(1);
    /// Experience from mining.
    pub const EXPERIENCE_MINING: Self = Self(2);
    /// Experience from exploration and quests.
    pub const EXPERIENCE_EXPLORATION: Self = Self(3);
    /// How fast workshops produce.
    pub const WORKSHOP_SPEED: Self = Self(4);
    /// How many resources a drop stacks.
    pub const RESOURCE_DROPS: Self = Self(5);
}
