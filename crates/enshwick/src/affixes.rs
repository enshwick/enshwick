//! Affixes: named bundles of stat modifiers with rolled values, such as
//! "of the Bear" giving 10 to 30 extra health, rolled onto single items as
//! they drop or are crafted and kept with the item.
//!
//! A mod defines its affixes with [`Load::define_affix`], each named by a
//! content id and by the text `<name>.name` in the mod's texts: the [`Modifier`](crate::stats::Modifier)s'
//! ranges, a weight against the other affixes, a [`ItemRarity`], the item
//! [`ItemCategoryMask`](crate::content::categories::ItemCategoryMask) it rolls on and an optional group whose affixes never roll
//! together. Every mod's affixes form one pool. What an item rolled is part
//! of its [`ItemStats`](crate::stats::ItemStats), so the entity that wears
//! it gets the modifiers, and [`Affixes::lines`] lists them for a tooltip.
//!
//! Rolling onto items, by hand with `Affixes::roll` or on loot and crafted
//! items with `Load::add_loot_affixes` and `Load::add_craft_affixes`, is
//! unstable: it needs the `unstable` feature and may change in any release,
//! since what an item rolled is kept as item data, which sets aside, after a
//! restart, a value on an item no load finds.
//!
//! [`roll`] is the same roll as a pure function: the same seed, item
//! category and pool give the same affixes.
//!
//! # Examples
//!
//! ```
//! use enshwick::affixes::{AffixDef, Range, ItemRarity};
//! use enshwick::content::categories::ItemCategoryMask;
//! use enshwick::content::id;
//! use enshwick::stats::{Op, builtin};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let health = load.open_stat(builtin::HEALTH_MAX)?;
//!     load.define_affix(
//!         id!("bear"),
//!         &AffixDef::new(ItemCategoryMask::EQUIPMENT)
//!             .with(Range::new(health, Op::Add, 10.0, 30.0).step(1.0))
//!             .rarity(ItemRarity::Uncommon),
//!     )?;
//!     Ok(())
//! }
//! ```

use crate::abi::{ItemLine, StatRef, Str};
use crate::content::Id;
use crate::inventory::ItemInstance;
use crate::stats::Op;
use crate::{Host, Load, Off};

/// What an affix is called and how rare it is, from [`Affixes::info`].
///
/// # Examples
///
/// See [`Affixes::info`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AffixInfo {
    /// The text key that names it in its owner's texts, see
    /// [`name_key`](crate::stats::name_key).
    pub text: String,
    /// How rare it is.
    pub rarity: ItemRarity,
}

/// Why an item did not roll.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::RollError;
///
/// assert_eq!(RollError::Rolled.to_string(), "the item rolled before");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RollError {
    /// A roll of an item that rolled before, or off the server.
    #[error("the item rolled before")]
    Rolled,
    /// Off the server's game thread.
    #[error("not on the server's game thread")]
    WrongThread,
    /// The item has no entity, or the loader refused for another reason.
    #[error("roll refused (status {0})")]
    Refused(u32),
}

/// The affix pool and rolls; get it from [`Load::affixes`]. Copy it
/// anywhere.
///
/// # Examples
///
/// See [`Affixes::lines`].
#[derive(Clone, Copy, Debug)]
pub struct Affixes {
    host: Host,
}

impl Affixes {
    /// What `item` gives, a line per modifier: its own first, then each
    /// affix's, for a tooltip. On a client, from what the server sent. Any
    /// thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn tooltip(affixes: &enshwick::affixes::Affixes, item: enshwick::inventory::ItemInstance) -> usize {
    ///     affixes.lines(item).iter().filter(|line| line.affix.is_some()).count()
    /// }
    /// ```
    #[must_use]
    pub fn lines(&self, item: ItemInstance) -> Vec<Line> {
        let mut out = vec![ItemLine::default(); 16];
        loop {
            let mut count = 0u32;
            let status = (self.host.api().item_lines)(
                self.host.handle(),
                item.pide,
                item.item,
                out.as_mut_ptr(),
                u32::try_from(out.len()).unwrap_or(u32::MAX),
                &raw mut count,
            );
            if !status.is_ok() {
                return Vec::new();
            }
            let count = count as usize;
            if count <= out.len() {
                out.truncate(count);
                break;
            }
            out = vec![ItemLine::default(); count];
        }
        out.iter()
            .filter_map(|l| {
                Some(Line {
                    stat: l.modifier.stat,
                    op: Op::from_raw(l.modifier.op)?,
                    value: l.modifier.value,
                    affix: l.affix.checked_sub(1).map(AffixId::new),
                })
            })
            .collect()
    }

    /// The text key and rarity of affix `affix`, an index from a
    /// [`Line`]. Any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn name(affixes: &enshwick::affixes::Affixes, line: &enshwick::affixes::Line) -> Option<String> {
    ///     affixes.info(line.affix?).map(|info| info.text)
    /// }
    /// ```
    #[must_use]
    pub fn info(&self, affix: AffixId) -> Option<AffixInfo> {
        let mut text = Str::EMPTY;
        let mut rarity = 0u8;
        let status = (self.host.api().affix_info)(
            self.host.handle(),
            affix.get(),
            &raw mut text,
            &raw mut rarity,
        );
        if !status.is_ok() {
            return None;
        }
        // SAFETY: `affix_info` returned OK, and the loader keeps its affixes'
        // texts alive and unchanged for the process; the text is copied.
        let text = unsafe { text.as_str() }?.to_owned();
        Some(AffixInfo {
            text,
            rarity: ItemRarity::from_raw(rarity)?,
        })
    }
}

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
    /// See the [module](self).
    pub fn define_affix(&mut self, id: Id, def: &AffixDef) -> Result<AffixId, Off> {
        let full = id.to_string();
        def.check()
            .map_err(|why| Off::new(format!("affix {full:?}: {why}")))?;
        let host = self.host();
        let ranges = def.to_ranges();
        let data = def.to_data(&ranges);
        let mut out = StatRef::default();
        let status = (host.api().affix_open)(
            host.handle(),
            Str::new(&full),
            &raw const data,
            &raw mut out,
        );
        crate::load::ok(status, out.index, || format!("affix {full:?}")).map(AffixId::new)
    }

    /// The affix pool and rolls. Call it during `init`.
    ///
    /// # Examples
    ///
    /// See [`Affixes::lines`].
    #[must_use]
    pub fn affixes(&mut self) -> Affixes {
        Affixes { host: self.host() }
    }
}

mod defs;
mod roll;
#[cfg(feature = "unstable")]
mod rolling;
#[cfg(test)]
mod tests;

pub use defs::*;
pub use roll::*;
#[cfg(feature = "unstable")]
pub use rolling::{AffixCraft, AffixLoot};
