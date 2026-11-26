//! The data of the quest events, and the filter on what a quest pays.

use core::ffi::c_void;

use crate::abi::{Entity, Header, KnowledgeId};

/// The data of the [`QuestCompleted`](../events/struct.QuestCompleted.html)
/// and [`QuestStarted`](../events/struct.QuestStarted.html) events.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, QuestData};
///
/// let done = enshwick::abi::headed!(QuestData { entity: enshwick::Entity::from_engine(2), quest: enshwick::knowledge::KnowledgeId::new(0x51) });
/// assert_eq!(done.entity, enshwick::Entity::from_engine(2));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, crate::Script)]
#[non_exhaustive]
pub struct QuestData {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character; 0 for a quest of the world's that started.
    pub entity: Entity,
    /// The quest's id, a player knowledge id.
    pub quest: KnowledgeId,
}

impl_headed!(QuestData);

/// The data of the [`QuestStepDone`](../events/struct.QuestStepDone.html) event.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, QuestStepData};
///
/// let done = enshwick::abi::headed!(QuestStepData {
///     entity: enshwick::Entity::from_engine(2),
///     quest: enshwick::knowledge::KnowledgeId::new(0x51),
///     step: 1,
/// });
/// assert_eq!(done.step, 1, "the quest's second step");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, crate::Script)]
#[non_exhaustive]
pub struct QuestStepData {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character, or 0 for the world's knowledge.
    pub entity: Entity,
    /// The quest's id.
    pub quest: KnowledgeId,
    /// The step's place among the quest's steps, from 0, as the journal
    /// lists them.
    pub step: u32,
}

impl_headed!(QuestStepData);

/// The data of the [`QuestProgressed`](../events/struct.QuestProgressed.html) event.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, QuestProgressData};
///
/// let counted = enshwick::abi::headed!(QuestProgressData {
///     entity: enshwick::Entity::from_engine(2),
///     quest: enshwick::knowledge::KnowledgeId::new(0x51),
///     step: 0,
///     part: 0,
///     value: 3,
///     target: 5,
/// });
/// assert!(counted.value < counted.target, "three of five");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, crate::Script)]
#[non_exhaustive]
pub struct QuestProgressData {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character, or 0 for the world's knowledge.
    pub entity: Entity,
    /// The quest's id.
    pub quest: KnowledgeId,
    /// The step's place among the quest's steps, from 0.
    pub step: u32,
    /// The part's place among the step's progress parts, from 0.
    pub part: u32,
    /// The counter's value now.
    pub value: u32,
    /// The count the part asks for: it holds at this count, or above it
    /// for a part that asks for at least the count.
    pub target: u32,
}

impl_headed!(QuestProgressData);

/// What the game pays a player for a quest, handed to the filters
/// registered with [`HostApi::quest_reward_filter`](crate::abi::HostApi::quest_reward_filter)
/// as the filters before left it.
///
/// The game pays a quest's experience and sets the quest's bool; the items
/// its journal draws as rewards it never pays, so they are not here.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, QuestPayment};
///
/// let mut paid = enshwick::abi::headed!(QuestPayment {
///     entity: enshwick::Entity::from_engine(2),
///     quest: enshwick::knowledge::KnowledgeId::new(0x51),
///     count: 1,
///     limit: 3,
///     allowed: 1,
///     reserved: [0; 3],
///     experience: [0x77; QuestPayment::MAX],
/// });
/// paid.count = 2;
/// assert!(paid.count <= paid.limit, "the same experience paid twice");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct QuestPayment {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character.
    pub entity: Entity,
    /// The quest's id.
    pub quest: KnowledgeId,
    /// How many of `experience` the game pays, from the first.
    pub count: u32,
    /// The most the game pays on this build: the loader pays the first
    /// `limit` of `experience` and drops the rest with a warning.
    pub limit: u32,
    /// 1 while the game pays it; 0 pays nothing, and the quest still
    /// completes.
    pub allowed: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// The experience values the game pays, each an experience value id of
    /// the game's, as `ExperienceValue` names them; the same id twice pays
    /// it twice.
    pub experience: [u32; 8],
}

impl_headed!(QuestPayment);

impl QuestPayment {
    /// How many experience values the struct holds.
    pub const MAX: usize = 8;
}

/// A mod's quest reward filter, which may change the payment in place.
///
/// It gets the payment and the pointer given at registration, and returns
/// [`FILTER_REFUSE`](crate::abi::FILTER_REFUSE) to pay nothing,
/// [`FILTER_YES`](crate::abi::FILTER_YES) to pay again what a filter before
/// refused, else [`FILTER_PASS`](crate::abi::FILTER_PASS).
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game's quest reward system, the game thread or
/// a task worker. A payment the game puts off, as when it cannot queue the
/// quest's bool, comes again on a later step, and the filters see it again.
///
/// # Examples
///
/// The safe layer wraps it in `Load::on_quest_reward`.
pub type QuestRewardFilterFn =
    unsafe extern "system" fn(call: *mut QuestPayment, user: *mut c_void) -> u8;
