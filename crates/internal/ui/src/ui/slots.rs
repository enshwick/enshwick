//! Item slots in the game's style, and the slot of the game's own inventory
//! screen under the cursor.

use crate::ItemId;
use crate::abi::UiWidget;
use crate::inventory::{InventoryCategory, Slot, SlotRef};

use super::Frame;

/// What the player did to an item slot in one frame, from
/// [`Frame::item_slot`]. The name is enshwick's.
///
/// # Examples
///
/// See [`Frame::item_slot`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub struct SlotInput {
    /// The player clicked the slot in this frame.
    pub clicked: bool,
    /// The cursor is on the slot, or, while a gamepad drives the interface,
    /// the slot holds the focus.
    pub hovered: bool,
}

impl SlotInput {
    const fn from_raw(result: u32) -> Self {
        Self {
            clicked: result & UiWidget::SLOT_CLICKED != 0,
            hovered: result & UiWidget::SLOT_HOVERED != 0,
        }
    }
}

impl Frame<'_> {
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Draws an item slot as the game's inventory screens draw one: the
    /// icon of `item` in the game's slot frame, with `count` on it when
    /// more than one. Inside a window or a screen extension; nothing shows
    /// for a count of 0, an item the client does not know, or on a game
    /// build where the loader does not find the game's slot, and the slot
    /// then reads as neither clicked nor hovered. Slots drawn one after
    /// another sit side by side, as in the game's grids: three to a row in
    /// a panel, five in a window.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    /// use enshwick::ui::{Edge, HudScreenType};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let torch = ItemId::new(0x5e01);
    ///     load.extend_screen(HudScreenType::Backpack, Edge::Left, move |panel| {
    ///         if panel.item_slot(torch, 3).clicked {
    ///             // Send the server a player action of the mod's own.
    ///         }
    ///     })
    /// }
    /// ```
    pub fn item_slot(&mut self, item: ItemId, count: u32) -> SlotInput {
        let mut widget = enshwick_abi::headed!(UiWidget {
            kind: UiWidget::ITEM_SLOT,
            item,
            count,
            ..UiWidget::EMPTY
        });
        let status = (self.host.api().ui_widget)(self.host.handle(), self.token, &raw mut widget);
        if status.is_ok() {
            SlotInput::from_raw(widget.result)
        } else {
            SlotInput::default()
        }
    }

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// The slot of the game's own inventory screen under the cursor in this
    /// frame, such as one of the backpack's or an open chest's, with what
    /// it holds; `None` while the cursor is on none, outside those screens
    /// and on a game build where the loader does not find the game's slot.
    /// While a gamepad drives the interface, it is the slot that holds the
    /// screen's focus. The slot the player selected with a click does not
    /// count while the cursor is elsewhere.
    /// Its inventory is an entity of the client's world, and its category
    /// reads [`InventoryCategory::INVALID`], since the screen does not say.
    /// An item slot the mod draws itself is never this slot.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::{Edge, HudScreenType, Label, Style};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.extend_screen(HudScreenType::Backpack, Edge::Left, |panel| {
    ///         if let Some(slot) = panel.hovered_slot() {
    ///             let text = format!("Slot {} holds {}", slot.at.index(), slot.count);
    ///             panel.label(Label::new(&text).style(Style::Secondary));
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub fn hovered_slot(&self) -> Option<Slot> {
        let raw = &self.raw;
        (raw.hovered_slot != 0).then(|| {
            Slot::new(
                SlotRef(raw.hovered_slot),
                InventoryCategory::INVALID,
                raw.hovered_item,
                raw.hovered_count,
                (!raw.hovered_pide.is_none()).then_some(raw.hovered_pide),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_reads_its_click_and_hover_from_the_result_bits() {
        assert_eq!(SlotInput::from_raw(0), SlotInput::default());
        let both = SlotInput::from_raw(UiWidget::SLOT_CLICKED | UiWidget::SLOT_HOVERED);
        assert!(both.clicked && both.hovered);
        let hovered = SlotInput::from_raw(UiWidget::SLOT_HOVERED);
        assert!(!hovered.clicked && hovered.hovered);
    }
}
