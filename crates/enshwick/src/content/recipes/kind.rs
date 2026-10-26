//! What the item a recipe makes is, as the game's item data classes it.

use super::super::Guid;

/// What the item a recipe makes is, as the game classes it, from
/// [`RecipeView::kind`](super::RecipeView::kind); the game may class more
/// items in later builds.
///
/// # Examples
///
/// See [`RecipeView::kind`](super::RecipeView::kind).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RecipeKind {
    /// A building block, such as a wall or floor block.
    BuildingBlock,
    /// A roof block.
    Roof,
    /// An overgrowth material.
    Overgrowth,
    /// A terrain material.
    Terrain,
    /// Anything else: equipment, props, furniture and decorations among them.
    Other,
}

impl RecipeKind {
    /// Every kind, in the order of their codes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::recipes::RecipeKind;
    ///
    /// assert!(RecipeKind::ALL.contains(&RecipeKind::Roof));
    /// ```
    pub const ALL: &[Self] = &[
        Self::Other,
        Self::BuildingBlock,
        Self::Roof,
        Self::Overgrowth,
        Self::Terrain,
    ];

    /// The loader's code for this kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi;
    /// use enshwick::content::recipes::RecipeKind;
    ///
    /// assert_eq!(RecipeKind::Roof.code(), abi::RecipeKind::ROOF);
    /// ```
    #[must_use]
    pub const fn code(self) -> crate::abi::RecipeKind {
        use crate::abi::RecipeKind as Abi;
        match self {
            Self::BuildingBlock => Abi::BUILDING_BLOCK,
            Self::Roof => Abi::ROOF,
            Self::Overgrowth => Abi::OVERGROWTH,
            Self::Terrain => Abi::TERRAIN,
            Self::Other => Abi::OTHER,
        }
    }

    /// The kind of the item `item`, as the loader answers it.
    pub(super) fn of(item: Guid) -> Option<Self> {
        let host = crate::host()?;
        let mut out = crate::abi::RecipeKind::OTHER;
        (host.api().recipe_kind)(host.handle(), item.id(), &raw mut out)
            .is_ok()
            .then(|| out.into())
    }
}

/// A kind code a newer loader may add reads as [`RecipeKind::Other`].
impl From<crate::abi::RecipeKind> for RecipeKind {
    fn from(kind: crate::abi::RecipeKind) -> Self {
        use crate::abi::RecipeKind as Abi;
        match kind {
            Abi::BUILDING_BLOCK => Self::BuildingBlock,
            Abi::ROOF => Self::Roof,
            Abi::OVERGROWTH => Self::Overgrowth,
            Abi::TERRAIN => Self::Terrain,
            _ => Self::Other,
        }
    }
}

impl From<RecipeKind> for crate::abi::RecipeKind {
    fn from(kind: RecipeKind) -> Self {
        kind.code()
    }
}
