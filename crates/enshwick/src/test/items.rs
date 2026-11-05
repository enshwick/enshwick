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
    /// `CONSUMED`. Whether every filter allowed it.
    ///
    /// # Examples
    ///
    /// ```
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
        let filters = table::with(Vec::new(), |s| s.item_filters.clone());
        let ask = |call: &ItemAction| {
            let refusable = !matches!(
                call.kind,
                ItemAction::PICKUP | ItemAction::PLACE | ItemAction::CONSUMED
            );
            let mut allowed = true;
            for &(callback, user) in &filters {
                if refusable && !allowed {
                    break;
                }
                let user = core::ptr::with_exposed_provenance_mut(user);
                // SAFETY: a valid call; `user` is the mod's pointer.
                allowed &= unsafe { callback(call, user) != 0 };
            }
            allowed || !refusable
        };
        let allowed = ask(call);
        if allowed && call.kind == ItemAction::CONSUME {
            let _heard = ask(&ItemAction {
                kind: ItemAction::CONSUMED,
                ..*call
            });
        }
        allowed
    }
}
