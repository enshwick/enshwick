//! The local player's inventory as the client's own copy of the world holds
//! it, on any client, a joined one included.

use super::Input;
use crate::abi::{ClientSlot as Raw, Status};
use crate::{ItemId, WorldError};
use enshwick_inventory::inventory::{InventoryCategory, Slot, SlotRef};
use enshwick_world::__internal::world_status;

/// One slot of the local player's inventory, from [`Input::client_slots`].
///
/// # Examples
///
/// ```
/// use enshwick::input::PlayerSlot;
///
/// fn on_the_bar(slots: &[PlayerSlot]) -> usize {
///     slots.iter().filter(|slot| slot.actionbar && !slot.slot.is_empty()).count()
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub struct PlayerSlot {
    /// The slot and what it holds, as the server's inventory reads name it.
    pub slot: Slot,
    /// Whether the slot is one of the action bar's.
    pub actionbar: bool,
}

impl PlayerSlot {
    fn from_raw(raw: &Raw) -> Self {
        Self {
            slot: Slot::new(
                SlotRef(raw.slot),
                InventoryCategory(raw.category),
                ItemId::new(raw.item),
                raw.count,
                (!raw.pide.is_none()).then_some(raw.pide),
            ),
            actionbar: raw.flags & Raw::ACTIONBAR != 0,
        }
    }
}

#[crate::script(client_slots)]
impl Input {
    /// Every slot of the local player's inventory, empty ones included, in
    /// the game's order: the equipment, the action bar and the backpack, as
    /// the client's last step read them; `None` while no step read them in
    /// the last second. Any thread.
    ///
    /// The values are the client's own copy, the one its screens draw, so on
    /// a joined client they may trail the server by a step. The first call
    /// starts the reads, so it answers `None`; they then run in each step
    /// until a few seconds after the last call, and cost nothing after.
    ///
    /// # Errors
    ///
    /// [`WorldError::Unavailable`] on a server and on a build whose slots
    /// the loader cannot read.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let input = load.input()?;
    ///     load.on_frame(move |frame| {
    ///         if let Ok(Some(slots)) = input.client_slots() {
    ///             let used = slots.iter().filter(|s| !s.slot.is_empty()).count();
    ///             frame.label(format!("{used} slots hold an item").as_str());
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    pub fn client_slots(self) -> Result<Option<Vec<PlayerSlot>>, WorldError> {
        let host = self.0.host;
        let entry = u32::try_from(size_of::<Raw>()).unwrap_or(0);
        let mut count = 0u32;
        let list = |out: *mut Raw, capacity: u32, count: &mut u32| {
            (host.api().client_slots)(host.handle(), out, entry, capacity, count)
        };
        match list(core::ptr::null_mut(), 0, &mut count) {
            Status::NOT_LOADED => return Ok(None),
            status => world_status(status)?,
        }
        let mut raw = vec![Raw::EMPTY; usize::try_from(count).unwrap_or(0)];
        let capacity = u32::try_from(raw.len()).unwrap_or(0);
        match list(raw.as_mut_ptr(), capacity, &mut count) {
            Status::NOT_LOADED => return Ok(None),
            status => world_status(status)?,
        }
        // Slots read between the two calls past the room are left out.
        let read = usize::try_from(count).unwrap_or(0).min(raw.len());
        Ok(Some(raw[..read].iter().map(PlayerSlot::from_raw).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Entity;

    #[test]
    fn a_raw_slot_names_its_place_item_and_the_action_bar() {
        let raw = enshwick_abi::headed!(Raw {
            slot: 0x41 | 2 << 32,
            item: 7,
            count: 3,
            pide: Entity::from_engine(0),
            category: 4,
            flags: Raw::ACTIONBAR,
            ..Raw::EMPTY
        });
        let slot = PlayerSlot::from_raw(&raw);
        assert_eq!(
            (slot.slot.at.inventory(), slot.slot.at.index()),
            (Entity::from_engine(0x41), 2)
        );
        assert_eq!(slot.slot.category, InventoryCategory::GENERIC);
        assert_eq!((slot.slot.count, slot.slot.pide), (3, None));
        assert!(slot.actionbar);
        let worn = PlayerSlot::from_raw(&enshwick_abi::headed!(Raw {
            pide: Entity::from_engine(900),
            ..Raw::EMPTY
        }));
        assert_eq!(worn.slot.pide, Some(Entity::from_engine(900)));
        assert!(!worn.actionbar);
    }
}
