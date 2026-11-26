//! What a player's move carries into an interaction filter: its kind, the
//! stack, the amount, both containers and both slots.

use super::{Interaction, RawInteraction};
use crate::{Entity, ItemId};

impl Interaction {
    /// The kind of move a [`Kind::Move`](super::Kind::Move) is, such as Take
    /// All; `None` for another kind or a move the platform could not read. A
    /// script reads it as `moved.transfer`, Luau records having no methods.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::interactions::{Kind, Transfer};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Nobody empties a container with one press.
    ///     load.on_interaction(|i| i.kind != Kind::Move || i.transfer() != Some(Transfer::TakeAll))
    /// }
    /// ```
    #[must_use]
    pub fn transfer(&self) -> Option<Transfer> {
        self.moved.map(|moved| moved.transfer)
    }

    /// This interaction with the move it carries, for a test of a filter.
    ///
    /// Luau has no setter: a script writes the `moved` field of the table.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Examples
    ///
    /// See [`Move::new`].
    #[must_use]
    pub const fn with_moved(mut self, moved: Move) -> Self {
        self.moved = Some(moved);
        self
    }
}

/// What a player moves, beside the containers each call of a move names.
///
/// A move of a whole inventory, such as Take All, names no slot and no
/// item: the game moves stack after stack, and a
/// [`container rule`](../struct.Load.html#method.container_rule) answers for
/// each stack that enters a container.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// See [`Interaction::transfer`] and [`Move::new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct Move {
    /// The kind of move.
    pub transfer: Transfer,
    /// How much of the stack moves; `None` when the platform could not read
    /// it.
    pub action: Option<InventoryActionType>,
    /// The amount the player asked for, which counts only for
    /// [`InventoryActionType::CustomAmount`].
    pub amount: u32,
    /// The item of the stack in [`from_slot`](Self::from_slot); `None` for
    /// a move of a whole inventory.
    pub item: Option<ItemId>,
    /// How many items that stack holds.
    pub stack: u32,
    /// The container the items leave.
    pub from: Option<Entity>,
    /// The container the items enter; `None` when the items are destroyed.
    pub to: Option<Entity>,
    /// The slot the items leave; `None` for a move of a whole inventory.
    pub from_slot: Option<crate::inventory::SlotRef>,
    /// The slot the items enter; `None` when the game picks the slots.
    pub to_slot: Option<crate::inventory::SlotRef>,
}

impl Move {
    /// A `transfer` from container `from` into `to` as a whole-inventory move
    /// is handed to a filter: action [`InventoryActionType::All`] and no slot,
    /// item or amount, for a test of a filter; the other fields are public to
    /// set.
    ///
    /// Luau has no constructor: a script writes the record as a table.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::interactions::{InventoryActionType, Interaction, Kind, Move, Transfer};
    ///
    /// const OWNER: Entity = Entity::from_engine(4096);
    ///
    /// fn allowed(i: &Interaction) -> bool {
    ///     i.player == OWNER || i.moved.is_none_or(|m| m.transfer != Transfer::TakeAll)
    /// }
    ///
    /// let (stranger, chest) = (Entity::from_engine(4097), Entity::from_engine(5000));
    /// let take_all = Move::new(Transfer::TakeAll, Some(chest), Some(stranger));
    /// assert_eq!(take_all.action, Some(InventoryActionType::All));
    /// let call = Interaction::new(Kind::Move, stranger, chest).with_moved(take_all);
    /// assert!(!allowed(&call));
    /// ```
    #[must_use]
    pub const fn new(transfer: Transfer, from: Option<Entity>, to: Option<Entity>) -> Self {
        Self {
            transfer,
            action: Some(InventoryActionType::All),
            amount: 0,
            item: None,
            stack: 0,
            from,
            to,
            from_slot: None,
            to_slot: None,
        }
    }
}

/// How much of a stack a player moves, as the game's `InventoryActionType`
/// names it.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::interactions::InventoryActionType;
///
/// assert_ne!(InventoryActionType::Half, InventoryActionType::All);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum InventoryActionType {
    /// One item.
    One,
    /// Half the stack.
    Half,
    /// The whole stack, or every stack of a whole-inventory move.
    All,
    /// The amount in [`Move::amount`].
    CustomAmount,
    /// The stack is destroyed.
    Destroy,
}

/// The kind of move a player makes; the names are enshwick's, after the
/// buttons and gestures the game offers.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// See [`Interaction::transfer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Transfer {
    /// One slot's stack, dragged, split or destroyed, or taken one stack at
    /// a time from a loot viewer.
    Slot,
    /// Take All: every stack from a container, or from a corpse or loot bag,
    /// into the player's inventory.
    TakeAll,
    /// Deposit All: every stack from the player's inventory into a
    /// container.
    DepositAll,
    /// Deposit Stacks: the player's stacks of items the container holds
    /// already.
    DepositStacks,
    /// Refill Stacks: the container's stacks of items the player holds
    /// already.
    RefillStacks,
    /// A gift to another player nearby.
    Gift,
    /// A stack to the action bar.
    ToActionbar,
    /// A stack to the backpack.
    ToBackpack,
}

impl Transfer {
    /// The kind of `call`'s move by `player`: its `TRANSFER_` mask, and
    /// whether it names a slot; a whole-inventory move leaves the player's
    /// inventory when `from` is the player.
    fn of(call: &RawInteraction) -> Self {
        let has = |bit: u8| call.transfer & bit != 0;
        let from_player = call.from == call.character;
        if has(RawInteraction::TRANSFER_GIFT) {
            Self::Gift
        } else if has(RawInteraction::TRANSFER_TO_ACTIONBAR) {
            Self::ToActionbar
        } else if has(RawInteraction::TRANSFER_TO_BACKPACK) {
            Self::ToBackpack
        } else if call.from_slot != 0 || call.to_slot != 0 {
            Self::Slot
        } else if has(RawInteraction::TRANSFER_REFILL_STACKS) {
            if from_player {
                Self::DepositStacks
            } else {
                Self::RefillStacks
            }
        } else if from_player {
            Self::DepositAll
        } else {
            Self::TakeAll
        }
    }
}

impl Move {
    /// The move `call` carries; `None` when the platform read none.
    pub(super) fn of(call: &RawInteraction) -> Option<Self> {
        if call.moving == 0 {
            return None;
        }
        let some = |entity: Entity| (!entity.is_none()).then_some(entity);
        let slot = |slot: u64| (slot != 0).then_some(crate::inventory::SlotRef(slot));
        Some(Self {
            transfer: Transfer::of(call),
            action: match call.action {
                RawInteraction::ACTION_ONE => Some(InventoryActionType::One),
                RawInteraction::ACTION_HALF => Some(InventoryActionType::Half),
                RawInteraction::ACTION_ALL => Some(InventoryActionType::All),
                RawInteraction::ACTION_AMOUNT => Some(InventoryActionType::CustomAmount),
                RawInteraction::ACTION_DESTROY => Some(InventoryActionType::Destroy),
                _ => None,
            },
            amount: call.amount,
            item: (call.moved.get() != 0).then_some(call.moved),
            stack: call.stack,
            from: some(call.from),
            to: some(call.to),
            from_slot: slot(call.from_slot),
            to_slot: slot(call.to_slot),
        })
    }
}
