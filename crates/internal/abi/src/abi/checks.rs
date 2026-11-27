//! Checks a mod answers for the game: interactions, skills, comfort, experience and altar limits.

use core::ffi::c_void;

use crate::abi::Metres;
use crate::abi::{Entity, Header, ItemId, SkillNode};

/// A filter's answer that lets the game decide, or the filters after it.
pub const FILTER_PASS: u8 = 0;

/// A filter's answer that lets through what the game or the filters before
/// it refused, in a filter that can; elsewhere the same as [`FILTER_PASS`].
pub const FILTER_YES: u8 = 1;

/// A filter's answer that refuses what it is asked about.
pub const FILTER_REFUSE: u8 = 2;

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
/// use enshwick::abi::Interaction;
///
/// let call = enshwick::abi::headed!(Interaction {
///     item: enshwick::ItemId::new(0x5e01),
///     known: 1,
///     position: [1.0, 2.0, 3.0],
///     ..Interaction::new(Interaction::OPEN, enshwick::Entity::from_raw(4096).ok_or("no entity")?, enshwick::Entity::from_raw(5000).ok_or("no entity")?)
/// });
/// assert_eq!(call.kind, Interaction::OPEN);
/// # Ok::<(), &str>(())
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct Interaction {
    /// Size and version of this struct.
    pub header: Header,
    /// What the player does; one of the constants on this type.
    pub kind: u32,
    /// The player's character entity.
    pub character: Entity,
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
    pub position: Metres,
    /// The item of the stack in `from_slot`, when `moving`; 0 for a move
    /// that names no source slot.
    pub moved: ItemId,
    /// How many items the stack in `from_slot` holds, when `moving`.
    pub stack: u32,
    /// The amount the player asked for, which counts only for
    /// [`ACTION_AMOUNT`](Self::ACTION_AMOUNT).
    pub amount: u32,
    /// The container the items leave: the source entity, or the owner of
    /// `from_slot`; 0 when unknown.
    pub from: Entity,
    /// The container the items enter: the target entity, or the owner of
    /// `to_slot`; 0 for none, as when items are destroyed.
    pub to: Entity,
    /// What the player moves, one of the `ACTION_` constants; 0 when
    /// unknown.
    pub action: u8,
    /// How the move travels, a mask of the `TRANSFER_` constants; 0 for a
    /// plain move.
    pub transfer: u8,
    /// 1 when the loader read the move fields that follow from a
    /// [`MOVE`](Self::MOVE).
    pub moving: u8,
    /// Zero.
    pub reserved_move: u8,
    /// The source slot, as a sub-inventory entity in the low 32 bits and
    /// its slot index above them; 0 for a move of a whole inventory.
    pub from_slot: u64,
    /// The target slot, encoded as `from_slot`; 0 when the game picks the
    /// slots.
    pub to_slot: u64,
}

impl_headed!(Interaction);

impl Interaction {
    /// A call of `kind` by `character` on `target`, its other fields zero:
    /// nothing read of the target or a move.
    ///
    /// # Examples
    ///
    /// See [`Interaction`].
    #[must_use]
    pub const fn new(kind: u32, character: Entity, target: Entity) -> Self {
        Self {
            header: Header::of::<Self>(),
            kind,
            character,
            target,
            item: ItemId::new(0),
            known: 0,
            reserved: [0; 3],
            position: [0.0; 3],
            moved: ItemId::new(0),
            stack: 0,
            amount: 0,
            from: Entity::from_engine(0),
            to: Entity::from_engine(0),
            action: 0,
            transfer: 0,
            moving: 0,
            reserved_move: 0,
            from_slot: 0,
            to_slot: 0,
        }
    }

    /// [`action`](Self::action): one item.
    pub const ACTION_ONE: u8 = 1;
    /// [`action`](Self::action): half the stack.
    pub const ACTION_HALF: u8 = 2;
    /// [`action`](Self::action): the whole stack, or every stack of a
    /// whole-inventory move.
    pub const ACTION_ALL: u8 = 3;
    /// [`action`](Self::action): [`amount`](Self::amount) items.
    pub const ACTION_AMOUNT: u8 = 4;
    /// [`action`](Self::action): the stack is destroyed.
    pub const ACTION_DESTROY: u8 = 5;
    /// [`transfer`](Self::transfer): a gift to another player nearby.
    pub const TRANSFER_GIFT: u8 = 1;
    /// [`transfer`](Self::transfer): only the stacks of items the target
    /// already holds move, as Deposit Stacks and Refill Stacks do.
    pub const TRANSFER_REFILL_STACKS: u8 = 2;
    /// [`transfer`](Self::transfer): every stack moves, as a storage
    /// window's Take All and Deposit All do.
    pub const TRANSFER_MOVE_ALL_STACKS: u8 = 4;
    /// [`transfer`](Self::transfer): the stack goes to the action bar.
    pub const TRANSFER_TO_ACTIONBAR: u8 = 8;
    /// [`transfer`](Self::transfer): the stack goes to the backpack.
    pub const TRANSFER_TO_BACKPACK: u8 = 16;

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
/// registration; returns [`FILTER_REFUSE`] to refuse, which the game handles as its own
/// refusal, so nothing moves and nothing is lost.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game system that asks, the game thread or a task
/// worker, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_interaction`](../struct.Load.html#method.on_interaction).
pub type InteractionFilterFn =
    unsafe extern "system" fn(call: *const Interaction, user: *mut c_void) -> u8;

/// A stack about to enter one slot of a container, as
/// [`HostApi::container_rule`](super::HostApi::container_rule) hands it on.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ContainerPut, Header};
///
/// let put = enshwick::abi::headed!(ContainerPut {
///     container: enshwick::Entity::from_raw(5000).ok_or("no entity")?,
///     container_item: enshwick::ItemId::new(0x5e01),
///     item: enshwick::ItemId::new(0x0e17),
///     count: 20,
///     slot: 5001 | 3 << 32,
/// });
/// assert_eq!(put.count, 20);
/// # Ok::<(), &str>(())
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct ContainerPut {
    /// Size and version of this struct.
    pub header: Header,
    /// The container: the placed entity whose inventory holds the slot.
    pub container: Entity,
    /// The item the container was placed from (`UsedItem`), the one the
    /// rule was registered for.
    pub container_item: ItemId,
    /// The item of the stack.
    pub item: ItemId,
    /// How many items enter.
    pub count: u32,
    /// The slot, as a sub-inventory entity in the low 32 bits and its slot
    /// index above them.
    pub slot: u64,
}

impl_headed!(ContainerPut);

/// A mod's container rule: the stack and the pointer given at registration;
/// returns [`FILTER_REFUSE`] to keep the stack out.
///
/// The game handles a refusal as a slot that does not take the stack, so it
/// stays where it was or goes to another slot.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game system that moves the items, the game
/// thread or a task worker, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in `Load::container_rule`, with the `unstable` feature.
pub type ContainerRuleFn =
    unsafe extern "system" fn(put: *const ContainerPut, user: *mut c_void) -> u8;

/// How the prompt of every object placed from one item reads and what its
/// press does, as [`HostApi::interaction_prompt`](super::HostApi::interaction_prompt)
/// takes it.
///
/// The server writes [`verb`](Self::verb) and the replaced action, so every
/// client sees them; a client with Enshwick also shows [`text`](Self::text)
/// in the verb's place.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, InteractionPrompt};
///
/// let prompt = enshwick::abi::headed!(InteractionPrompt {
///     item: enshwick::ItemId::new(0x5e01),
///     verb: 0xc22a_47e8,
///     text: 0,
///     flags: InteractionPrompt::REPLACE_ACTION,
/// });
/// assert_eq!(prompt.flags, InteractionPrompt::REPLACE_ACTION);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct InteractionPrompt {
    /// Size and version of this struct.
    pub header: Header,
    /// The item the objects were placed from (`UsedItem`); never 0.
    pub item: ItemId,
    /// The game's `LocaTagId` of the verb every client shows; 0 keeps the
    /// object's own verb.
    pub verb: u32,
    /// The `LocaTagId` of one of the mod's own texts, shown in the verb's
    /// place on clients with Enshwick; 0 for none.
    pub text: u32,
    /// [`REPLACE_ACTION`](Self::REPLACE_ACTION) or 0.
    pub flags: u32,
}

impl_headed!(InteractionPrompt);

impl InteractionPrompt {
    /// The press runs the game's empty interaction instead of the object's
    /// own action, such as opening a chest, so only the mod's action runs.
    pub const REPLACE_ACTION: u32 = 1;

    /// Whether a host takes the prompt: an item, no unknown flag, and a
    /// verb, a text or the replaced action.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, InteractionPrompt};
    ///
    /// let mut prompt = enshwick::abi::headed!(InteractionPrompt { item: enshwick::ItemId::new(0x5e01), verb: 0, text: 0, flags: 0 });
    /// assert!(!prompt.is_valid(), "it changes nothing");
    /// prompt.flags = InteractionPrompt::REPLACE_ACTION;
    /// assert!(prompt.is_valid());
    /// ```
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        let known = self.flags & !Self::REPLACE_ACTION == 0;
        let changes = self.flags != 0 || self.verb != 0 || self.text != 0;
        self.item.get() != 0 && known && changes
    }
}

/// The game asking whether a character unlocked a skill node, as
/// [`HostApi::skill_override`](super::HostApi::skill_override) hands it on.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, SkillCheck};
///
/// let check = enshwick::abi::headed!(SkillCheck { node: enshwick::abi::SkillNode::new(0x4475_e8dd), gate: SkillCheck::FLY_MODE, unlocked: 0, reserved: [0; 3] });
/// assert_eq!(check.gate, SkillCheck::FLY_MODE);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct SkillCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The skill node.
    pub node: SkillNode,
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
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game system that asks, the game thread or a task
/// worker, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_skill`](../struct.Load.html#method.on_skill).
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
/// let comfort = enshwick::abi::headed!(Comfort { category: 3, value: 5, position: [0.0; 3] });
/// assert_eq!(comfort.value, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct Comfort {
    /// Size and version of this struct.
    pub header: Header,
    /// The comfort category; a base keeps the best provider per category.
    pub category: u32,
    /// The comfort it registers.
    pub value: i32,
    /// Where the provider stands, in metres.
    pub position: Metres,
}

impl_headed!(Comfort);

/// A comfort filter: the registration and the pointer given at
/// registration; returns the comfort to register.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game system that asks, the game thread or a task
/// worker, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_comfort`](../struct.Load.html#method.on_comfort).
pub type ComfortFilterFn =
    unsafe extern "system" fn(comfort: *const Comfort, user: *mut c_void) -> i32;

/// One character's experience before the game applies it, in an
/// [`ExperiencePass`]. Frozen, passed as an array: what concerns the whole
/// pass grows in the headed [`ExperiencePass`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::ExperienceRow;
///
/// let row = ExperienceRow { entity: enshwick::Entity::from_raw(4096).ok_or("no entity")?, level: 3, progress: 10, need: 100, gained: 5 };
/// assert_eq!(row.gained, 5);
/// # Ok::<(), &str>(())
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "an ABI row without a header: a field of its own is a new ABI major"
)]
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

// Entity 0, the ABI's "none", for a row the host fills.
impl Default for ExperienceRow {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

/// Every spawned character's experience before the server applies it, as
/// [`HostApi::experience_filter`](super::HostApi::experience_filter) hands it on.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ExperiencePass, Header};
///
/// let pass = enshwick::abi::headed!(ExperiencePass { rows: core::ptr::null_mut(), row_count: 0, level_cap: 25, need_for: None, token: core::ptr::null_mut() });
/// assert_eq!(pass.level_cap, 25);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct ExperiencePass {
    /// Size and version of this struct.
    pub header: Header,
    /// The characters, valid and writable for the call.
    pub rows: *mut ExperienceRow,
    /// How many rows there are.
    pub row_count: u32,
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
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game system that asks, the game thread or a task
/// worker, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_experience`](../struct.Load.html#method.on_experience).
pub type ExperienceFilterFn =
    unsafe extern "system" fn(pass: *mut ExperiencePass, user: *mut c_void);

/// How many altars a Flame level allows, as the loader asks every
/// [`HostApi::altar_limit_filter`](super::HostApi::altar_limit_filter) in
/// load order.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{AltarLimit, Header};
///
/// let limit = enshwick::abi::headed!(AltarLimit { flame_level: 3, vanilla: 6, allowed: 6, reserved: 0 });
/// assert_eq!(limit.allowed, limit.vanilla);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct AltarLimit {
    /// Size and version of this struct.
    pub header: Header,
    /// The Flame level.
    pub flame_level: u32,
    /// The altars the game itself allows at that level.
    pub vanilla: u32,
    /// The altars allowed so far, as the filters before have left it; the
    /// filter's answer.
    pub allowed: u32,
    /// Zero.
    pub reserved: u32,
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
/// let limit = enshwick::abi::headed!(BaseLimitInfo {
///     zones: 16,
///     bases: 16,
///     vanilla_zones: 16,
///     reserved: 0,
///     vanilla_bases: 16,
///     max_zones: 255,
///     max_bases: 16,
/// });
/// assert_eq!(limit.zones, limit.vanilla_zones);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
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
    /// Bases the game's own code holds.
    pub vanilla_bases: u32,
    /// The most altar zones a mod may ask for with
    /// [`HostApi::request_base_limits`](super::HostApi::request_base_limits).
    pub max_zones: u32,
    /// The most bases a mod may ask for.
    pub max_bases: u32,
}

impl_headed!(BaseLimitInfo);

/// An altar limit filter: the limit so far, writable, and the pointer given
/// at registration. It must not unwind.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game system that asks, the game thread or a task
/// worker, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_altar_limit`](../struct.Load.html#method.on_altar_limit).
pub type AltarLimitFilterFn = unsafe extern "system" fn(limit: *mut AltarLimit, user: *mut c_void);
