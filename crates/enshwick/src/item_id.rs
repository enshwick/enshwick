//! The id the game gives an item.

/// An item of the game, by the id the game gives it: the id an item's
/// static data and an inventory slot carry.
///
/// # Examples
///
/// ```
/// use enshwick::ItemId;
///
/// let stone = ItemId(0x5678);
/// assert!(stone < ItemId(0x6000));
/// ```
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ItemId(pub u32);
