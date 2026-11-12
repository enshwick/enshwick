//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::affixes::{AffixDef, AffixId, Affixes};
use crate::content::Id;
use crate::{Load, Off};
use enshwick_inventory::__load::*;

#[cfg(feature = "unstable")]
mod rolling;

#[enshwick_macros::forward(base)]
#[crate::script(affix_defs)]
impl Load<'_> {
    /// Defines the mod's affix `id`, named with [`id!`](crate::content::id).
    /// Its text is `<name>.name` in the mod's texts
    /// ([`name_key`](crate::stats::name_key)). Call it during `init`, on both
    /// sides, so clients name what items rolled.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of another mod, a malformed definition, an affix
    /// defined twice, or
    /// after init.
    ///
    /// # Examples
    ///
    /// See the [module](crate::affixes).
    #[script]
    pub fn define_affix(&mut self, id: Id, def: &AffixDef) -> Result<AffixId, Off>;

    /// The affix pool and rolls. Call it during `init`.
    ///
    /// # Examples
    ///
    /// See [`Affixes::lines`].
    #[must_use]
    pub fn affixes(&mut self) -> Affixes;
}
