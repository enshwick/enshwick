use crate::abi::{AffixDefData, AffixModifierData, Header, Str};
use crate::stats::{Op, Stat};

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
    pub(super) fn at(&self, unit: f32) -> f32 {
        let raw = self.min + unit * (self.max - self.min);
        let value = if self.step > 0.0 {
            self.min + ((raw - self.min) / self.step).round() * self.step
        } else {
            raw
        };
        value.clamp(self.min.min(self.max), self.max.max(self.min))
    }
}

/// An affix's definition, for [`Load::define_affix`](crate::Load::define_affix).
///
/// # Examples
///
/// See the [module](crate::affixes).
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
    /// See the [module](crate::affixes).
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
    /// See the [module](crate::affixes).
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
    /// See the [module](crate::affixes).
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
    pub(super) fn check(&self) -> Result<(), String> {
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
