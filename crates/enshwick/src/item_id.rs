//! The id the game gives each item.

/// An item of the game by the number its item data gives it, as a prop's
/// `UsedItem` names the item the prop was placed from.
///
/// # Examples
///
/// ```
/// use enshwick::ItemId;
///
/// let torch = ItemId::from(1_042);
/// assert_eq!(torch.get(), 1_042);
/// assert_eq!(torch, ItemId(1_042));
/// ```
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct ItemId(pub u32);

impl ItemId {
    /// The game's number for the item.
    ///
    /// # Examples
    ///
    /// See [`ItemId`].
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for ItemId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl From<ItemId> for u32 {
    fn from(item: ItemId) -> Self {
        item.0
    }
}
