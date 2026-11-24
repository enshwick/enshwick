//! Affixes: definitions mods give during init, the pool items roll from,
//! and the lines an item's rolled affixes give.

use enshwick_core::Entity;
use enshwick_core::abi::{AffixRoll, AffixSpec, ItemLine, StatRef, Str};
use enshwick_core::affixes::{
    AffixDef, AffixKey, Candidate, ItemRarity, RollRules, lines_of, reroll,
};
use enshwick_core::stats::{ItemStats, Stat, key_of};

use super::{Asked, SEALED, Sealed, StatsError, declare, full_id, pending, wanted};
use crate::base::ids::Space;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;

/// One affix.
#[derive(Debug)]
pub(super) struct Affix {
    /// The full id.
    id: Box<str>,
    /// The text key that names it, see [`name_key`](enshwick_core::stats::name_key).
    text: Box<str>,
    def: AffixDef,
}

/// Defines mod `owner`'s affix `id`. During init only.
pub(crate) fn open_affix(
    loader: &Loader,
    owner: ModIndex,
    id: &str,
    data: &AffixSpec,
) -> Result<StatRef, StatsError> {
    let record = loader.mods.get(owner.get()).ok_or(StatsError::NoMod)?;
    let Asked { full, text, own } = full_id(&record.id, id).ok_or(StatsError::BadId)?;
    if !own {
        return Err(StatsError::NotOwn);
    }
    declare(loader, owner, Space::Affix, &full)?;
    pending(|p| {
        let kinds = &p.kinds;
        let stat = |index: u32| {
            let kind = kinds.get(index as usize)?;
            Some(Stat::from_ref(StatRef {
                index,
                key: key_of(&kind.id).0,
            }))
        };
        // SAFETY: the caller passes the definition's text and ranges for
        // the call.
        let def = unsafe { AffixDef::from_data(data, stat) }.ok_or(StatsError::BadDef)?;
        if p.affixes.iter().any(|known| *known.id == *full) {
            return Err(StatsError::Defined);
        }
        p.affixes.push(Affix {
            id: full.clone().into_boxed_str(),
            text,
            def,
        });
        Ok(StatRef {
            index: u32::try_from(p.affixes.len() - 1).map_err(|_| StatsError::Full)?,
            key: AffixKey::of(&full).0,
        })
    })?
}

/// The affix pool, in the order the affixes were defined.
pub(super) fn pool(sealed: &Sealed) -> Vec<Candidate<'_>> {
    sealed
        .affixes
        .iter()
        .map(|affix| Candidate {
            key: AffixKey::of(&affix.id),
            def: &affix.def,
        })
        .collect()
}

/// Why an item was not rolled anew.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RollAffixesError {
    /// No mod opened affixes, so the item stats do not run.
    #[error("affixes are not open")]
    Unopened,
    /// No item, or no entity holding it.
    #[error("no item to roll")]
    NoItem,
    /// The rules leave no roll for the item.
    #[error("no roll fits the rules")]
    NoRoll,
    /// The rolled stats did not encode.
    #[error("the rolled stats did not encode")]
    Encode,
    /// The item stats were not kept.
    #[error(transparent)]
    Kept(#[from] crate::inventory::item_data::UseItemDataError),
    /// The server's world, unreadable now.
    #[error(transparent)]
    World(#[from] crate::engine::world::WorldError),
    /// The item's static data, unreadable now.
    #[error(transparent)]
    Item(#[from] crate::content::item_info::ItemInfoError),
}

/// Rolls an item anew and keeps the result as its item stats; how many
/// affixes it rolled. On the server's game thread.
///
/// # Errors
///
/// [`RollAffixesError`] while affixes are not open, for no item, a roll
/// the rules leave none of, stats not kept, or the world refusing a read.
pub(crate) fn roll(loader: &Loader, roll: &AffixRoll) -> Result<u32, RollAffixesError> {
    let sealed = SEALED
        .get()
        .filter(|_| wanted())
        .ok_or(RollAffixesError::Unopened)?;
    crate::engine::world::server()?;
    if roll.pide == Entity(0) || roll.item == enshwick_core::ItemId::new(0) {
        return Err(RollAffixesError::NoItem);
    }
    let category = crate::inventory::transactions::item_info(loader, roll.item.get())?.category;
    let current = crate::inventory::item_data::get_platform(roll.pide.0, roll.item.get())
        .and_then(|bytes| postcard::from_bytes::<ItemStats>(&bytes).ok())
        .unwrap_or_default();
    let rules = RollRules::new(roll.min, roll.max)
        .max_rarity(ItemRarity::from_raw(roll.max_rarity).unwrap_or(ItemRarity::Legendary));
    let once = roll.flags & AffixRoll::ONCE != 0;
    let next = reroll(&current, &pool(sealed), category, rules, roll.seed, once)
        .ok_or(RollAffixesError::NoRoll)?;
    if next == current {
        return Ok(0);
    }
    let bytes = postcard::to_allocvec(&next).map_err(|_| RollAffixesError::Encode)?;
    crate::inventory::item_data::set_platform(roll.pide.0, roll.item.get(), &bytes)?;
    Ok(u32::try_from(next.affixes.len()).unwrap_or(0))
}

/// What the item `pide` holding `item` gives, a line per modifier.
pub(crate) fn item_lines(pide: u32, item: u32) -> Result<Vec<ItemLine>, StatsError> {
    let sealed = SEALED
        .get()
        .filter(|_| wanted())
        .ok_or(StatsError::NotRunning)?;
    let record = crate::inventory::item_data::get_platform(pide, item)
        .and_then(|bytes| postcard::from_bytes::<ItemStats>(&bytes).ok())
        .unwrap_or_default();
    Ok(lines_of(&record, &pool(sealed), |key| {
        sealed.by_key.get(&key.0).copied()
    }))
}

/// The text key and rarity of affix `affix`.
pub(crate) fn affix_info(affix: u32) -> Result<(Str, u8), StatsError> {
    let sealed = SEALED.get().ok_or(StatsError::NotRunning)?;
    let affix = usize::try_from(affix)
        .ok()
        .and_then(|affix| sealed.affixes.get(affix))
        .ok_or(StatsError::NoAffix)?;
    // The affixes live as long as the process, so their text does too.
    Ok((Str::new(&affix.text), affix.def.rarity.to_raw()))
}
