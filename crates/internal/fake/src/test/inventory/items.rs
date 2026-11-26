//! Players' actions with items, as the loader hands them to item filters.

use enshwick_core::abi::ItemAction;

use crate::test::Installed;
use crate::test::table;

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
    /// use enshwick::abi::{Header, ItemAction};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let call = enshwick::abi::headed!(ItemAction {
    ///     kind: ItemAction::CONSUME,
    ///     character: enshwick::Entity::from_engine(1),
    ///     item: enshwick::ItemId::new(0x5e01),
    ///     count: 1,
    ///     inventory: enshwick::Entity::from_engine(2),
    ///     slot: 0,
    /// });
    /// assert!(loader.item_action(&call));
    /// ```
    #[must_use]
    pub fn item_action(&self, call: &ItemAction) -> bool {
        let allowed = ask(call);
        if allowed && call.kind == ItemAction::CONSUME {
            let _heard = ask(&enshwick_core::headed!(ItemAction {
                kind: ItemAction::CONSUMED,
                ..*call
            }));
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
    /// use enshwick::abi::{Header, ItemAction};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let call = enshwick::abi::headed!(ItemAction {
    ///     kind: ItemAction::CONSUME,
    ///     character: enshwick::Entity::from_engine(1),
    ///     item: enshwick::ItemId::new(0x5e01),
    ///     count: 1,
    ///     inventory: enshwick::Entity::from_engine(2),
    ///     slot: 0,
    /// });
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
/// Whether the item filters let the move `call` destroy its source stack,
/// as the loader asks them about a transfer of type `Destroy`; true for any
/// other move.
pub(crate) fn allows_destroy(call: &enshwick_core::abi::Interaction) -> bool {
    use enshwick_core::abi::Interaction;
    let (inventory, index) = (
        call.from_slot & u64::from(u32::MAX),
        call.from_slot >> 32 & 0xff,
    );
    let (Ok(inventory), Ok(index)) = (u32::try_from(inventory), u32::try_from(index)) else {
        return true;
    };
    if call.kind != Interaction::MOVE
        || call.moving == 0
        || call.action != Interaction::ACTION_DESTROY
        || inventory == 0
        || call.moved.get() == 0
        || call.stack == 0
    {
        return true;
    }
    ask(&enshwick_core::headed!(ItemAction {
        kind: ItemAction::TRASH,
        character: call.character,
        item: call.moved,
        count: call.stack,
        inventory: enshwick_core::Entity::from_engine(inventory),
        slot: index,
    }))
}

fn ask(call: &ItemAction) -> bool {
    let filters = table::with(Vec::new(), |s| s.inventory.item_filters.clone());
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
        allowed &= filter.run(|callback, user| unsafe {
            callback(call, user) != enshwick_core::abi::FILTER_REFUSE
        });
    }
    allowed || !refusable
}
