//! Affixes: named bundles of stat modifiers with rolled values, such as
//! "of the Bear" giving 10 to 30 extra health, rolled onto single items as
//! they drop or are crafted and kept with the item.
//!
//! A mod defines its affixes with [`Load::define_affix`]: the [`Modifier`](crate::stats::Modifier)s'
//! ranges, a weight against the other affixes, a [`Rarity`], the item
//! [`Categories`] it rolls on and an optional group whose affixes never roll
//! together. Every mod's affixes form one pool. [`Affixes::roll`] rolls an
//! item from it on the server; [`Load::add_loot_affixes`] and
//! [`Load::add_craft_affixes`] roll the items that drop from random loot or
//! come out of crafting. What an item rolled is part of its
//! [`ItemStats`], so the entity that wears it gets
//! the modifiers, and [`Affixes::lines`] lists them for a tooltip.
//!
//! [`roll`] is the same roll as a pure function: the same seed, item
//! category and pool give the same affixes.
//!
//! # Examples
//!
//! ```
//! use enshwick::affixes::{AffixDef, AffixLoot, Categories, Range, Rarity};
//! use enshwick::loot::Source;
//! use enshwick::stats::{Op, builtin};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let health = load.open_stat(builtin::HEALTH_MAX)?;
//!     load.define_affix(
//!         "bear",
//!         &AffixDef::new("affix.bear", Categories::EQUIPMENT)
//!             .with(Range::new(health, Op::Add, 10.0, 30.0).step(1.0))
//!             .rarity(Rarity::Uncommon),
//!     )?;
//!     load.add_loot_affixes(AffixLoot::new(Source::bosses(), 0.5).count(1, 2))
//! }
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::abi::{
    AffixDefData, AffixModifierData, AffixRoll, Header, ItemLine, StatModifierData, StatRef,
    Status, Str,
};
use crate::ecs::Entity;
use crate::game_events::CraftRequested;
use crate::inventory::Slot;
use crate::item_data::ItemInstance;
use crate::locks::Relock;
use crate::loot::Source;
use crate::stats::{ItemStats, Op, Stat, StatKey};
use crate::{Host, Load, Off};

/// How rare an affix is.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::Rarity;
///
/// assert!(Rarity::Common < Rarity::Legendary);
/// assert_eq!(Rarity::from_raw(2), Some(Rarity::Rare));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Rarity {
    /// Common.
    #[default]
    Common,
    /// Uncommon.
    Uncommon,
    /// Rare.
    Rare,
    /// Epic.
    Epic,
    /// Legendary.
    Legendary,
}

impl Rarity {
    /// The rarity's number, 0 to 4.
    ///
    /// # Examples
    ///
    /// See [`Rarity`].
    #[must_use]
    pub const fn to_raw(self) -> u8 {
        self as u8
    }

    /// The rarity numbered `raw`.
    ///
    /// # Examples
    ///
    /// See [`Rarity`].
    #[must_use]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Common),
            1 => Some(Self::Uncommon),
            2 => Some(Self::Rare),
            3 => Some(Self::Epic),
            4 => Some(Self::Legendary),
            _ => None,
        }
    }
}

/// Item categories, a bit per the game's `ItemCategory`.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::Categories;
///
/// let gear = Categories::EQUIPMENT | Categories::WEAPONS;
/// assert!(gear.contains(Categories::WEAPONS));
/// assert!(gear.holds(2));
/// assert!(!gear.holds(3));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Categories(pub u32);

impl Categories {
    /// Armour and rings, the game's `Equipment`.
    pub const EQUIPMENT: Self = Self(1 << 1);
    /// Weapons.
    pub const WEAPONS: Self = Self(1 << 2);
    /// Tools.
    pub const TOOLS: Self = Self(1 << 3);
    /// Instruments.
    pub const INSTRUMENTS: Self = Self(1 << 4);
    /// Fishing rods.
    pub const FISHING_RODS: Self = Self(1 << 5);

    /// Whether every category of `other` is here.
    ///
    /// # Examples
    ///
    /// See [`Categories`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the game's `ItemCategory` numbered `category` is here.
    ///
    /// # Examples
    ///
    /// See [`Categories`].
    #[must_use]
    pub const fn holds(self, category: u8) -> bool {
        category < 32 && self.0 & (1 << category) != 0
    }
}

impl core::ops::BitOr for Categories {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// One modifier of an affix with the range its value rolls in.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::Range;
/// use enshwick::stats::Op;
///
/// fn fire(stat: enshwick::stats::Stat) -> Range {
///     Range::new(stat, Op::Add, 0.05, 0.15).step(0.01)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    /// The stat.
    pub stat: Stat,
    /// What it does.
    pub op: Op,
    /// The least value.
    pub min: f32,
    /// The greatest value.
    pub max: f32,
    /// The value rolls in steps of this from `min`; 0 for any value.
    pub step: f32,
}

impl Range {
    /// `op` on `stat` with a value from `min` to `max`.
    ///
    /// # Examples
    ///
    /// See [`Range`].
    #[must_use]
    pub const fn new(stat: Stat, op: Op, min: f32, max: f32) -> Self {
        Self {
            stat,
            op,
            min,
            max,
            step: 0.0,
        }
    }

    /// The same, rolling in steps of `step`.
    ///
    /// # Examples
    ///
    /// See [`Range`].
    #[must_use]
    pub const fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    /// The value at `unit`, from 0 to 1.
    fn at(&self, unit: f32) -> f32 {
        let raw = self.min + unit * (self.max - self.min);
        let value = if self.step > 0.0 {
            self.min + ((raw - self.min) / self.step).round() * self.step
        } else {
            raw
        };
        value.clamp(self.min.min(self.max), self.max.max(self.min))
    }
}

/// An affix's definition, for [`Load::define_affix`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AffixDef {
    /// The text key that names the affix, in the mod's texts.
    pub text: String,
    /// The modifiers and their ranges.
    pub ranges: Vec<Range>,
    /// How likely it rolls against the others, 100 by default.
    pub weight: u32,
    /// How rare it is.
    pub rarity: Rarity,
    /// The item categories it rolls on.
    pub categories: Categories,
    /// Affixes of one group never roll together on an item.
    pub group: Option<String>,
}

impl AffixDef {
    /// An affix named by text key `text` that rolls on `categories`, with
    /// no modifiers yet.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn new(text: &str, categories: Categories) -> Self {
        Self {
            text: text.to_owned(),
            ranges: Vec::new(),
            weight: 100,
            rarity: Rarity::Common,
            categories,
            group: None,
        }
    }

    /// The same with `range` added.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn with(mut self, range: Range) -> Self {
        self.ranges.push(range);
        self
    }

    /// The same with weight `weight`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::{AffixDef, Categories};
    ///
    /// assert_eq!(AffixDef::new("a", Categories::WEAPONS).weight(5).weight, 5);
    /// ```
    #[must_use]
    pub const fn weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// The same with rarity `rarity`.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn rarity(mut self, rarity: Rarity) -> Self {
        self.rarity = rarity;
        self
    }

    /// The same in group `group`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::{AffixDef, Categories};
    ///
    /// let def = AffixDef::new("a", Categories::WEAPONS).group("element");
    /// assert_eq!(def.group.as_deref(), Some("element"));
    /// ```
    #[must_use]
    pub fn group(mut self, group: &str) -> Self {
        self.group = Some(group.to_owned());
        self
    }

    /// Why the loader would refuse it, if it would.
    fn check(&self) -> Result<(), String> {
        if self.ranges.is_empty() || self.weight == 0 || self.categories.0 == 0 {
            return Err("needs a modifier, a weight and a category".to_owned());
        }
        let bad = self.ranges.iter().any(|r| {
            !(r.min.is_finite() && r.max.is_finite() && r.step.is_finite())
                || r.min > r.max
                || r.step < 0.0
        });
        if bad {
            return Err("a range is empty or not finite".to_owned());
        }
        Ok(())
    }

    /// The definition as the loader takes it, borrowing the text and
    /// `ranges`, which the caller keeps alive for the call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::{AffixDef, Categories};
    ///
    /// let def = AffixDef::new("a", Categories::WEAPONS);
    /// let ranges = def.to_ranges();
    /// assert_eq!(def.to_data(&ranges).modifier_count, 0);
    /// ```
    #[must_use]
    pub fn to_data(&self, ranges: &[AffixModifierData]) -> AffixDefData {
        AffixDefData {
            header: Header::of::<AffixDefData>(),
            text: Str::new(&self.text),
            group: self.group.as_deref().map_or(Str::EMPTY, Str::new),
            modifiers: ranges.as_ptr(),
            modifier_count: u32::try_from(ranges.len()).unwrap_or(u32::MAX),
            weight: self.weight,
            categories: self.categories.0,
            rarity: self.rarity.to_raw(),
            reserved: [0; 3],
        }
    }

    /// The definition the loader took, its stats named by `stat`; `None`
    /// for one that does not read or names a stat `stat` does not know.
    ///
    /// # Safety
    ///
    /// - `data.text` and `data.group` are null or point at their `len` bytes,
    ///   alive and unchanged for the call.
    /// - `data.modifiers` is null or points at `modifier_count` initialised
    ///   ranges, aligned and alive for the call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::StatRef;
    /// use enshwick::affixes::{AffixDef, Categories, Range};
    /// use enshwick::stats::{Op, Stat};
    ///
    /// let health = Stat::from_ref(StatRef { index: 0, key: 9 });
    /// let def = AffixDef::new("bear", Categories::EQUIPMENT).with(Range::new(health, Op::Add, 1.0, 2.0));
    /// let ranges = def.to_ranges();
    /// let data = def.to_data(&ranges);
    /// // SAFETY: `data` points into `def` and `ranges`, both alive and unchanged here.
    /// let back = unsafe { AffixDef::from_data(&data, |i| (i == 0).then_some(health)) };
    /// assert_eq!(back, Some(def));
    /// ```
    #[must_use]
    pub unsafe fn from_data(
        data: &AffixDefData,
        stat: impl Fn(u32) -> Option<Stat>,
    ) -> Option<Self> {
        // SAFETY: the caller keeps the text's bytes alive and unchanged for the
        // call, and the text is copied at once.
        let text = unsafe { data.text.as_str() }?.to_owned();
        // SAFETY: the caller keeps the group's bytes alive and unchanged for
        // the call, and `group` is copied before it returns.
        let group = unsafe { data.group.as_str() }?;
        let ranges = if data.modifier_count == 0 {
            &[][..]
        } else if data.modifiers.is_null() {
            return None;
        } else {
            // SAFETY: `modifiers` is not null in this branch, and the caller
            // guarantees `modifier_count` aligned, initialised ranges there,
            // alive for the call.
            unsafe { core::slice::from_raw_parts(data.modifiers, data.modifier_count as usize) }
        };
        let def = Self {
            text,
            ranges: ranges
                .iter()
                .map(|r| {
                    Some(Range {
                        stat: stat(r.stat)?,
                        op: Op::from_raw(r.op)?,
                        min: r.min,
                        max: r.max,
                        step: r.step,
                    })
                })
                .collect::<Option<_>>()?,
            weight: data.weight,
            rarity: Rarity::from_raw(data.rarity)?,
            categories: Categories(data.categories),
            group: (!group.is_empty()).then(|| group.to_owned()),
        };
        def.check().is_ok().then_some(def)
    }

    /// The ranges as the loader takes them.
    ///
    /// # Examples
    ///
    /// See [`AffixDef::to_data`].
    #[must_use]
    pub fn to_ranges(&self) -> Vec<AffixModifierData> {
        self.ranges
            .iter()
            .map(|r| AffixModifierData {
                stat: r.stat.index(),
                op: r.op.to_raw(),
                reserved: [0; 3],
                min: r.min,
                max: r.max,
                step: r.step,
            })
            .collect()
    }
}

/// The stored name of an affix: the FNV-1a 32 hash of its full id.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::AffixKey;
///
/// assert_eq!(AffixKey::of("rpg:bear"), AffixKey::of("rpg:bear"));
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
pub struct AffixKey(pub u32);

impl AffixKey {
    /// The key of the affix with full id `id`.
    ///
    /// # Examples
    ///
    /// See [`AffixKey`].
    #[must_use]
    pub const fn of(id: &str) -> Self {
        Self(crate::stats::key_of(id).0)
    }
}

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

/// What a roll may give, for [`roll`] and [`Affixes::roll`].
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{Rarity, RollRules};
///
/// let rules = RollRules::new(1, 3).max_rarity(Rarity::Rare);
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
    pub max_rarity: Rarity,
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
            max_rarity: Rarity::Legendary,
        }
    }

    /// The same, no rarer than `rarity`.
    ///
    /// # Examples
    ///
    /// See [`RollRules`].
    #[must_use]
    pub const fn max_rarity(mut self, rarity: Rarity) -> Self {
        self.max_rarity = rarity;
        self
    }
}

/// The affixes an item of the game's `ItemCategory` numbered `category`
/// rolls from `pool` under `rules`, from `seed`: the count first, then each
/// affix by weight among those that fit, are not taken and share no group
/// with one taken, then its values. The pool's order does not matter.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{AffixDef, AffixKey, Candidate, Categories, RollRules, roll};
///
/// let ring = AffixDef::new("ring", Categories::EQUIPMENT);
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
/// before. An item no affix of the pool fits comes back as it was, not
/// marked as rolled.
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{AffixDef, AffixKey, Candidate, Categories, RollRules, reroll};
/// use enshwick::stats::ItemStats;
///
/// let ring = AffixDef::new("ring", Categories::EQUIPMENT);
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
/// `stat` gives their key. Modifiers of unknown stats or affixes are left
/// out.
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

/// A line of what an item gives, from [`Affixes::lines`].
///
/// # Examples
///
/// See [`Affixes::lines`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Line {
    /// The stat's index in this process (see [`Stat::index`]); its text and
    /// format come from [`Stats::info`](crate::stats::Stats::info).
    pub stat: u32,
    /// What it does.
    pub op: Op,
    /// Its value.
    pub value: f32,
    /// The affix it comes from, by index (see [`Affixes::info`]); `None` for
    /// the item's own modifiers.
    pub affix: Option<u32>,
}

/// What an affix is called and how rare it is, from [`Affixes::info`].
///
/// # Examples
///
/// See [`Affixes::info`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AffixInfo {
    /// The text key that names it.
    pub text: String,
    /// How rare it is.
    pub rarity: Rarity,
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
/// See [`Affixes::roll`].
#[derive(Clone, Copy, Debug)]
pub struct Affixes {
    host: Host,
}

/// A seed from the item and the time, for rolls that need no repeating.
fn fresh_seed(item: ItemInstance) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    #[expect(clippy::cast_possible_truncation, reason = "the low bits seed")]
    let now = now as u64;
    now ^ (u64::from(item.pide.0) << 32 | u64::from(item.item))
}

impl Affixes {
    /// Rolls `item` from the pool of every mod's affixes that fit its
    /// category, replacing the affixes it had, from `seed` or a fresh one;
    /// `once` rolls only an item that never rolled. How many it rolled. On
    /// the server's game thread.
    ///
    /// # Errors
    ///
    /// [`RollError`] for an item that rolled before under `once`, off the
    /// server's game thread, or for an item without an entity.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::RollRules;
    ///
    /// fn reroll(affixes: &enshwick::affixes::Affixes, item: enshwick::item_data::ItemInstance) {
    ///     let _rolled = affixes.roll(item, RollRules::new(1, 3), None, false);
    /// }
    /// ```
    pub fn roll(
        &self,
        item: ItemInstance,
        rules: RollRules,
        seed: Option<u64>,
        once: bool,
    ) -> Result<u32, RollError> {
        let request = AffixRoll {
            header: Header::of::<AffixRoll>(),
            pide: item.pide.0,
            item: item.item,
            min: rules.min,
            max: rules.max,
            max_rarity: rules.max_rarity.to_raw(),
            flags: if once { AffixRoll::ONCE } else { 0 },
            seed: seed.unwrap_or_else(|| fresh_seed(item)),
        };
        let mut count = 0;
        match (self.host.api().affix_roll)(self.host.handle(), &raw const request, &raw mut count) {
            Status::OK => Ok(count),
            Status::REFUSED => Err(RollError::Rolled),
            Status::WRONG_THREAD => Err(RollError::WrongThread),
            other => Err(RollError::Refused(other.0)),
        }
    }

    /// What `item` gives, a line per modifier: its own first, then each
    /// affix's, for a tooltip. On a client, from what the server sent. Any
    /// thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn tooltip(affixes: &enshwick::affixes::Affixes, item: enshwick::item_data::ItemInstance) -> usize {
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
                item.pide.0,
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
                    affix: l.affix.checked_sub(1),
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
    pub fn info(&self, affix: u32) -> Option<AffixInfo> {
        let mut text = Str::EMPTY;
        let mut rarity = 0u8;
        let status =
            (self.host.api().affix_info)(self.host.handle(), affix, &raw mut text, &raw mut rarity);
        if !status.is_ok() {
            return None;
        }
        // SAFETY: `affix_info` returned OK, and the loader keeps its affixes'
        // texts alive and unchanged for the process; the text is copied.
        let text = unsafe { text.as_str() }?.to_owned();
        Some(AffixInfo {
            text,
            rarity: Rarity::from_raw(rarity)?,
        })
    }
}

/// Rolls on random loot, for [`Load::add_loot_affixes`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AffixLoot {
    /// The containers whose rolled loot rolls affixes.
    pub from: Source,
    /// The chance each fitting item rolls at all, from 0 to 1.
    pub chance: f64,
    /// What a roll may give.
    pub rules: RollRules,
}

impl AffixLoot {
    /// One affix at `chance` per fitting item in loot rolled `from`.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn new(from: Source, chance: f64) -> Self {
        Self {
            from,
            chance,
            rules: RollRules::new(1, 1),
        }
    }

    /// The same with `min` to `max` affixes.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn count(mut self, min: u8, max: u8) -> Self {
        self.rules.min = min;
        self.rules.max = max;
        self
    }

    /// The same, no rarer than `rarity`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::{AffixLoot, Rarity};
    /// use enshwick::loot::Source;
    ///
    /// let rule = AffixLoot::new(Source::bosses(), 1.0).max_rarity(Rarity::Epic);
    /// assert_eq!(rule.rules.max_rarity, Rarity::Epic);
    /// ```
    #[must_use]
    pub const fn max_rarity(mut self, rarity: Rarity) -> Self {
        self.rules.max_rarity = rarity;
        self
    }
}

/// Rolls on crafted items, for [`Load::add_craft_affixes`].
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{AffixCraft, RollRules};
///
/// let rule = AffixCraft::new(0.25, RollRules::new(1, 2));
/// assert_eq!(rule.chance, 0.25);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct AffixCraft {
    /// The chance each fitting crafted item rolls at all, from 0 to 1.
    pub chance: f64,
    /// What a roll may give.
    pub rules: RollRules,
}

impl AffixCraft {
    /// Rolls under `rules` at `chance` per fitting crafted item.
    ///
    /// # Examples
    ///
    /// See [`AffixCraft`].
    #[must_use]
    pub const fn new(chance: f64, rules: RollRules) -> Self {
        Self { chance, rules }
    }
}

/// The crafters waiting for their items: when they asked, and the items
/// they held then.
type Crafts = Mutex<HashMap<Entity, (Instant, HashSet<ItemInstance>)>>;

/// How long after a craft request the crafter's new items count as crafted.
const CRAFT_WINDOW: Duration = Duration::from_secs(1);
/// How often the loot and craft rules look for items to roll.
const SWEEP: Duration = Duration::from_millis(250);

/// The items with an entity in `slots`.
fn instances(slots: &[Slot]) -> impl Iterator<Item = ItemInstance> + '_ {
    slots.iter().filter_map(ItemInstance::of)
}

/// Rolls `item` under `rules` at `chance`, unless it rolled before; items
/// whose category no affix fits stay as they are.
fn try_roll(affixes: Affixes, item: ItemInstance, chance: f64, rules: RollRules) {
    let hit = fastrand::f64() < chance;
    let rules = if hit { rules } else { RollRules::new(0, 0) };
    let _rolled = affixes.roll(item, rules, None, true);
}

impl Load<'_> {
    /// Defines the mod's affix `name` (`<mod>:<name>`). Call it during
    /// `init`, on both sides, so clients name what items rolled.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id or definition, an affix defined twice, or
    /// after init.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn define_affix(&mut self, name: &str, def: &AffixDef) -> Result<u32, Off> {
        def.check()
            .map_err(|why| Off::new(format!("affix {name:?}: {why}")))?;
        let host = self.host();
        let ranges = def.to_ranges();
        let data = def.to_data(&ranges);
        let mut out = StatRef::default();
        let status =
            (host.api().affix_open)(host.handle(), Str::new(name), &raw const data, &raw mut out);
        crate::load::ok(status, out.index, || format!("affix {name:?}"))
    }

    /// The affix pool and rolls. Call it during `init`.
    ///
    /// # Examples
    ///
    /// See [`Affixes::roll`].
    #[must_use]
    pub fn affixes(&mut self) -> Affixes {
        Affixes { host: self.host() }
    }

    /// Rolls affixes onto the items with an entity that random loot puts
    /// into containers `rule.from` matches, each at `rule.chance`, as the
    /// container rolls its loot. Each item rolls once: an item rolled
    /// before, by this rule or another, stays as it is. On the server.
    ///
    /// # Errors
    ///
    /// [`Off`] for a chance outside 0 to 1, or when the build lacks the
    /// loot or inventory pieces.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn add_loot_affixes(&mut self, rule: AffixLoot) -> Result<(), Off> {
        if !(0.0..=1.0).contains(&rule.chance) {
            return Err(Off::new(format!(
                "affix loot: chance {} is not between 0 and 1",
                rule.chance
            )));
        }
        let affixes = self.affixes();
        let inventories = self.inventories()?;
        let rolled = crate::loot::watch_rolls(self)?;
        let scheduler = self.scheduler()?;
        scheduler.every(SWEEP, move || {
            let containers: Vec<_> = core::mem::take(&mut *rolled.relock());
            for roll in containers.iter().filter(|r| rule.from.matches(r)) {
                let Some(container) = roll.entity else {
                    continue;
                };
                for item in instances(&inventories.read().slots(container)) {
                    try_roll(affixes, item, rule.chance, rule.rules);
                }
            }
            core::ops::ControlFlow::Continue(())
        });
        Ok(())
    }

    /// Rolls affixes onto the items with an entity a player crafts, each at
    /// `rule.chance`: the items new in the crafter's inventories within a
    /// second of the craft request. Each item rolls once. On the server.
    ///
    /// # Errors
    ///
    /// [`Off`] for a chance outside 0 to 1, or when the build lacks the
    /// event or inventory pieces.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::{AffixCraft, RollRules};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_craft_affixes(AffixCraft::new(0.2, RollRules::new(1, 1)))
    /// }
    /// ```
    pub fn add_craft_affixes(&mut self, rule: AffixCraft) -> Result<(), Off> {
        if !(0.0..=1.0).contains(&rule.chance) {
            return Err(Off::new(format!(
                "affix craft: chance {} is not between 0 and 1",
                rule.chance
            )));
        }
        let affixes = self.affixes();
        let inventories = self.inventories()?;
        let scheduler = self.scheduler()?;
        let pending: Arc<Crafts> = Arc::default();
        let requests = Arc::clone(&pending);
        self.on_game::<CraftRequested>(move |craft| {
            let held: HashSet<ItemInstance> =
                instances(&inventories.read().slots(craft.player)).collect();
            requests
                .relock()
                .entry(craft.player)
                .or_insert((Instant::now(), held));
        })?;
        scheduler.every(SWEEP, move || {
            let due: Vec<(Entity, HashSet<ItemInstance>)> = {
                let mut pending = pending.relock();
                let now = Instant::now();
                let ready: Vec<Entity> = pending
                    .iter()
                    .filter(|(_, (at, _))| now.duration_since(*at) >= CRAFT_WINDOW)
                    .map(|(&player, _)| player)
                    .collect();
                ready
                    .into_iter()
                    .filter_map(|player| Some((player, pending.remove(&player)?.1)))
                    .collect()
            };
            for (player, before) in due {
                for item in
                    instances(&inventories.read().slots(player)).filter(|i| !before.contains(i))
                {
                    try_roll(affixes, item, rule.chance, rule.rules);
                }
            }
            core::ops::ControlFlow::Continue(())
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::StatRef as Ref;

    fn stat(index: u32) -> Stat {
        // Stats are handed out by the loader; tests build them by their raw
        // form.
        Stat::from_ref(Ref { index, key: index })
    }

    fn pool() -> Vec<(AffixKey, AffixDef)> {
        vec![
            (
                AffixKey(1),
                AffixDef::new("bear", Categories::EQUIPMENT)
                    .with(Range::new(stat(0), Op::Add, 10.0, 30.0).step(1.0)),
            ),
            (
                AffixKey(2),
                AffixDef::new("fire", Categories::EQUIPMENT | Categories::WEAPONS)
                    .with(Range::new(stat(1), Op::Add, 0.05, 0.15))
                    .group("element"),
            ),
            (
                AffixKey(3),
                AffixDef::new("ice", Categories::EQUIPMENT)
                    .with(Range::new(stat(2), Op::Add, 0.05, 0.15))
                    .group("element"),
            ),
            (
                AffixKey(4),
                AffixDef::new("myth", Categories::EQUIPMENT)
                    .with(Range::new(stat(3), Op::Multiply, 1.5, 2.0))
                    .rarity(Rarity::Legendary)
                    .weight(1),
            ),
        ]
    }

    fn candidates(pool: &[(AffixKey, AffixDef)]) -> Vec<Candidate<'_>> {
        pool.iter()
            .map(|(key, def)| Candidate { key: *key, def })
            .collect()
    }

    #[test]
    fn the_same_seed_rolls_the_same_whatever_the_pool_order() {
        let pool = pool();
        let mut reversed = candidates(&pool);
        reversed.reverse();
        for seed in 0..200 {
            let rules = RollRules::new(1, 3);
            assert_eq!(
                roll(&candidates(&pool), 1, rules, seed),
                roll(&reversed, 1, rules, seed)
            );
        }
    }

    #[test]
    fn rolls_keep_groups_ranges_steps_and_rarity() {
        let pool = pool();
        let all = candidates(&pool);
        for seed in 0..500 {
            let rolled = roll(&all, 1, RollRules::new(4, 4).max_rarity(Rarity::Epic), seed);
            assert!(rolled.len() <= 3, "the legendary one stays out");
            assert!(rolled.iter().all(|r| r.affix != AffixKey(4)));
            let elements = rolled
                .iter()
                .filter(|r| r.affix == AffixKey(2) || r.affix == AffixKey(3))
                .count();
            assert!(elements <= 1, "one of a group");
            for r in &rolled {
                let def = &pool.iter().find(|(k, _)| *k == r.affix).unwrap().1;
                for (value, range) in r.values.iter().zip(&def.ranges) {
                    assert!((range.min..=range.max).contains(value));
                    if range.step > 0.0 {
                        assert!((value - value.round()).abs() < 1e-4, "whole steps");
                    }
                }
            }
        }
    }

    #[test]
    fn categories_decide_what_fits_and_weights_how_often() {
        let pool = pool();
        let all = candidates(&pool);
        for seed in 0..100 {
            let rolled = roll(&all, 2, RollRules::new(1, 1), seed);
            assert_eq!(
                rolled.iter().map(|r| r.affix).collect::<Vec<_>>(),
                [AffixKey(2)]
            );
        }
        let myths = (0..2000)
            .filter(|&seed| {
                roll(&all, 1, RollRules::new(1, 1), seed)
                    .iter()
                    .any(|r| r.affix == AffixKey(4))
            })
            .count();
        assert!(myths < 40, "weight 1 of 301 rolls rarely: {myths}");
        assert!(roll(&all, 1, RollRules::new(0, 0), 7).is_empty());
    }

    #[test]
    fn definitions_need_modifiers_and_sane_ranges() {
        let empty = AffixDef::new("x", Categories::WEAPONS);
        assert!(empty.check().is_err());
        let backwards = empty.clone().with(Range::new(stat(0), Op::Add, 3.0, 1.0));
        assert!(backwards.check().is_err());
        let fine = empty.with(Range::new(stat(0), Op::Add, 1.0, 3.0));
        assert!(fine.check().is_ok());
    }
}
