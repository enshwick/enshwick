//! What the item a recipe makes is, as the game's item data classes it.

use super::super::Guid;

/// What the item a recipe makes is, as the game classes it, from
/// [`RecipeView::kind`](super::RecipeView::kind); the game may class more
/// items in later builds.
///
/// The name is enshwick's: the game has no recipe kind; it follows from the
/// output item's equipment slot and, for a placed prop, its item tags. A kind
/// the game names itself takes its name: [`RecipeKind::Decorative`] is the
/// game's crafting group of that name.
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
    /// A placed prop the game files under its Decorative crafting group,
    /// from the item tag behind that group: decorations and much of the
    /// furniture.
    Decorative,
    /// Anything else: equipment, functional props and materials among them.
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
        Self::Decorative,
    ];

    /// The kind's name in a data pack, such as `building-block`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::recipes::RecipeKind;
    ///
    /// assert_eq!(RecipeKind::BuildingBlock.name(), "building-block");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BuildingBlock => "building-block",
            Self::Roof => "roof",
            Self::Overgrowth => "overgrowth",
            Self::Terrain => "terrain",
            Self::Decorative => "decorative",
            Self::Other => "other",
        }
    }

    /// The kind [`RecipeKind::name`] calls `name`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::recipes::RecipeKind;
    ///
    /// assert_eq!(RecipeKind::named("roof"), Some(RecipeKind::Roof));
    /// ```
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.name() == name)
    }

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
            Self::Decorative => Abi::DECORATIVE,
            Self::Other => Abi::OTHER,
        }
    }

    /// The kind of the item `item`, as the loader answers it.
    pub(super) fn of(item: Guid) -> Option<Self> {
        let host = crate::host()?;
        let mut out = crate::abi::RecipeKind::OTHER;
        (host.api().recipe_kind)(
            host.handle(),
            crate::ItemId::new(item.fnv1a()),
            &raw mut out,
        )
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
            Abi::DECORATIVE => Self::Decorative,
            _ => Self::Other,
        }
    }
}

impl From<RecipeKind> for crate::abi::RecipeKind {
    fn from(kind: RecipeKind) -> Self {
        kind.code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_reads_back_from_its_name() {
        for kind in RecipeKind::ALL {
            assert_eq!(RecipeKind::named(kind.name()), Some(*kind));
        }
        assert_eq!(RecipeKind::named("Roof"), None);
    }

    #[test]
    fn every_kind_reads_back_from_its_code_and_each_code_is_its_own() {
        for (i, kind) in RecipeKind::ALL.iter().enumerate() {
            assert_eq!(RecipeKind::from(kind.code()), *kind);
            assert!(
                RecipeKind::ALL[i + 1..]
                    .iter()
                    .all(|k| k.code() != kind.code())
            );
        }
    }

    #[test]
    fn decorative_props_have_a_kind_of_their_own() {
        assert_eq!(
            RecipeKind::named("decorative"),
            Some(RecipeKind::Decorative)
        );
        assert_eq!(
            RecipeKind::Decorative.code(),
            crate::abi::RecipeKind::DECORATIVE
        );
    }
}
