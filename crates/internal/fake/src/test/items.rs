//! Players' actions with items, as the loader hands them to item filters.

use crate::abi::ItemAction;

use super::Installed;
use super::table;

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Hands `call` to the mod's item filters as the loader would: an
    /// action the filters can refuse stops at the first refusal, and one
    /// after the fact ([`ItemAction::PICKUP`], [`ItemAction::PLACE`],
    /// [`ItemAction::CONSUMED`]) reaches every filter, and a
    /// [`ItemAction::CONSUME`] every filter allowed is followed by its
    /// `CONSUMED`, as when the game then takes the item; see
    /// [`Installed::refused_use`] for a use the game refuses. Whether every
    /// filter allowed it.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// use enshwick::abi::{Header, ItemAction};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let call = ItemAction {
    ///     header: Header::of::<ItemAction>(),
    ///     kind: ItemAction::CONSUME,
    ///     player: enshwick::Entity(1),
    ///     item: enshwick::ItemId::new(0x5e01),
    ///     count: 1,
    ///     inventory: enshwick::Entity(2),
    ///     slot: 0,
    /// };
    /// assert!(loader.item_action(&call));
    /// ```
    #[must_use]
    pub fn item_action(&self, call: &ItemAction) -> bool {
        let allowed = ask(call);
        if allowed && call.kind == ItemAction::CONSUME {
            let _heard = ask(&ItemAction {
                kind: ItemAction::CONSUMED,
                ..*call
            });
        }
        allowed
    }

    /// Hands the use `call` to the mod's item filters as the loader would
    /// for a use the game refuses after them, such as a potion at full
    /// health, or one whose player does anything else before the game takes
    /// the item, such as dragging the stack out of its slot: no
    /// [`ItemAction::CONSUMED`] follows. Whether every filter allowed it.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// use enshwick::abi::{Header, ItemAction};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let call = ItemAction {
    ///     header: Header::of::<ItemAction>(),
    ///     kind: ItemAction::CONSUME,
    ///     player: enshwick::Entity(1),
    ///     item: enshwick::ItemId::new(0x5e01),
    ///     count: 1,
    ///     inventory: enshwick::Entity(2),
    ///     slot: 0,
    /// };
    /// assert!(loader.refused_use(&call));
    /// ```
    #[must_use]
    pub fn refused_use(&self, call: &ItemAction) -> bool {
        ask(call)
    }
}

/// Asks the installed mod's filters about `call` as the loader does: a
/// refusable action stops at the first refusal, one after the fact reaches
/// every filter. Whether it is allowed.
fn ask(call: &ItemAction) -> bool {
    let filters = table::with(Vec::new(), |s| s.item_filters.clone());
    let refusable = !matches!(
        call.kind,
        ItemAction::PICKUP | ItemAction::PLACE | ItemAction::CONSUMED
    );
    let mut allowed = true;
    for filter in &filters {
        if refusable && !allowed {
            break;
        }
        // SAFETY: a valid call; `user` is the mod's pointer.
        allowed &= filter.run(|callback, user| unsafe { callback(call, user) != 0 });
    }
    allowed || !refusable
}
