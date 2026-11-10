//! What a prop holds that a player would lose if it were removed and placed again.

use crate::abi::TransactionInfo;

/// A kind of state a prop holds that the game keeps on the prop itself and
/// a prop placed again from its item would not have; the names are the
/// game's components.
///
/// # Examples
///
/// See [`PropState`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum HeldState {
    /// Items in its `Inventory`, its linked inventories included: a chest's
    /// contents, a station's materials and outputs.
    Inventory,
    /// A plant, its `Growth`: any plant the transaction did not place, as
    /// the level a plant starts at is not known; one it placed once its
    /// `Level` differs from the level it first stood with.
    Growth,
    /// A `FactoryStation`'s production: a recipe running or queued.
    FactoryStation,
    /// A survivor assigned to the `NpcBed`.
    NpcBed,
    /// Survivors assigned to it: `StoreAssociatedNpcIds`.
    StoreAssociatedNpcIds,
    /// Text a player wrote on it: `CustomPlayerString`.
    CustomPlayerString,
    /// More `Fuel` charges than a fresh one starts with, in a prop that
    /// does not refill itself, such as a barrel or a station; a well or a
    /// rain barrel never counts.
    Fuel,
    /// Wiring to other logic: `LogicGraphNodeConnected`.
    LogicGraphNodeConnected,
    /// A logic trigger's settings, such as its area: `LogicGraphTrigger`.
    /// A trigger the transaction did not place always counts, as the loader
    /// cannot tell a player's settings from its item's; one it placed
    /// counts once its settings differ from those it first stood with.
    LogicGraphTrigger,
}

impl HeldState {
    /// Its `TransactionInfo::HOLDS_` bit.
    #[cfg(any(test, feature = "test"))]
    pub(crate) fn bit(self) -> u32 {
        Self::ALL
            .iter()
            .find(|&&(kind, _)| kind == self)
            .map_or(0, |&(_, bit)| bit)
    }

    const ALL: [(Self, u32); 9] = [
        (Self::Inventory, TransactionInfo::HOLDS_INVENTORY),
        (Self::Growth, TransactionInfo::HOLDS_GROWTH),
        (Self::FactoryStation, TransactionInfo::HOLDS_FACTORY_STATION),
        (Self::NpcBed, TransactionInfo::HOLDS_NPC_BED),
        (
            Self::StoreAssociatedNpcIds,
            TransactionInfo::HOLDS_STORE_ASSOCIATED_NPC_IDS,
        ),
        (
            Self::CustomPlayerString,
            TransactionInfo::HOLDS_CUSTOM_PLAYER_STRING,
        ),
        (Self::Fuel, TransactionInfo::HOLDS_FUEL),
        (
            Self::LogicGraphNodeConnected,
            TransactionInfo::HOLDS_LOGIC_GRAPH_NODE_CONNECTED,
        ),
        (
            Self::LogicGraphTrigger,
            TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER,
        ),
    ];
}

/// The kinds of state a prop holds that a player would lose, which stop
/// a removal and an undo.
///
/// [`Transaction::remove_prop`](super::Transaction::remove_prop) refuses a
/// prop that holds any, and an undo will not take one away. A switch's
/// on or off and a container's Quick Stash setting are settings and do not
/// count.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::WorldError;
/// use enshwick::world::{HeldState, TransactionStatus, World};
///
/// fn why(world: World, id: enshwick::world::TransactionId) {
///     if let Some(TransactionStatus::Failed(WorldError::HoldsState(state))) = world.transaction_status(id) {
///         if state.holds(HeldState::Inventory) {
///             enshwick::info!("empty the chest first");
///         }
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PropState(u32);

impl PropState {
    pub(crate) const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Whether it holds `kind`.
    ///
    /// # Examples
    ///
    /// See [`PropState`].
    #[must_use]
    pub fn holds(self, kind: HeldState) -> bool {
        HeldState::ALL
            .iter()
            .any(|&(known, bit)| known == kind && self.0 & bit != 0)
    }

    /// Every kind it holds.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn list(state: enshwick::world::PropState) -> Vec<String> {
    ///     state.iter().map(|kind| format!("{kind:?}")).collect()
    /// }
    /// ```
    pub fn iter(self) -> impl Iterator<Item = HeldState> {
        HeldState::ALL
            .into_iter()
            .filter(move |&(_, bit)| self.0 & bit != 0)
            .map(|(kind, _)| kind)
    }
}

impl crate::script::Typed for PropState {
    const TY: &'static crate::script::Ty =
        &crate::script::Ty::List(<HeldState as crate::script::Typed>::TY);
}

impl crate::script::IntoScript for PropState {
    fn into_value(self) -> crate::script::Value {
        crate::script::Value::List(
            self.iter()
                .map(crate::script::IntoScript::into_value)
                .collect(),
        )
    }
}

impl crate::script::FromScript for PropState {
    fn from_value(value: crate::script::Value) -> Result<Self, crate::script::Error> {
        let kinds: Vec<HeldState> = crate::script::FromScript::from_value(value)?;
        Ok(Self(kinds.into_iter().fold(0, |bits, kind| {
            bits | HeldState::ALL
                .iter()
                .find(|(known, _)| *known == kind)
                .map_or(0, |&(_, bit)| bit)
        })))
    }
}

impl core::fmt::Display for PropState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, kind) in self.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{kind:?}")?;
        }
        Ok(())
    }
}
