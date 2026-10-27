use crate::Entity;
use crate::abi::{InventoryOp, InventoryRequest};

use super::slots::{Fill, SlotRef};
use super::{MAX_INVENTORIES, MAX_OPS, OP_LOG, inventories};

/// One change in a [`Batch`].
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
/// use enshwick::inventory::Op;
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
        /// How full each new item's meter starts; `None` for its
        /// template's start. Items without a meter ignore it.
        fill: Option<Fill>,
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
    /// Replaces the stack in a slot with new items, each with its own entity
    /// and its meter at `fill`, asked for the same slot: gives an item that
    /// lost its entity, such as one from before its mod gave it a
    /// [`FuelSetup`](crate::content::items::FuelSetup), a fresh one.
    Renew {
        /// The stack.
        slot: SlotRef,
        /// The inventory the slot belongs to, at its root
        /// ([`Reader::root`](super::Reader::root)).
        root: Entity,
        /// The new items' id.
        item: u32,
        /// How many.
        count: u32,
        /// Their meter's fill.
        fill: Fill,
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
    pub(super) fn raw(&self) -> InventoryOp {
        let entity = |e: Entity| u64::from(e.0);
        match *self {
            Self::Give {
                to,
                item,
                count,
                fill,
            } => InventoryOp {
                kind: InventoryOp::GIVE,
                flags: if fill.is_some() {
                    InventoryOp::FILLED
                } else {
                    0
                },
                target: entity(to),
                other: fill.map_or(0, |f| u64::from(f.fraction().to_bits())),
                item,
                count,
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
            Self::Renew {
                slot,
                root,
                item,
                count,
                fill,
            } => InventoryOp {
                kind: InventoryOp::RENEW,
                flags: InventoryOp::FILLED,
                target: slot.0,
                other: entity(root) | u64::from(fill.fraction().to_bits()) << 32,
                item,
                count,
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
            Self::Renew { .. } => 17,
            Self::DestroySlot(_) => 1,
            Self::MoveSlot { .. } => 4,
            Self::MoveAll { .. } => 256,
        }
    }

    /// The inventories the operation names.
    pub(super) const fn inventories(&self) -> [Entity; 2] {
        match *self {
            Self::Give { to, .. } => [to, Entity::NONE],
            Self::Take { from, .. } => [from, Entity::NONE],
            Self::DestroySlot(slot) => [slot.inventory(), Entity::NONE],
            Self::Renew { root, .. } => [root, Entity::NONE],
            Self::MoveSlot { slot, to, .. } => [slot.inventory(), to],
            Self::MoveAll { from, to, .. } => [from, to],
        }
    }

    /// Whether the operation names what it needs.
    fn is_valid(&self) -> bool {
        match *self {
            Self::Give {
                to: e, item, count, ..
            }
            | Self::Take {
                from: e,
                item,
                count,
            } => !e.is_none() && item != 0 && count != 0 && i32::try_from(count).is_ok(),
            Self::DestroySlot(slot) => !slot.inventory().is_none(),
            Self::Renew {
                slot,
                root,
                item,
                count,
                ..
            } => {
                !slot.inventory().is_none()
                    && !root.is_none()
                    && item != 0
                    && count != 0
                    && i32::try_from(count).is_ok()
            }
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
/// let empty = enshwick::inventory::Batch::new();
/// assert_eq!(empty.check(), Err(enshwick::inventory::BatchError::Empty));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
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
/// use enshwick::Entity;
/// use enshwick::inventory::{Batch, Op};
///
/// // A trade: 5 coins out, a sword in, or neither.
/// let player = Entity(2);
/// let trade = Batch::new()
///     .with(Op::Take { from: player, item: 0x0c01, count: 5 })
///     .with(Op::Give { to: player, item: 0x5e01, count: 1, fill: None });
/// assert_eq!(trade.check(), Ok(()));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    pub(super) ops: Vec<Op>,
    pub(super) allow_partial: bool,
    notify: bool,
    pub(super) actor: Entity,
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
    /// let mut batch = enshwick::inventory::Batch::new();
    /// batch.push(enshwick::inventory::Op::DestroySlot(enshwick::inventory::SlotRef(0x41)));
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
    /// use enshwick::Entity;
    /// use enshwick::inventory::{Batch, Op};
    ///
    /// // As many as fit.
    /// let _ = Batch::new().with(Op::Give { to: Entity(2), item: 7, count: 500, fill: None }).allow_partial();
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
    /// use enshwick::Entity;
    /// use enshwick::inventory::{Batch, Op};
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
    /// use enshwick::Entity;
    /// use enshwick::inventory::{Batch, Op};
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
    /// use enshwick::Entity;
    /// use enshwick::inventory::{Batch, Op};
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

    pub(super) fn request_flags(&self) -> u32 {
        let mut flags = 0;
        if self.allow_partial {
            flags |= InventoryRequest::ALLOW_PARTIAL;
        }
        if self.notify || self.ops.iter().any(|op| matches!(op, Op::Give { .. })) {
            flags |= InventoryRequest::NOTIFY;
        }
        flags
    }

    pub(super) fn actor_entity(&self) -> Entity {
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
