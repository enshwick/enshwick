//! Affixes: definitions mods give during init, the pool items roll from,
//! and the lines an item's rolled affixes give.

use enshwick::Entity;
use enshwick::abi::{AffixDefData, AffixRoll, ItemLine, StatRef, Status, Str};
use enshwick::affixes::{AffixDef, AffixKey, Candidate, ItemRarity, RollRules, lines_of, reroll};
use enshwick::stats::{ItemStats, Stat, key_of};

use super::{Asked, SEALED, Sealed, declare, full_id, pending, wanted};
use crate::host::Loader;
use crate::ids::Space;
use crate::mod_index::ModIndex;

/// One affix.
#[derive(Debug)]
pub(super) struct Affix {
    /// The full id.
    id: Box<str>,
    /// The text key that names it, see [`name_key`](enshwick::stats::name_key).
    text: Box<str>,
    def: AffixDef,
}

/// Defines mod `owner`'s affix `id`. During init only.
pub(crate) fn open_affix(
    loader: &Loader,
    owner: ModIndex,
    id: &str,
    data: &AffixDefData,
) -> Result<StatRef, Status> {
    let record = loader.mods.get(owner.get()).ok_or(Status::BAD_ARGS)?;
    let Asked { full, text, own } = full_id(&record.id, id).ok_or(Status::BAD_ARGS)?;
    if !own {
        return Err(Status::REFUSED);
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
        let def = unsafe { AffixDef::from_data(data, stat) }.ok_or(Status::BAD_ARGS)?;
        if p.affixes.iter().any(|known| *known.id == *full) {
            return Err(Status::REFUSED);
        }
        p.affixes.push(Affix {
            id: full.clone().into_boxed_str(),
            text,
            def,
        });
        Ok(StatRef {
            index: u32::try_from(p.affixes.len() - 1).map_err(|_| Status::REFUSED)?,
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

/// Rolls an item anew and keeps the result as its item stats; how many
/// affixes it rolled. On the server's game thread.
pub(crate) fn roll(loader: &Loader, roll: &AffixRoll) -> Result<u32, Status> {
    let sealed = SEALED.get().filter(|_| wanted()).ok_or(Status::MISSING)?;
    crate::world::server()?;
    if roll.pide == Entity(0) || roll.item == enshwick::ItemId::new(0) {
        return Err(Status::BAD_ARGS);
    }
    let category = crate::inventory::item_info(loader, roll.item.get())?.category;
    let current = crate::item_data::get_platform(roll.pide.0, roll.item.get())
        .and_then(|bytes| postcard::from_bytes::<ItemStats>(&bytes).ok())
        .unwrap_or_default();
    let rules = RollRules::new(roll.min, roll.max)
        .max_rarity(ItemRarity::from_raw(roll.max_rarity).unwrap_or(ItemRarity::Legendary));
    let once = roll.flags & AffixRoll::ONCE != 0;
    let next =
        reroll(&current, &pool(sealed), category, rules, roll.seed, once).ok_or(Status::REFUSED)?;
    if next == current {
        return Ok(0);
    }
    let bytes = postcard::to_allocvec(&next).map_err(|_| Status::REFUSED)?;
    crate::item_data::set_platform(roll.pide.0, roll.item.get(), &bytes)?;
    Ok(u32::try_from(next.affixes.len()).unwrap_or(0))
}

/// What the item `pide` holding `item` gives, a line per modifier.
pub(crate) fn item_lines(pide: u32, item: u32) -> Result<Vec<ItemLine>, Status> {
    let sealed = SEALED.get().filter(|_| wanted()).ok_or(Status::MISSING)?;
    let record = crate::item_data::get_platform(pide, item)
        .and_then(|bytes| postcard::from_bytes::<ItemStats>(&bytes).ok())
        .unwrap_or_default();
    Ok(lines_of(&record, &pool(sealed), |key| {
        sealed.by_key.get(&key.0).copied()
    }))
}

/// The text key and rarity of affix `affix`.
pub(crate) fn affix_info(affix: u32) -> Result<(Str, u8), Status> {
    let sealed = SEALED.get().ok_or(Status::MISSING)?;
    let affix = sealed
        .affixes
        .get(usize::try_from(affix).map_err(|_| Status::BAD_ARGS)?)
        .ok_or(Status::MISSING)?;
    // The affixes live as long as the process, so their text does too.
    Ok((Str::new(&affix.text), affix.def.rarity.to_raw()))
}
