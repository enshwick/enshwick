//! Checks a mod answers for the game: interactions, skills, comfort, experience and altar limits.

use core::ffi::c_void;

use crate::abi::{Entity, Header, ItemId};

/// A player about to interact with an entity, as
/// [`HostApi::interaction_filter`](super::HostApi::interaction_filter) hands it
/// on.
///
/// Open it, move items in or out of it, sort it, craft or build from it,
/// dismantle it, or flip its Quick Stash or upgrade switch.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Interaction};
///
/// let call = Interaction {
///     header: Header::of::<Interaction>(),
///     kind: Interaction::OPEN,
///     player: enshwick::Entity(4096),
///     target: enshwick::Entity(5000),
///     item: enshwick::ItemId::new(0x5e01),
///     known: 1,
///     reserved: [0; 3],
///     position: [1.0, 2.0, 3.0],
/// };
/// assert_eq!(call.kind, Interaction::OPEN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct Interaction {
    /// Size and version of this struct.
    pub header: Header,
    /// What the player does; one of the constants on this type.
    pub kind: u32,
    /// The player's character entity.
    pub player: Entity,
    /// The entity they act on: a chest, a placed piece, the inventory a slot
    /// belongs to.
    pub target: Entity,
    /// The item the target was placed from (`UsedItem`), 0 when unknown.
    pub item: ItemId,
    /// 1 when `item` and `position` were read from the target.
    pub known: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// Where the target stands, in metres, when `known`.
    pub position: [f64; 3],
}

impl_headed!(Interaction);

impl Interaction {
    /// The game offers to open the target.
    pub const OPEN: u32 = 1;
    /// Items move out of or into the target.
    pub const MOVE: u32 = 2;
    /// The target's inventory is sorted.
    pub const SORT: u32 = 3;
    /// The target lends its stock to crafting or building nearby; asked
    /// without the target's item or position.
    pub const STOCK: u32 = 4;
    /// The cursor offers to pick up or tear down the target.
    pub const DISMANTLE: u32 = 5;
    /// The player flips the target's Quick Stash switch.
    pub const QUICK_STASH: u32 = 6;
    /// The player upgrades the target into another piece.
    pub const UPGRADE: u32 = 7;
}

/// A mod's interaction filter: the call and the pointer given at
/// registration; returns 0 to refuse, which the game handles as its own
/// refusal, so nothing moves and nothing is lost.
pub type InteractionFn =
    unsafe extern "system" fn(call: *const Interaction, user: *mut c_void) -> u8;

/// The game asking whether a character unlocked a skill node, as
/// [`HostApi::skill_override`](super::HostApi::skill_override) hands it on.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, SkillCheck};
///
/// let check = SkillCheck { header: Header::of::<SkillCheck>(), node: 0x4475_e8dd, gate: SkillCheck::FLY_MODE, unlocked: 0, reserved: [0; 3] };
/// assert_eq!(check.gate, SkillCheck::FLY_MODE);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct SkillCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The skill node.
    pub node: u32,
    /// The gate that asks, one of the constants on this type.
    pub gate: u32,
    /// 1 when the character unlocked it.
    pub unlocked: u8,
    /// Zero.
    pub reserved: [u8; 3],
}

impl_headed!(SkillCheck);

impl SkillCheck {
    /// A caller the platform does not name.
    pub const OTHER: u32 = 0;
    /// The building fly mode gate, on both sides.
    pub const FLY_MODE: u32 = 1;
    /// A [`SkillCheckFn`] answer: the game's own.
    pub const VANILLA: u8 = 0;
    /// A [`SkillCheckFn`] answer: the skill counts as unlocked.
    pub const UNLOCKED: u8 = 1;
    /// A [`SkillCheckFn`] answer: the skill counts as locked.
    pub const LOCKED: u8 = 2;
}

/// A skill override: the check and the pointer given at registration;
/// returns [`SkillCheck::VANILLA`], [`SkillCheck::UNLOCKED`] or
/// [`SkillCheck::LOCKED`].
pub type SkillCheckFn =
    unsafe extern "system" fn(check: *const SkillCheck, user: *mut c_void) -> u8;

/// A comfort provider in a base registering its comfort for its category,
/// as [`HostApi::comfort_filter`](super::HostApi::comfort_filter) hands it on.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Comfort, Header};
///
/// let comfort = Comfort { header: Header::of::<Comfort>(), category: 3, value: 5, position: [0.0; 3] };
/// assert_eq!(comfort.value, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct Comfort {
    /// Size and version of this struct.
    pub header: Header,
    /// The comfort category; a base keeps the best provider per category.
    pub category: u32,
    /// The comfort it registers.
    pub value: i32,
    /// Where the provider stands, in metres.
    pub position: [f32; 3],
}

impl_headed!(Comfort);

/// A comfort filter: the registration and the pointer given at
/// registration; returns the comfort to register.
pub type ComfortFn = unsafe extern "system" fn(comfort: *const Comfort, user: *mut c_void) -> i32;

/// One character's experience before the game applies it, in an
/// [`ExperiencePass`]. Its layout is frozen: it is passed as an array.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ExperienceRow;
///
/// let row = ExperienceRow { entity: enshwick::Entity(4096), level: 3, progress: 10, need: 100, gained: 5 };
/// assert_eq!(row.gained, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExperienceRow {
    /// The character's entity.
    pub entity: Entity,
    /// Its level.
    pub level: u32,
    /// Experience inside the level.
    pub progress: u32,
    /// What the level needs.
    pub need: u32,
    /// Gained experience the game applies next, at most one level a tick;
    /// a filter may change it.
    pub gained: u32,
}

/// Every spawned character's experience before the server applies it, as
/// [`HostApi::experience_filter`](super::HostApi::experience_filter) hands it on.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ExperiencePass, Header};
///
/// let pass = ExperiencePass { header: Header::of::<ExperiencePass>(), rows: core::ptr::null_mut(), row_count: 0, level_cap: 25, need_for: None, token: core::ptr::null_mut() };
/// assert_eq!(pass.level_cap, 25);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ExperiencePass {
    /// Size and version of this struct.
    pub header: Header,
    /// The characters, valid and writable for the call.
    pub rows: *mut ExperienceRow,
    /// How many rows there are.
    pub row_count: usize,
    /// The highest level.
    pub level_cap: u32,
    /// What level `level` (1 or more) needs; valid during the call with
    /// `token`.
    pub need_for: Option<unsafe extern "system" fn(token: *mut c_void, level: u32) -> u32>,
    /// The argument of `need_for`.
    pub token: *mut c_void,
}

impl_headed!(ExperiencePass);

/// An experience filter: the pass and the pointer given at registration.
pub type ExperiencePassFn = unsafe extern "system" fn(pass: *mut ExperiencePass, user: *mut c_void);

/// How many altars a Flame level allows, as the loader asks every
/// [`HostApi::altar_limit_filter`](super::HostApi::altar_limit_filter) in
/// load order.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{AltarLimit, Header};
///
/// let limit = AltarLimit { header: Header::of::<AltarLimit>(), flame_level: 3, vanilla: 6, allowed: 6, reserved: [0; 2] };
/// assert_eq!(limit.allowed, limit.vanilla);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct AltarLimit {
    /// Size and version of this struct.
    pub header: Header,
    /// The Flame level.
    pub flame_level: u32,
    /// The altars the game itself allows at that level.
    pub vanilla: u8,
    /// The altars allowed so far, as the filters before have left it; the
    /// filter's answer.
    pub allowed: u8,
    /// Zero.
    pub reserved: [u8; 2],
}

impl_headed!(AltarLimit);

/// The most altar zones and bases the game groups, as
/// [`HostApi::base_limit`](super::HostApi::base_limit) writes it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BaseLimitInfo, Header};
///
/// let limit = BaseLimitInfo { header: Header::of::<BaseLimitInfo>(), zones: 16, bases: 16, vanilla_zones: 16, reserved: 0 };
/// assert_eq!(limit.zones, limit.vanilla_zones);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct BaseLimitInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// Altar zones the game puts into bases now.
    pub zones: u32,
    /// Bases the game holds.
    pub bases: u32,
    /// Altar zones the game's own grouping takes, without the raised limit.
    pub vanilla_zones: u32,
    /// Zero.
    pub reserved: u32,
}

impl_headed!(BaseLimitInfo);

/// An altar limit filter: the limit so far, writable, and the pointer given
/// at registration. It must not unwind.
pub type AltarLimitFilterFn = unsafe extern "system" fn(limit: *mut AltarLimit, user: *mut c_void);
