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
        match kind {
            RecipeKind::BuildingBlock => Self::BUILDING_BLOCK,
            RecipeKind::Roof => Self::ROOF,
            RecipeKind::Overgrowth => Self::OVERGROWTH,
            RecipeKind::Terrain => Self::TERRAIN,
            RecipeKind::Other => Self::OTHER,
        }
    }
}
