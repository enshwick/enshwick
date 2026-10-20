//! Inventories on the server: read what a chest, corpse or player holds, and
//! change it through the game's own transactions.
//!
//! Reads are synchronous plain memory reads. Make them where no inventory
//! changes underneath: on the server's game thread between steps (a game
//! event or a [`Scheduler`] task) with [`Inventories::read`], or inside a
//! hooked ECS system with [`Inventories::read_in`]. Anywhere else they race
//! with the game's writes, and the game-thread reader returns nothing.
//!
//! Changes are requests. Submit them from any thread; the loader runs each in
//! the game's inventory system as one transaction, all or nothing, and hands
//! the [`Outcome`] to a callback on the game thread after a later step. The
//! [`Ticket`] a request returns holds the same outcome for polling.
//!
//! Items are not entities: an inventory links up to 16 sub-inventories of 8
//! slots each, and only items with durability or a level have an entity of
//! their own, the item's PIDE.

use core::ffi::c_void;
use std::sync::{Arc, Mutex, PoisonError};

use crate::abi::{
    Header, HostApi, InventoryOp, InventoryOpResult, InventoryRequest, InventoryResult, ItemData,
};
use crate::ecs::{Components, Entity};
use crate::raw::read_at;
use crate::schedule::Scheduler;
use crate::world::{ComponentType, World};
use crate::{Host, Load, Off};

/// The most operations in one [`Batch`].
pub const MAX_OPS: usize = 64;
/// The most inventories one [`Batch`] may name; the game takes 16 touched
/// inventories per transaction.
pub const MAX_INVENTORIES: usize = 16;
/// The game's op log holds this many records per transaction.
const OP_LOG: u32 = 0x200;
/// Slots per sub-inventory.
const SLOTS: u8 = 8;
/// Links per `InventorySetup`.
const LINKS: usize = 16;
/// How deep virtual sub-inventories nest before the walk stops.
const MAX_DEPTH: u32 = 4;
/// How far the owner walk climbs.
const MAX_OWNERS: usize = 8;

/// A slot: a sub-inventory entity and a slot index in it, as the game packs
/// it, `entity | index << 32`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::ecs::Entity;
/// use enshwick_loader_api::inventory::SlotRef;
///
/// let slot = SlotRef::new(Entity(0x41), 3);
/// assert_eq!(slot.0, 0x3_0000_0041);
/// assert_eq!((slot.inventory(), slot.index()), (Entity(0x41), 3));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlotRef(pub u64);

impl SlotRef {
    /// Slot `index` of the sub-inventory `inventory`.
    ///
    /// # Examples
    ///
    /// See [`SlotRef`].
    #[must_use]
    pub const fn new(inventory: Entity, index: u8) -> Self {
        Self(inventory.0 as u64 | (index as u64) << 32)
    }

    /// The sub-inventory entity.
    ///
    /// # Examples
    ///
    /// See [`SlotRef`].
    #[must_use]
    #[expect(clippy::cast_possible_truncation, reason = "the low half")]
    pub const fn inventory(self) -> Entity {
        Entity(self.0 as u32)
    }

    /// The slot index, 0 to 7.
    ///
    /// # Examples
    ///
    /// See [`SlotRef`].
    #[must_use]
    #[expect(clippy::cast_possible_truncation, reason = "a byte")]
    pub const fn index(self) -> u8 {
        (self.0 >> 32) as u8
    }
}

/// What a sub-inventory is for, from the inventory's `InventorySetup`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::inventory::Category;
///
/// assert_ne!(Category::GENERIC, Category::EQUIPMENT);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Category(pub u8);

impl Category {
    /// Not known: a slot read alone.
    pub const UNKNOWN: Self = Self(0);
    /// Looks.
    pub const CUSTOMIZATION: Self = Self(1);
    /// Worn and held items.
    pub const EQUIPMENT: Self = Self(2);
    /// Currency.
    pub const CURRENCY: Self = Self(3);
    /// Ordinary storage: a chest's slots, a player's backpack.
    pub const GENERIC: Self = Self(4);
    /// A link to another inventory, which the reads walk into.
    pub const VIRTUAL: Self = Self(5);
}

/// One slot and what it holds.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::ecs::Entity;
/// use enshwick_loader_api::inventory::{Category, Slot, SlotRef};
///
/// let slot = Slot { at: SlotRef::new(Entity(9), 0), category: Category::GENERIC, item: 0, count: 0, pide: Entity::NONE };
/// assert!(slot.is_empty());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slot {
    /// Where the slot is.
    pub at: SlotRef,
    /// The sub-inventory's category.
    pub category: Category,
    /// The item id, 0 for none.
    pub item: u32,
    /// How many.
    pub count: u32,
    /// The item's own entity, for items with durability or a level.
    pub pide: Entity,
}

impl Slot {
    /// Whether the slot holds nothing.
    ///
    /// # Examples
    ///
    /// See [`Slot`].
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.item == 0 || self.count == 0
    }
}

/// An item's static data.
///
/// # Examples
///
/// ```
/// fn stackable(inventories: &enshwick_loader_api::inventory::Inventories, item: u32) -> bool {
///     inventories.item_info(item).is_some_and(|info| info.max_stack > 1 && !info.has_pide())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemInfo {
    /// The item id.
    pub id: u32,
    /// How many fit one stack.
    pub max_stack: u16,
    /// `ItemInfo` flags, see [`ItemData`].
    pub flags: u32,
}

impl ItemInfo {
    /// Whether each item has an entity of its own (durability, level).
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    pub const fn has_pide(&self) -> bool {
        self.flags & ItemData::HAS_PIDE != 0
    }

    /// Whether the item is a container.
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    pub const fn is_container(&self) -> bool {
        self.flags & ItemData::IS_CONTAINER != 0
    }

    /// Whether the item is a blueprint.
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    pub const fn is_blueprint(&self) -> bool {
        self.flags & ItemData::IS_BLUEPRINT != 0
    }
}

/// An item's durability, as clients see it.
///
/// # Examples
///
/// ```
/// let worn = enshwick_loader_api::inventory::Durability { current: 40, max: 160 };
/// assert!(worn.current < worn.max);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Durability {
    /// What is left.
    pub current: u16,
    /// When new.
    pub max: u16,
}

/// One change in a [`Batch`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::ecs::Entity;
/// use enshwick_loader_api::inventory::Op;
///
/// let loot = Op::MoveAll { from: Entity(40), to: Entity(2), include_special: false };
/// let _ = loot;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    /// New items: merged into stacks of the same item, then free slots.
    Give {
        /// The inventory.
        to: Entity,
        /// The item id.
        item: u32,
        /// How many.
        count: u32,
    },
    /// Removes items, from whichever stacks hold them.
    Take {
        /// The inventory.
        from: Entity,
        /// The item id.
        item: u32,
        /// How many.
        count: u32,
    },
    /// Destroys a stack.
    DestroySlot(SlotRef),
    /// Moves a stack, or part of it, into an inventory's free or matching
    /// slots. What does not fit stays in the source.
    MoveSlot {
        /// The stack.
        slot: SlotRef,
        /// The inventory.
        to: Entity,
        /// How many; `None` for the whole stack.
        amount: Option<u16>,
    },
    /// Moves every stack of one inventory into another, as "take all" does.
    /// What does not fit stays in the source.
    MoveAll {
        /// The source inventory.
        from: Entity,
        /// The destination inventory.
        to: Entity,
        /// Also the source's action bar, equipment and locked slots.
        include_special: bool,
    },
}

impl Op {
    fn raw(&self) -> InventoryOp {
        let entity = |e: Entity| u64::from(e.0);
        match *self {
            Self::Give { to, item, count } => InventoryOp {
                kind: InventoryOp::GIVE,
                target: entity(to),
                item,
                count,
                ..InventoryOp::default()
            },
            Self::Take { from, item, count } => InventoryOp {
                kind: InventoryOp::TAKE,
                target: entity(from),
                item,
                count,
                ..InventoryOp::default()
            },
            Self::DestroySlot(slot) => InventoryOp {
                kind: InventoryOp::DESTROY_SLOT,
                target: slot.0,
                ..InventoryOp::default()
            },
            Self::MoveSlot { slot, to, amount } => InventoryOp {
                kind: InventoryOp::MOVE_SLOT,
                target: slot.0,
                other: entity(to),
                count: amount.map_or(0, u32::from),
                ..InventoryOp::default()
            },
            Self::MoveAll {
                from,
                to,
                include_special,
            } => InventoryOp {
                kind: InventoryOp::MOVE_ALL,
                flags: if include_special {
                    InventoryOp::INCLUDE_SPECIAL
                } else {
                    0
                },
                target: entity(from),
                other: entity(to),
                ..InventoryOp::default()
            },
        }
    }

    /// Op log records the operation may write, at most.
    const fn cost(&self) -> u32 {
        match self {
            Self::Give { .. } | Self::Take { .. } => 16,
            Self::DestroySlot(_) => 1,
            Self::MoveSlot { .. } => 4,
            Self::MoveAll { .. } => 256,
        }
    }

    /// The inventories the operation names.
    const fn inventories(&self) -> [Entity; 2] {
        match *self {
            Self::Give { to, .. } => [to, Entity::NONE],
            Self::Take { from, .. } => [from, Entity::NONE],
            Self::DestroySlot(slot) => [slot.inventory(), Entity::NONE],
            Self::MoveSlot { slot, to, .. } => [slot.inventory(), to],
            Self::MoveAll { from, to, .. } => [from, to],
        }
    }

    /// Whether the operation names what it needs.
    fn is_valid(&self) -> bool {
        match *self {
            Self::Give { to: e, item, count }
            | Self::Take {
                from: e,
                item,
                count,
            } => !e.is_none() && item != 0 && count != 0 && i32::try_from(count).is_ok(),
            Self::DestroySlot(slot) => !slot.inventory().is_none(),
            Self::MoveSlot { slot, to, amount } => {
                !slot.inventory().is_none() && !to.is_none() && amount != Some(0)
            }
            Self::MoveAll { from, to, .. } => !from.is_none() && !to.is_none() && from != to,
        }
    }
}

/// Why a [`Batch`] cannot run as one transaction.
///
/// # Examples
///
/// ```
/// let empty = enshwick_loader_api::inventory::Batch::new();
/// assert_eq!(empty.check(), Err(enshwick_loader_api::inventory::BatchError::Empty));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum BatchError {
    /// No operations.
    #[error("the batch is empty")]
    Empty,
    /// An operation names no inventory or item, or a count of 0.
    #[error("an operation is malformed")]
    Malformed,
    /// More than [`MAX_OPS`] operations.
    #[error("more than {MAX_OPS} operations")]
    TooManyOps,
    /// More than [`MAX_INVENTORIES`] inventories.
    #[error("more than {MAX_INVENTORIES} inventories")]
    TooManyInventories,
    /// The operations could overflow the game's op log.
    #[error("too much work for one transaction")]
    TooLarge,
}

/// Operations to run as one transaction: every one succeeds, or nothing
/// changes.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::ecs::Entity;
/// use enshwick_loader_api::inventory::{Batch, Op};
///
/// // A trade: 5 coins out, a sword in, or neither.
/// let player = Entity(2);
/// let trade = Batch::new()
///     .with(Op::Take { from: player, item: 0x0c01, count: 5 })
///     .with(Op::Give { to: player, item: 0x5e01, count: 1 });
/// assert_eq!(trade.check(), Ok(()));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    ops: Vec<Op>,
    allow_partial: bool,
    notify: bool,
    actor: Entity,
}

impl Batch {
    /// An empty batch.
    ///
    /// # Examples
    ///
    /// See [`Batch`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The batch with `op` appended.
    ///
    /// # Examples
    ///
    /// See [`Batch`].
    #[must_use]
    pub fn with(mut self, op: Op) -> Self {
        self.ops.push(op);
        self
    }

    /// Appends `op`.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut batch = enshwick_loader_api::inventory::Batch::new();
    /// batch.push(enshwick_loader_api::inventory::Op::DestroySlot(enshwick_loader_api::inventory::SlotRef(0x41)));
    /// assert_eq!(batch.ops().len(), 1);
    /// ```
    pub fn push(&mut self, op: Op) {
        self.ops.push(op);
    }

    /// Commits even when a give or take leaves some of its count undone;
    /// otherwise that rolls the batch back.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    /// use enshwick_loader_api::inventory::{Batch, Op};
    ///
    /// // As many as fit.
    /// let _ = Batch::new().with(Op::Give { to: Entity(2), item: 7, count: 500 }).allow_partial();
    /// ```
    #[must_use]
    pub const fn allow_partial(mut self) -> Self {
        self.allow_partial = true;
        self
    }

    /// Shows the actor the game's pickup message for each stack it gets and
    /// its inventory-full message for what does not fit, as the game's own
    /// looting does. Batches with a give always do; moves are silent without
    /// this.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    /// use enshwick_loader_api::inventory::{Batch, Op};
    ///
    /// let (corpse, player) = (Entity(40), Entity(2));
    /// let _ = Batch::new()
    ///     .with(Op::MoveAll { from: corpse, to: player, include_special: false })
    ///     .actor(player)
    ///     .notify();
    /// ```
    #[must_use]
    pub const fn notify(mut self) -> Self {
        self.notify = true;
        self
    }

    /// The player the game credits with the change, for its pickup and
    /// inventory-full messages. By default the destination of the first
    /// give or move.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    /// use enshwick_loader_api::inventory::{Batch, Op};
    ///
    /// let _ = Batch::new().with(Op::MoveAll { from: Entity(40), to: Entity(41), include_special: false }).actor(Entity(2));
    /// ```
    #[must_use]
    pub const fn actor(mut self, player: Entity) -> Self {
        self.actor = player;
        self
    }

    /// The operations, in order.
    ///
    /// # Examples
    ///
    /// See [`Batch::push`].
    #[must_use]
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// Whether the batch fits one transaction.
    ///
    /// # Errors
    ///
    /// The first limit the batch breaks.
    ///
    /// # Examples
    ///
    /// See [`Batch`].
    pub fn check(&self) -> Result<(), BatchError> {
        if self.ops.is_empty() {
            return Err(BatchError::Empty);
        }
        if !self.ops.iter().all(Op::is_valid) {
            return Err(BatchError::Malformed);
        }
        if self.ops.len() > MAX_OPS {
            return Err(BatchError::TooManyOps);
        }
        if inventories(&self.ops) > MAX_INVENTORIES {
            return Err(BatchError::TooManyInventories);
        }
        if self.ops.iter().map(Op::cost).sum::<u32>() > OP_LOG {
            return Err(BatchError::TooLarge);
        }
        Ok(())
    }

    /// Splits the batch into batches that each fit one transaction, in
    /// order. Each part is all or nothing on its own; together they are not.
    /// Malformed operations are kept, for [`Batch::check`] to name.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    /// use enshwick_loader_api::inventory::{Batch, Op};
    ///
    /// let mut everything = Batch::new();
    /// for chest in 100..120 {
    ///     everything.push(Op::MoveAll { from: Entity(chest), to: Entity(2), include_special: false });
    /// }
    /// let parts = everything.split();
    /// assert!(parts.len() > 1);
    /// assert!(parts.iter().all(|part| part.check().is_ok()));
    /// ```
    #[must_use]
    pub fn split(self) -> Vec<Self> {
        let mut parts: Vec<Self> = Vec::new();
        let mut part = Self {
            ops: Vec::new(),
            ..self.clone()
        };
        for op in self.ops {
            part.ops.push(op);
            if part.ops.len() > 1 && part.check().is_err_and(|e| e != BatchError::Malformed) {
                part.ops.pop();
                let full = std::mem::replace(&mut part.ops, vec![op]);
                parts.push(Self {
                    ops: full,
                    ..part.clone()
                });
            }
        }
        if !part.ops.is_empty() {
            parts.push(part);
        }
        parts
    }

    fn request_flags(&self) -> u32 {
        let mut flags = 0;
        if self.allow_partial {
            flags |= InventoryRequest::ALLOW_PARTIAL;
        }
        if self.notify || self.ops.iter().any(|op| matches!(op, Op::Give { .. })) {
            flags |= InventoryRequest::NOTIFY;
        }
        flags
    }

    fn actor_entity(&self) -> Entity {
        if !self.actor.is_none() {
            return self.actor;
        }
        self.ops
            .iter()
            .find_map(|op| match *op {
                Op::Give { to, .. } | Op::MoveSlot { to, .. } | Op::MoveAll { to, .. } => Some(to),
                _ => None,
            })
            .unwrap_or(Entity::NONE)
    }
}

/// How many distinct inventories `ops` name.
fn inventories(ops: &[Op]) -> usize {
    let mut seen: Vec<Entity> = ops
        .iter()
        .flat_map(Op::inventories)
        .filter(|e| !e.is_none())
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

/// How a request ended.
///
/// # Examples
///
/// See [`Outcome`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Done {
    /// Every operation succeeded and the transaction was committed.
    Committed,
    /// An operation failed or fell short; nothing changed.
    RolledBack,
    /// This game build lacks a piece, or the loader's hook is not in place;
    /// nothing changed.
    Unavailable,
    /// The game did not run its inventory system in time, as with nobody
    /// online; nothing changed.
    TimedOut,
    /// The batch was refused before it reached the game.
    Refused(BatchError),
}

/// The result of one operation.
///
/// # Examples
///
/// See [`Outcome`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct OpResult {
    /// The game's error code, 0 for none; [`InventoryOpResult::NOT_RUN`]
    /// when an operation before it failed.
    pub error: u32,
    /// How much of the count the operation could not do.
    pub remaining: u32,
}

impl OpResult {
    /// The game's code for a full op log or more than 16 touched
    /// inventories; split the batch.
    pub const TOO_LARGE: u32 = 0x27;
    /// The game's code for an item that did not fit or does not exist.
    pub const NO_ROOM: u32 = 0x15;

    /// Whether the operation ran without an error.
    ///
    /// # Examples
    ///
    /// See [`Outcome`].
    #[must_use]
    pub const fn ok(self) -> bool {
        self.error == 0
    }
}

/// What became of a request, with a result per operation once it ran.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::inventory::{Done, Outcome};
///
/// fn report(outcome: &Outcome) {
///     match outcome.done {
///         Done::Committed => enshwick_loader_api::info!("done"),
///         Done::RolledBack => {
///             let failed = outcome.ops.iter().position(|op| !op.ok() || op.remaining > 0);
///             enshwick_loader_api::warn!("rolled back at operation {failed:?}");
///         }
///         other => enshwick_loader_api::warn!("not run: {other:?}"),
///     }
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// How it ended.
    pub done: Done,
    /// One per operation when the request ran, else empty.
    pub ops: Vec<OpResult>,
}

impl Outcome {
    /// Whether the change was made.
    ///
    /// # Examples
    ///
    /// See [`Ticket`].
    #[must_use]
    pub const fn is_committed(&self) -> bool {
        matches!(self.done, Done::Committed)
    }

    const fn not_run(done: Done) -> Self {
        Self {
            done,
            ops: Vec::new(),
        }
    }

    fn from_raw(result: &InventoryResult) -> Self {
        let done = match result.outcome {
            InventoryResult::COMMITTED => Done::Committed,
            InventoryResult::ROLLED_BACK => Done::RolledBack,
            InventoryResult::TIMED_OUT => Done::TimedOut,
            _ => Done::Unavailable,
        };
        let ops = if result.ops.is_null() || result.op_count == 0 {
            &[][..]
        } else {
            // SAFETY: the loader passes `op_count` results, valid for the call.
            unsafe { core::slice::from_raw_parts(result.ops, result.op_count as usize) }
        };
        Self {
            done,
            ops: ops
                .iter()
                .map(|r: &InventoryOpResult| OpResult {
                    error: r.error,
                    remaining: r.remaining,
                })
                .collect(),
        }
    }
}

/// A submitted request; holds its [`Outcome`] once it is known. Clone it
/// freely.
///
/// # Examples
///
/// ```
/// fn finished(ticket: &enshwick_loader_api::inventory::Ticket) -> bool {
///     ticket.outcome().is_some_and(|outcome| outcome.is_committed())
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct Ticket(Arc<Mutex<Option<Outcome>>>);

impl Ticket {
    /// The outcome, once the request is done.
    ///
    /// # Examples
    ///
    /// See [`Ticket`].
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether the request is done.
    ///
    /// # Examples
    ///
    /// ```
    /// assert!(!enshwick_loader_api::inventory::Ticket::default().is_done());
    /// ```
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }
}

type Callback = Box<dyn FnOnce(&Outcome) + Send>;

/// A request on its way: where its outcome goes.
struct Pending {
    ticket: Ticket,
    done: Callback,
}

impl Pending {
    fn complete(self, outcome: &Outcome) {
        *self.ticket.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome.clone());
        (self.done)(outcome);
    }
}

unsafe extern "system" fn deliver(result: *const InventoryResult, user: *mut c_void) {
    if user.is_null() {
        return;
    }
    // SAFETY: `user` is the leaked `Pending` of `submit`, handed back once.
    let pending = unsafe { Box::from_raw(user.cast::<Pending>()) };
    // SAFETY: the loader's result, valid for the call; an all-zero result is
    // valid. Every field is there since minor 12.
    let outcome = unsafe { crate::abi::read_headed(result, size_of::<InventoryResult>()) }
        .map_or_else(
            || Outcome::not_run(Done::Unavailable),
            |r| Outcome::from_raw(&r),
        );
    crate::host::guard(
        "panic in an inventory callback",
        || (),
        || {
            pending.complete(&outcome);
        },
    );
}

/// Reflected layouts the reads use.
#[derive(Clone, Copy, Debug)]
struct Layout {
    setup: ComponentType,
    inventory: ComponentType,
    durability: ComponentType,
    owner: ComponentType,
    links: usize,
    categories: usize,
    initialized: usize,
    stack: usize,
    stack_count: usize,
    stack_pide: usize,
    durability_max: usize,
    /// The game's entity that inventory walks skip.
    skip: usize,
}

/// Inventory reads and transactions on the server; get it from
/// [`Load::inventories`]. Copy it anywhere.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::ecs::Entity;
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let inventories = load.inventories()?;
///     let (corpse, player) = (Entity(40), Entity(2));
///     load.scheduler()?.next_tick(move || {
///         let full = inventories.read().slots(corpse).iter().filter(|s| !s.is_empty()).count();
///         enshwick_loader_api::info!("{full} stacks to loot");
///         inventories.move_all(corpse, player, |outcome| {
///             enshwick_loader_api::info!("looted: {}", outcome.is_committed());
///         });
///     });
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Inventories {
    host: Host,
    world: World,
    components: Components,
    scheduler: Scheduler,
    layout: Layout,
}

impl Inventories {
    pub(crate) fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        let host = load.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, inventory_submit),
            "inventories",
        )?;
        let scheduler = load.scheduler()?;
        let status = (host.api().inventory_open)(host.handle());
        if !status.is_ok() {
            return Err(Off::new(format!(
                "inventories: unavailable on this game build (status {})",
                status.0
            )));
        }
        let world = load.world()?;
        let components = Components::resolve(load)?;
        let offset = |ty: &str, field: &str| {
            load.field_offset(ty, field)
                .and_then(|v| usize::try_from(v).map_err(|_| Off::new("offset")))
        };
        let layout = Layout {
            setup: world.component_type(load, "keen::ecs::InventorySetup")?,
            inventory: world.component_type(load, "keen::ecs::Inventory")?,
            durability: world.component_type(load, "keen::ecs::NetworkDurability")?,
            owner: world.component_type(load, "keen::ecs::OwnerRelationship")?,
            links: offset("keen::ecs::InventorySetup", "linksEntities")?,
            categories: offset("keen::ecs::InventorySetup", "linksCategories")?,
            initialized: offset("keen::ecs::InventorySetup", "isInitialized")?,
            stack: usize::try_from(load.type_size("keen::ItemStack")?)
                .map_err(|_| Off::new("ItemStack"))?,
            stack_count: offset("keen::ItemStack", "data")?,
            stack_pide: offset("keen::ItemStack", "data")? + size_of::<u32>(),
            durability_max: offset("keen::ecs::NetworkDurability", "durabilityMax")?,
            skip: usize::try_from(load.symbol("inventory.skip_entity")?)
                .map_err(|_| Off::new("inventory.skip_entity"))?,
        };
        Ok(Self {
            host,
            world,
            components,
            scheduler,
            layout,
        })
    }

    /// Reads on the server's game thread, between steps: from a game event
    /// or a [`Scheduler`] task. Elsewhere they return nothing.
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    #[must_use]
    pub const fn read(&self) -> Reader<'_> {
        Reader {
            inventories: self,
            ctx: core::ptr::null_mut(),
        }
    }

    /// Reads from inside a hooked ECS system, through its context.
    ///
    /// # Safety
    ///
    /// `ctx` is the live context of the hooked system, and the reader is
    /// used on its thread during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn held(inventories: &enshwick_loader_api::inventory::Inventories, ctx: *mut u64, player: enshwick_loader_api::ecs::Entity) -> u32 {
    ///     // SAFETY: called from a system's hook with its context.
    ///     unsafe { inventories.read_in(ctx) }.count(player, 0x0c01)
    /// }
    /// ```
    #[must_use]
    pub const unsafe fn read_in(&self, ctx: *mut u64) -> Reader<'_> {
        Reader {
            inventories: self,
            ctx,
        }
    }

    /// The static data of `item`; `None` for an unknown item, or before the
    /// game first ran its inventory system (a player was online). Any thread.
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    pub fn item_info(&self, item: u32) -> Option<ItemInfo> {
        let mut out = ItemData {
            header: Header::of::<ItemData>(),
            id: 0,
            max_stack: 0,
            reserved: 0,
            flags: 0,
        };
        let status = (self.host.api().item_info)(self.host.handle(), item, &raw mut out);
        status.is_ok().then_some(ItemInfo {
            id: out.id,
            max_stack: out.max_stack,
            flags: out.flags,
        })
    }

    /// Submits `batch` from any thread. `done` runs on the game thread after
    /// a later step with the outcome, which the returned [`Ticket`] also
    /// holds. A batch that breaks a limit is refused without reaching the
    /// game. If the mod is turned off first, `done` never runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    /// use enshwick_loader_api::inventory::{Batch, Inventories, Op};
    ///
    /// fn trade(inventories: &Inventories, player: Entity) {
    ///     let batch = Batch::new()
    ///         .with(Op::Take { from: player, item: 0x0c01, count: 5 })
    ///         .with(Op::Give { to: player, item: 0x5e01, count: 1 });
    ///     let _ticket = inventories.submit(&batch, |outcome| {
    ///         enshwick_loader_api::info!("trade: {:?}", outcome.done);
    ///     });
    /// }
    /// ```
    pub fn submit(&self, batch: &Batch, done: impl FnOnce(&Outcome) + Send + 'static) -> Ticket {
        let ticket = Ticket::default();
        let pending = Pending {
            ticket: ticket.clone(),
            done: Box::new(done),
        };
        if let Err(error) = batch.check() {
            self.later(pending, Done::Refused(error));
            return ticket;
        }
        let ops: Vec<InventoryOp> = batch.ops.iter().map(Op::raw).collect();
        let request = InventoryRequest {
            header: Header::of::<InventoryRequest>(),
            flags: batch.request_flags(),
            actor: batch.actor_entity().0,
            ops: ops.as_ptr(),
            op_count: u32::try_from(ops.len()).unwrap_or(0),
            reserved: 0,
        };
        let user = Box::into_raw(Box::new(pending));
        let status = (self.host.api().inventory_submit)(
            self.host.handle(),
            &raw const request,
            deliver,
            user.cast(),
        );
        if !status.is_ok() {
            // SAFETY: refused, so the loader keeps no copy of the pointer.
            let pending = unsafe { Box::from_raw(user) };
            self.later(*pending, Done::Unavailable);
        }
        ticket
    }

    /// Completes `pending` with `done` on the game thread after the next step.
    fn later(&self, pending: Pending, done: Done) {
        self.scheduler
            .next_tick(move || pending.complete(&Outcome::not_run(done)));
    }

    /// Gives `count` new items `item` to the inventory `to`, all or nothing,
    /// with the game's pickup message.
    ///
    /// # Examples
    ///
    /// ```
    /// fn reward(inventories: &enshwick_loader_api::inventory::Inventories, player: enshwick_loader_api::ecs::Entity) {
    ///     inventories.give(player, 0x0c01, 10, |outcome| {
    ///         if !outcome.is_committed() {
    ///             enshwick_loader_api::warn!("no room for the reward");
    ///         }
    ///     });
    /// }
    /// ```
    pub fn give(
        &self,
        to: Entity,
        item: u32,
        count: u32,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        self.submit(&Batch::new().with(Op::Give { to, item, count }), done)
    }

    /// Takes `count` items `item` from the inventory `from`, all or nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// fn charge(inventories: &enshwick_loader_api::inventory::Inventories, player: enshwick_loader_api::ecs::Entity) {
    ///     inventories.take(player, 0x0c01, 5, |outcome| enshwick_loader_api::info!("paid: {}", outcome.is_committed()));
    /// }
    /// ```
    pub fn take(
        &self,
        from: Entity,
        item: u32,
        count: u32,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        self.submit(&Batch::new().with(Op::Take { from, item, count }), done)
    }

    /// Destroys the stack in `slot`.
    ///
    /// # Examples
    ///
    /// ```
    /// fn burn(inventories: &enshwick_loader_api::inventory::Inventories, slot: enshwick_loader_api::inventory::SlotRef) {
    ///     inventories.destroy_slot(slot, |_| {});
    /// }
    /// ```
    pub fn destroy_slot(
        &self,
        slot: SlotRef,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        self.submit(&Batch::new().with(Op::DestroySlot(slot)), done)
    }

    /// Moves the stack in `slot`, or `amount` of it, into the inventory `to`;
    /// what does not fit stays in the source.
    ///
    /// # Examples
    ///
    /// ```
    /// fn hand_over(inventories: &enshwick_loader_api::inventory::Inventories, slot: enshwick_loader_api::inventory::SlotRef, player: enshwick_loader_api::ecs::Entity) {
    ///     inventories.move_slot(slot, player, None, |_| {});
    /// }
    /// ```
    pub fn move_slot(
        &self,
        slot: SlotRef,
        to: Entity,
        amount: Option<u16>,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        self.submit(&Batch::new().with(Op::MoveSlot { slot, to, amount }), done)
    }

    /// Moves every stack of the inventory `from` into `to`, as the game's
    /// "take all" does; what does not fit stays in the source. The source's
    /// action bar, equipment and locked slots stay; use [`Op::MoveAll`] in a
    /// [`Batch`] to include them.
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    pub fn move_all(
        &self,
        from: Entity,
        to: Entity,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        let all = Op::MoveAll {
            from,
            to,
            include_special: false,
        };
        self.submit(&Batch::new().with(all), done)
    }
}

/// Synchronous inventory reads; get one from [`Inventories::read`] or
/// [`Inventories::read_in`].
///
/// # Examples
///
/// See [`Inventories`].
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a> {
    inventories: &'a Inventories,
    /// A system's context, or null for the game thread.
    ctx: *mut u64,
}

impl Reader<'_> {
    fn component(&self, entity: Entity, ty: ComponentType) -> Option<*const u8> {
        if entity.is_none() {
            return None;
        }
        if self.ctx.is_null() {
            self.inventories
                .world
                .component(entity, ty)
                .map(<*const c_void>::cast)
        } else {
            // SAFETY: `read_in`'s caller vouches for the context and thread.
            unsafe {
                self.inventories
                    .components
                    .component(self.ctx, entity, ty.hash())
            }
            .map(<*mut u8>::cast_const)
        }
    }

    /// Every slot of the inventory `root`, its virtual links followed, empty
    /// slots included, in the game's order; empty when `root` has no
    /// inventory.
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    #[must_use]
    pub fn slots(&self, root: Entity) -> Vec<Slot> {
        let layout = &self.inventories.layout;
        let skip = if layout.skip == 0 {
            0
        } else {
            // SAFETY: a global entity id in the game's image.
            unsafe { read_at::<u32>(core::ptr::with_exposed_provenance(layout.skip), 0) }
        };
        let mut out = Vec::new();
        self.walk(root, 0, skip, &mut out);
        out
    }

    fn walk(&self, root: Entity, depth: u32, skip: u32, out: &mut Vec<Slot>) {
        let layout = &self.inventories.layout;
        let Some(setup) = self.component(root, layout.setup) else {
            return;
        };
        // SAFETY: a live InventorySetup holds its initialised flag.
        if unsafe { read_at::<u8>(setup, layout.initialized) } == 0 {
            return;
        }
        for i in 0..LINKS {
            // SAFETY: the setup holds 16 links and 16 categories.
            let link: u32 = unsafe { read_at(setup, layout.links + i * size_of::<u32>()) };
            if link == 0 {
                break;
            }
            if link == skip {
                continue;
            }
            // SAFETY: as above.
            let category = Category(unsafe { read_at(setup, layout.categories + i) });
            if category == Category::VIRTUAL {
                if depth < MAX_DEPTH {
                    self.walk(Entity(link), depth + 1, skip, out);
                }
                continue;
            }
            if let Some(slots) = self.component(Entity(link), layout.inventory) {
                out.extend((0..SLOTS).map(|index| {
                    // SAFETY: an Inventory holds 8 stacks.
                    unsafe { self.stack(slots, Entity(link), index, category) }
                }));
            }
        }
    }

    /// The stack `index` of the `Inventory` at `slots`.
    ///
    /// # Safety
    ///
    /// `slots` is a live `Inventory` and `index` below 8.
    unsafe fn stack(
        &self,
        slots: *const u8,
        inventory: Entity,
        index: u8,
        category: Category,
    ) -> Slot {
        let layout = &self.inventories.layout;
        let at = usize::from(index) * layout.stack;
        Slot {
            at: SlotRef::new(inventory, index),
            category,
            // SAFETY: forwarded from the caller.
            item: unsafe { read_at(slots, at) },
            // SAFETY: as above.
            count: unsafe { read_at(slots, at + layout.stack_count) },
            // SAFETY: as above.
            pide: Entity(unsafe { read_at(slots, at + layout.stack_pide) }),
        }
    }

    /// What `slot` holds, with [`Category::UNKNOWN`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn holds(reader: &enshwick_loader_api::inventory::Reader<'_>, slot: enshwick_loader_api::inventory::SlotRef) -> bool {
    ///     reader.slot(slot).is_some_and(|s| !s.is_empty())
    /// }
    /// ```
    #[must_use]
    pub fn slot(&self, slot: SlotRef) -> Option<Slot> {
        if slot.index() >= SLOTS {
            return None;
        }
        let slots = self.component(slot.inventory(), self.inventories.layout.inventory)?;
        // SAFETY: a live Inventory and an index below 8.
        Some(unsafe { self.stack(slots, slot.inventory(), slot.index(), Category::UNKNOWN) })
    }

    /// How many items `item` the inventory `root` holds.
    ///
    /// # Examples
    ///
    /// See [`Inventories::read_in`].
    #[must_use]
    pub fn count(&self, root: Entity, item: u32) -> u32 {
        self.slots(root)
            .iter()
            .filter(|s| s.item == item && !s.is_empty())
            .fold(0, |sum, s| sum.saturating_add(s.count))
    }

    /// The inventory a slot belongs to at the root, the chest or player,
    /// found by climbing `OwnerRelationship` from its sub-inventory; `None`
    /// when the sub-inventory has no owner.
    ///
    /// # Examples
    ///
    /// ```
    /// fn owner(reader: &enshwick_loader_api::inventory::Reader<'_>, slot: enshwick_loader_api::inventory::SlotRef) {
    ///     if let Some(root) = reader.root(slot) {
    ///         enshwick_loader_api::info!("slot of {}", root.0);
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn root(&self, slot: SlotRef) -> Option<Entity> {
        let mut entity = slot.inventory();
        let mut climbed = false;
        for _ in 0..MAX_OWNERS {
            let Some(owner) = self.component(entity, self.inventories.layout.owner) else {
                break;
            };
            // SAFETY: OwnerRelationship is the owner's entity id alone.
            let owner = Entity(unsafe { read_at(owner, 0) });
            if owner.is_none() || owner == entity {
                break;
            }
            entity = owner;
            climbed = true;
        }
        climbed.then_some(entity)
    }

    /// The durability of an item's own entity, its PIDE.
    ///
    /// # Examples
    ///
    /// ```
    /// fn worn(reader: &enshwick_loader_api::inventory::Reader<'_>, slot: &enshwick_loader_api::inventory::Slot) -> bool {
    ///     reader.durability(slot.pide).is_some_and(|d| d.current * 4 < d.max)
    /// }
    /// ```
    #[must_use]
    pub fn durability(&self, pide: Entity) -> Option<Durability> {
        let layout = &self.inventories.layout;
        let data = self.component(pide, layout.durability)?;
        Some(Durability {
            // SAFETY: a live NetworkDurability holds both values.
            current: unsafe { read_at(data, 0) },
            // SAFETY: as above.
            max: unsafe { read_at(data, layout.durability_max) },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(from: u32, to: u32) -> Op {
        Op::MoveAll {
            from: Entity(from),
            to: Entity(to),
            include_special: false,
        }
    }

    fn give(to: u32) -> Op {
        Op::Give {
            to: Entity(to),
            item: 7,
            count: 1,
        }
    }

    #[test]
    fn slot_refs_pack_the_sub_inventory_low_and_the_index_high() {
        let slot = SlotRef::new(Entity(u32::MAX), 7);
        assert_eq!(slot.0, 0x7_ffff_ffff);
        assert_eq!(slot.inventory(), Entity(u32::MAX));
        assert_eq!(slot.index(), 7);
        assert_eq!(SlotRef(0x41).index(), 0);
    }

    #[test]
    fn batches_are_checked_against_the_game_limits() {
        assert_eq!(Batch::new().check(), Err(BatchError::Empty));
        assert_eq!(
            Batch::new().with(all(5, 5)).check(),
            Err(BatchError::Malformed)
        );
        let zero = Op::Give {
            to: Entity(2),
            item: 7,
            count: 0,
        };
        assert_eq!(Batch::new().with(zero).check(), Err(BatchError::Malformed));
        let mut many = Batch::new();
        for _ in 0..=MAX_OPS {
            many.push(Op::DestroySlot(SlotRef(0x41)));
        }
        assert_eq!(many.check(), Err(BatchError::TooManyOps));
        let mut wide = Batch::new();
        for to in 1..=17 {
            wide.push(give(to));
        }
        assert_eq!(wide.check(), Err(BatchError::TooManyInventories));
        let heavy = Batch::new().with(all(1, 2)).with(all(3, 2)).with(give(2));
        assert_eq!(heavy.check(), Err(BatchError::TooLarge));
        assert_eq!(Batch::new().with(all(1, 2)).with(all(3, 2)).check(), Ok(()));
    }

    #[test]
    fn splitting_keeps_order_and_every_part_fits() {
        let mut big = Batch::new().allow_partial().actor(Entity(2));
        for from in 100..105 {
            big.push(all(from, 2));
        }
        for to in 200..230 {
            big.push(give(to));
        }
        let ops = big.ops().to_vec();
        let parts = big.split();
        assert!(parts.len() > 3);
        assert!(parts.iter().all(|p| p.check().is_ok()));
        assert!(
            parts
                .iter()
                .all(|p| p.allow_partial && p.actor == Entity(2))
        );
        let joined: Vec<Op> = parts.iter().flat_map(|p| p.ops().to_vec()).collect();
        assert_eq!(joined, ops);
        assert_eq!(Batch::new().with(give(2)).split().len(), 1);
        assert!(Batch::new().split().is_empty());
    }

    #[test]
    fn requests_notify_for_gives_and_credit_the_first_destination() {
        let loot = Batch::new().with(all(40, 2));
        assert_eq!(loot.request_flags(), 0);
        assert_eq!(
            loot.clone().notify().request_flags(),
            InventoryRequest::NOTIFY
        );
        assert_eq!(loot.actor_entity(), Entity(2));
        let paid = Batch::new()
            .with(Op::Take {
                from: Entity(3),
                item: 1,
                count: 1,
            })
            .with(give(3))
            .allow_partial();
        assert_eq!(
            paid.request_flags(),
            InventoryRequest::ALLOW_PARTIAL | InventoryRequest::NOTIFY
        );
        assert_eq!(paid.actor_entity(), Entity(3));
        assert_eq!(loot.actor(Entity(9)).actor_entity(), Entity(9));
    }

    #[test]
    fn operations_become_the_loaders_records() {
        let slot = SlotRef::new(Entity(0x41), 2);
        let raw = Op::MoveSlot {
            slot,
            to: Entity(2),
            amount: Some(3),
        }
        .raw();
        assert_eq!(
            (raw.kind, raw.target, raw.other, raw.count),
            (InventoryOp::MOVE_SLOT, slot.0, 2, 3)
        );
        let special = Op::MoveAll {
            from: Entity(40),
            to: Entity(2),
            include_special: true,
        }
        .raw();
        assert_eq!(special.flags, InventoryOp::INCLUDE_SPECIAL);
    }

    #[test]
    fn outcomes_read_the_loaders_result() {
        let ops = [
            InventoryOpResult {
                error: 0,
                remaining: 0,
            },
            InventoryOpResult {
                error: 0x15,
                remaining: 4,
            },
        ];
        let raw = InventoryResult {
            header: Header::of::<InventoryResult>(),
            outcome: InventoryResult::ROLLED_BACK,
            op_count: 2,
            ops: ops.as_ptr(),
        };
        let outcome = Outcome::from_raw(&raw);
        assert_eq!(outcome.done, Done::RolledBack);
        assert!(!outcome.is_committed());
        assert_eq!(
            outcome.ops[1],
            OpResult {
                error: OpResult::NO_ROOM,
                remaining: 4
            }
        );
        let empty = InventoryResult {
            outcome: InventoryResult::TIMED_OUT,
            op_count: 0,
            ops: core::ptr::null(),
            ..raw
        };
        assert_eq!(Outcome::from_raw(&empty), Outcome::not_run(Done::TimedOut));
    }
}
