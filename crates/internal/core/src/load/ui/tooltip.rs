//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::inventory::ItemInstance;
use crate::ui::TooltipLines;
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Adds lines under the game's rows in the tooltip and the detail panel
    /// of every item with an entity of its own, such as a weapon or a
    /// staff: `handler` runs on the thread that draws the interface once
    /// each time the tooltip shows another item, never once per frame, and
    /// the lines it pushes show until the tooltip shows another item. It
    /// reads what the client holds of the item, such as its
    /// [item data](crate::item_data), which on a joined client is what the
    /// server sent. Stackable items without an entity of their own get no
    /// lines. Client only; a client without Enshwick shows the game's rows
    /// alone. A panic in `handler` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server or after `init`. On a game build where the
    /// loader does not find the tooltip, `handler` never runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::{Label, Style};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.extend_item_tooltip(|item, lines| {
    ///         let text = format!("Item {}", item.item.get());
    ///         let _shown = lines.push(Label::new(&text).style(Style::Secondary));
    ///     })
    /// }
    /// ```
    pub fn extend_item_tooltip<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: FnMut(ItemInstance, &mut TooltipLines<'_>) + Send + 'static;
}
