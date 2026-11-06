//! Players' actions with items: a mod's say before a player uses, throws
//! away, equips or takes off an item, and its word after a pickup or a
//! placement.
//!
//! The filters run in the server's world only, in the dedicated server or
//! a hosting client, on the thread of the system that runs the action. A
//! refusal takes the game's path for an action it already ran, so the item
//! stays where it was. An item a mod adds is an [`ItemId`] like any other:
//! [`ItemId::from_guid`] of its content id's GUID.
//!
//! # Examples
//!
//! ```
//! use enshwick::content::{Id, id};
//! use enshwick::items::Kind;
//!
//! const TONIC: Id = id!("tonic");
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let tonic = enshwick::ItemId::from_guid(TONIC.guid());
//!     // Nobody throws the tonic away.
//!     load.on_item(tonic, Kind::Trash, |_| false)?;
//!     load.on_item(tonic, Kind::Consume, |used| {
//!         enshwick::info!("player {} drank a tonic", used.player.0);
//!         true
//!     })
//! }
//! ```

use core::ffi::c_void;

use crate::abi::ItemAction as RawItemAction;
use crate::inventory::SlotRef;
use crate::{Entity, ItemId};
use crate::{Load, Off};

/// What a player does with an item; the names are the game's.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Kind {
    /// Eats, drinks or uses it from the inventory; a refusal leaves it in
    /// its slot.
    Consume,
    /// Throws the stack away from the inventory; a refusal keeps it.
    Trash,
    /// Equips it; a refusal keeps the equipment as it was.
    Equip,
    /// Takes it off; a refusal keeps it on.
    Unequip,
    /// Picked it up, after the game moved it; the answer is ignored.
    Pickup,
    /// Placed it as a building piece or a prop, after the game placed it;
    /// the answer is ignored.
    Place,
    /// The game took the item of a [`Kind::Consume`] every filter allowed
    /// from its slot; a use the game refuses, such as a potion at full
    /// health, raises none. The answer is ignored. A reward or a count of
    /// uses listens here, since a later filter or the game may still refuse
    /// a `Consume`.
    Consumed,
}

impl Kind {
    /// Whether a filter's `false` refuses the action, which holds for the
    /// actions the filters hear before the game runs them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::items::Kind;
    ///
    /// assert!(Kind::Consume.can_refuse());
    /// assert!(!Kind::Pickup.can_refuse());
    /// ```
    #[must_use]
    pub const fn can_refuse(self) -> bool {
        matches!(
            self,
            Self::Consume | Self::Trash | Self::Equip | Self::Unequip
        )
    }

    /// The kind of a raw action's `kind`, `None` for one this build of the
    /// API does not know.
    const fn from_raw(kind: u32) -> Option<Self> {
        match kind {
            RawItemAction::CONSUME => Some(Self::Consume),
            RawItemAction::TRASH => Some(Self::Trash),
            RawItemAction::EQUIP => Some(Self::Equip),
            RawItemAction::UNEQUIP => Some(Self::Unequip),
            RawItemAction::PICKUP => Some(Self::Pickup),
            RawItemAction::PLACE => Some(Self::Place),
            RawItemAction::CONSUMED => Some(Self::Consumed),
            _ => None,
        }
    }
}

/// A player's action with an item, from [`Load::on_item_action`] and
/// [`Load::on_item`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct ItemAction {
    /// What the player does.
    pub kind: Kind,
    /// The player's character.
    pub player: Entity,
    /// The item.
    pub item: ItemId,
    /// How many: 1 for a use and [`Kind::Place`], the stack
    /// for the others.
    pub count: u32,
    /// The slot the item is in before the action; `None` for a pickup or a
    /// placement.
    pub slot: Option<SlotRef>,
}

impl ItemAction {
    /// The action the loader hands over, `None` for a kind this build does
    /// not know or a call without a player or an item.
    fn from_raw(call: &RawItemAction) -> Option<Self> {
        let kind = Kind::from_raw(call.kind)?;
        if call.player.is_none() || call.item.get() == 0 {
            return None;
        }
        let slot = u8::try_from(call.slot)
            .ok()
            .filter(|_| !call.inventory.is_none())
            .map(|index| SlotRef::new(call.inventory, index));
        Some(Self {
            kind,
            player: call.player,
            item: call.item,
            count: call.count,
            slot,
        })
    }
}

unsafe extern "system" fn run_filter<F>(call: *const RawItemAction, user: *mut c_void) -> u8
where
    F: Fn(&ItemAction) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(action) = (unsafe { call.as_ref() }).and_then(ItemAction::from_raw) else {
        return 1;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    u8::from(crate::host::guard(
        "panic in an item filter",
        || true,
        || filter(&action),
    ))
}

#[crate::script(items)]
impl Load<'_> {
    /// Runs `filter` on each player's action with any item in the server's
    /// world; returning `false` refuses a [`Kind::Consume`],
    /// [`Kind::Trash`], [`Kind::Equip`] or [`Kind::Unequip`] the game's own
    /// way, and is ignored for the others. It runs on the thread of the
    /// system that runs the action, once per action: keep it quick and never
    /// block.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the player input it reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::items::Kind;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_item_action(|action| {
    ///         if action.kind == Kind::Pickup {
    ///             enshwick::info!("{} items picked up", action.count);
    ///         }
    ///         true
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_item_action<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&ItemAction) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().item_filter)(host.handle(), run_filter::<F>, user),
            "item filter",
        )
    }

    /// Runs `handler` when a player does `kind` with `item`, as
    /// [`Load::on_item_action`] runs its filter: returning `false` refuses
    /// the action where [`Kind::can_refuse`] says it can. Actions with other
    /// items or of other kinds never reach it.
    ///
    /// # Errors
    ///
    /// As [`Load::on_item_action`].
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn on_item<F>(&mut self, item: ItemId, kind: Kind, handler: F) -> Result<(), Off>
    where
        F: Fn(&ItemAction) -> bool + Send + Sync + 'static,
    {
        self.on_item_action(move |action| {
            action.item != item || action.kind != kind || handler(action)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::Header;

    fn raw(kind: u32, inventory: u32, slot: u32) -> RawItemAction {
        RawItemAction {
            header: Header::of::<RawItemAction>(),
            kind,
            player: Entity(4096),
            item: ItemId::new(0x5e01),
            count: 1,
            inventory: Entity(inventory),
            slot,
        }
    }

    #[test]
    fn an_action_keeps_its_slot_only_when_an_inventory_holds_it() {
        let used = ItemAction::from_raw(&raw(RawItemAction::CONSUME, 4100, 3));
        assert_eq!(
            used.and_then(|a| a.slot),
            Some(SlotRef::new(Entity(4100), 3))
        );
        let picked = ItemAction::from_raw(&raw(RawItemAction::PICKUP, 0, 0));
        assert_eq!(picked.map(|a| (a.kind, a.slot)), Some((Kind::Pickup, None)));
    }

    #[test]
    fn every_raw_kind_reads_as_its_kind_and_only_the_first_four_refuse() {
        let kinds = [
            (RawItemAction::CONSUME, Kind::Consume, true),
            (RawItemAction::TRASH, Kind::Trash, true),
            (RawItemAction::EQUIP, Kind::Equip, true),
            (RawItemAction::UNEQUIP, Kind::Unequip, true),
            (RawItemAction::PICKUP, Kind::Pickup, false),
            (RawItemAction::PLACE, Kind::Place, false),
            (RawItemAction::CONSUMED, Kind::Consumed, false),
        ];
        for (raw, kind, refuses) in kinds {
            assert_eq!(Kind::from_raw(raw), Some(kind));
            assert_eq!(kind.can_refuse(), refuses, "{kind:?}");
        }
        let mut nobody = raw(RawItemAction::CONSUME, 4100, 3);
        nobody.player = Entity(0);
        assert_eq!(ItemAction::from_raw(&nobody), None, "no player, no action");
    }

    #[test]
    fn an_unknown_kind_or_a_slot_past_a_byte_reads_as_no_action() {
        assert_eq!(ItemAction::from_raw(&raw(99, 4100, 3)), None);
        let wide = ItemAction::from_raw(&raw(RawItemAction::TRASH, 4100, 300));
        assert_eq!(wide.map(|a| a.slot), Some(None));
    }
}
