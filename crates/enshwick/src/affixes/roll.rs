use std::collections::HashSet;

use crate::abi::{ItemLine, StatModifierData};
use crate::stats::{ItemStats, Op, StatKey};

use super::defs::{AffixDef, AffixKey, ItemRarity};

/// An affix an item rolled: its key and each modifier's value, in the order
/// of the affix's ranges.
///
/// # Examples
///
/// See [`roll`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rolled {
    /// The affix.
    pub affix: AffixKey,
    /// The values.
    pub values: Vec<f32>,
}

/// One affix of the pool [`roll`] draws from.
///
/// # Examples
///
/// See [`roll`].
#[derive(Clone, Copy, Debug)]
pub struct Candidate<'a> {
    /// The affix's key.
    pub key: AffixKey,
    /// Its definition.
    pub def: &'a AffixDef,
}

/// What a roll may give, for [`roll`] and [`Affixes::roll`](crate::affixes::Affixes::roll).
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{ItemRarity, RollRules};
///
/// let rules = RollRules::new(1, 3).max_rarity(ItemRarity::Rare);
/// assert_eq!((rules.min, rules.max), (1, 3));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct RollRules {
    /// The fewest affixes.
    pub min: u8,
    /// The most affixes.
    pub max: u8,
    /// The rarest an affix may be.
    pub max_rarity: ItemRarity,
}

impl RollRules {
    /// `min` to `max` affixes of any rarity.
    ///
    /// # Examples
    ///
    /// See [`RollRules`].
    #[must_use]
    pub const fn new(min: u8, max: u8) -> Self {
        Self {
            min,
            max,
            max_rarity: ItemRarity::Legendary,
        }
    }

    /// The same, no rarer than `rarity`.
    ///
    /// # Examples
    ///
    /// See [`RollRules`].
    #[must_use]
    pub const fn max_rarity(mut self, rarity: ItemRarity) -> Self {
        self.max_rarity = rarity;
        self
    }
}

/// The affixes an item of the game's `ItemCategory` numbered `category` rolls
/// from `pool` under `rules`, from `seed`.
///
/// The count comes first, then each affix by weight among those that fit, are
/// not taken and share no group with one taken, then its values.
///
/// The pool's order does not matter.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{AffixDef, AffixKey, Candidate, RollRules, roll};
/// use enshwick::content::categories::ItemCategoryMask;
///
/// let ring = AffixDef::new("ring", ItemCategoryMask::EQUIPMENT);
/// let pool = [Candidate { key: AffixKey(1), def: &ring }];
/// // An affix without ranges rolls no values.
/// let first = roll(&pool, 1, RollRules::new(1, 1), 42);
/// assert_eq!(first, roll(&pool, 1, RollRules::new(1, 1), 42));
/// assert_eq!(first.len(), 1);
/// assert!(roll(&pool, 2, RollRules::new(1, 1), 42).is_empty(), "weapons do not fit");
/// ```
#[must_use]
pub fn roll(pool: &[Candidate<'_>], category: u8, rules: RollRules, seed: u64) -> Vec<Rolled> {
    let mut rng = fastrand::Rng::with_seed(seed);
    let mut fitting: Vec<&Candidate<'_>> = pool
        .iter()
        .filter(|c| {
            c.def.weight > 0 && c.def.categories.holds(category) && c.def.rarity <= rules.max_rarity
        })
        .collect();
    fitting.sort_by_key(|c| c.key);
    fitting.dedup_by_key(|c| c.key);
    let (low, high) = (rules.min.min(rules.max), rules.max.max(rules.min));
    let count = rng.u8(low..=high);
    let mut groups: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for _ in 0..count {
        fitting.retain(|c| c.def.group.as_deref().is_none_or(|g| !groups.contains(g)));
        let total: u64 = fitting.iter().map(|c| u64::from(c.def.weight)).sum();
        if total == 0 {
            break;
        }
        let mut pick = rng.u64(0..total);
        let Some(at) = fitting.iter().position(|c| {
            let weight = u64::from(c.def.weight);
            if pick < weight {
                true
            } else {
                pick -= weight;
                false
            }
        }) else {
            break;
        };
        let chosen = fitting.remove(at);
        if let Some(group) = chosen.def.group.as_deref() {
            groups.insert(group);
        }
        out.push(Rolled {
            affix: chosen.key,
            values: chosen.def.ranges.iter().map(|r| r.at(rng.f32())).collect(),
        });
    }
    out
}

/// `stats` rolled anew from `pool` for an item of the game's `ItemCategory`
/// numbered `category`, as [`roll`] rolls; `None` when `once` and it rolled
/// before.
///
/// An item no affix of the pool fits comes back as it was, not marked as
/// rolled.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{AffixDef, AffixKey, Candidate, RollRules, reroll};
/// use enshwick::content::categories::ItemCategoryMask;
/// use enshwick::stats::ItemStats;
///
/// let ring = AffixDef::new("ring", ItemCategoryMask::EQUIPMENT);
/// let pool = [Candidate { key: AffixKey(1), def: &ring }];
/// let rolled = reroll(&ItemStats::default(), &pool, 1, RollRules::new(1, 1), 7, true).unwrap();
/// assert!(rolled.rolled);
/// assert!(reroll(&rolled, &pool, 1, RollRules::new(1, 1), 7, true).is_none(), "once");
/// assert!(!reroll(&ItemStats::default(), &pool, 2, RollRules::new(1, 1), 7, true).unwrap().rolled);
/// ```
#[must_use]
pub fn reroll(
    stats: &ItemStats,
    pool: &[Candidate<'_>],
    category: u8,
    rules: RollRules,
    seed: u64,
    once: bool,
) -> Option<ItemStats> {
    if once && stats.rolled {
        return None;
    }
    let mut next = stats.clone();
    if pool.iter().any(|c| c.def.categories.holds(category)) {
        next.affixes = roll(pool, category, rules, seed);
        next.rolled = true;
    }
    Some(next)
}

/// What `stats` gives, a modifier per line, its own first and then each
/// affix's: affixes by their place in `pool` plus one, stats by the index
/// `stat` gives their key.
///
/// Modifiers of unknown stats or affixes are left out.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{lines_of, Candidate};
/// use enshwick::stats::{ItemModifier, ItemStats, Op, StatKey};
///
/// let stats = ItemStats { modifiers: vec![ItemModifier { stat: StatKey(9), op: Op::Add, value: 2.0 }], ..ItemStats::default() };
/// let lines = lines_of(&stats, &[], |key| (key == StatKey(9)).then_some(0));
/// assert_eq!(lines.len(), 1);
/// assert_eq!(lines[0].affix, 0);
/// ```
#[must_use]
pub fn lines_of(
    stats: &ItemStats,
    pool: &[Candidate<'_>],
    stat: impl Fn(StatKey) -> Option<u32>,
) -> Vec<ItemLine> {
    let line = |stat: u32, op: Op, value: f32, affix: u32| ItemLine {
        modifier: StatModifierData {
            stat,
            op: op.to_raw(),
            reserved: [0; 3],
            value,
        },
        affix,
    };
    let mut out: Vec<ItemLine> = stats
        .modifiers
        .iter()
        .filter_map(|m| Some(line(stat(m.stat)?, m.op, m.value, 0)))
        .collect();
    for rolled in &stats.affixes {
        let Some((at, candidate)) = pool.iter().enumerate().find(|(_, c)| c.key == rolled.affix)
        else {
            continue;
        };
        let affix = u32::try_from(at + 1).unwrap_or(u32::MAX);
        for (range, &value) in candidate.def.ranges.iter().zip(&rolled.values) {
            out.push(line(range.stat.index(), range.op, value, affix));
        }
    }
    out
}

/// A line of what an item gives, from [`Affixes::lines`](crate::affixes::Affixes::lines).
///
/// # Examples
///
/// See [`Affixes::lines`](crate::affixes::Affixes::lines).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Line {
    /// The stat's index in this process (see [`Stat::index`](crate::stats::Stat::index)); its text and
    /// format come from [`Stats::info`](crate::stats::Stats::info).
    pub stat: u32,
    /// What it does.
    pub op: Op,
    /// Its value.
    pub value: f32,
    /// The affix it comes from, by index (see [`Affixes::info`](crate::affixes::Affixes::info)); `None` for
    /// the item's own modifiers.
    pub affix: Option<u32>,
}
