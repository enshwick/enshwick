use crate::abi::{StatRef, StatSpec};
use crate::content::items::StatFormat;

/// What a stat's number is.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See [`StatDef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum StatType {
    /// A sum of points, 0 by default, such as extra health.
    Additive,
    /// A factor, 1 by default, such as a damage multiplier.
    Multiplicative,
    /// On or off, off by default: any value but 0 is on.
    Flag,
}

/// A stat kind's definition, for [`Load::define_stat`](../struct.Load.html#method.define_stat).
///
/// # Examples
///
/// ```
/// use enshwick::content::items::StatFormat;
/// use enshwick::stats::{StatDef, StatType};
///
/// let crit = StatDef::additive().range(0.0, 1.0).format(StatFormat::Percent);
/// assert_eq!(crit.ty, StatType::Additive);
/// assert_eq!((crit.default, crit.min, crit.max), (0.0, 0.0, 1.0));
/// let haste = StatDef::multiplicative();
/// assert_eq!(haste.default, 1.0);
/// ```
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct StatDef {
    /// What the number is.
    pub ty: StatType,
    /// The value without modifiers.
    pub default: f32,
    /// The least value.
    pub min: f32,
    /// The greatest value.
    pub max: f32,
    /// How the value reads.
    pub format: StatFormat,
    /// The group of the Character screen's attributes where the local
    /// player's value shows as a row; `None` for no row. Unstable: needs the
    /// `unstable` feature and may change in any release.
    #[cfg(feature = "unstable")]
    #[script(default)]
    pub group: Option<StatGroup>,
    /// Shown nowhere: no row on the Character screen and no line in item
    /// tooltips, for a number only mods read.
    #[script(default)]
    pub hidden: bool,
}

/// A group of the Character screen's attributes, where a stat's row shows.
///
/// The names are the game's groups; the column beside the tabs shows the
/// first two, the Basics, Damage and Protection tabs the others.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::stats::{StatDef, StatGroup};
///
/// let luck = StatDef::additive().in_group(StatGroup::Primary);
/// assert_eq!(luck.group, Some(StatGroup::Primary));
/// ```
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum StatGroup {
    /// The column beside the tabs, with the attribute points.
    Main,
    /// The column beside the tabs, under the shroud survival rows.
    FogSurvival,
    /// The Basics tab's first group.
    Primary,
    /// The Basics tab's regeneration group.
    Regeneration,
    /// The Basics tab's last group.
    Misc,
    /// The Damage tab's first group.
    Damage,
    /// The Damage tab's weapon damage group.
    WeaponDamage,
    /// The Damage tab's damage against enemies.
    EnemyDamage,
    /// The Protection tab.
    Protection,
}

#[cfg(feature = "unstable")]
impl StatGroup {
    /// Every group, in the order the screen shows them.
    pub const ALL: &'static [Self] = &[
        Self::Main,
        Self::FogSurvival,
        Self::Primary,
        Self::Regeneration,
        Self::Misc,
        Self::Damage,
        Self::WeaponDamage,
        Self::EnemyDamage,
        Self::Protection,
    ];

    /// The group's number in a [`StatSpec`]: its place in [`StatGroup::ALL`]
    /// plus one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::StatSpec;
    /// use enshwick::stats::StatGroup;
    ///
    /// assert_eq!(StatGroup::Protection.to_raw(), StatSpec::GROUP_PROTECTION);
    /// ```
    #[must_use]
    pub const fn to_raw(self) -> u8 {
        self as u8 + 1
    }

    /// The group numbered `raw` in a [`StatSpec`]; `None` for none or a
    /// number this build does not know.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::StatSpec;
    /// use enshwick::stats::StatGroup;
    ///
    /// assert_eq!(StatGroup::from_raw(StatSpec::GROUP_MAIN), Some(StatGroup::Main));
    /// assert_eq!(StatGroup::from_raw(StatSpec::GROUP_NONE), None);
    /// ```
    #[must_use]
    pub fn from_raw(raw: u8) -> Option<Self> {
        Self::ALL.get(usize::from(raw).checked_sub(1)?).copied()
    }
}

impl StatDef {
    const fn of(ty: StatType, default: f32, format: StatFormat) -> Self {
        Self {
            ty,
            default,
            min: f32::MIN,
            max: f32::MAX,
            format,
            #[cfg(feature = "unstable")]
            group: None,
            hidden: false,
        }
    }

    /// The same, shown as a row of `group` on the Character screen with the
    /// local player's value.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Examples
    ///
    /// See [`StatGroup`].
    #[cfg(feature = "unstable")]
    #[must_use]
    pub const fn in_group(mut self, group: StatGroup) -> Self {
        self.group = Some(group);
        self
    }

    /// The same, shown nowhere: neither a row on the Character screen nor a
    /// line in item tooltips.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::stats::StatDef;
    ///
    /// assert!(StatDef::flag().hidden().hidden);
    /// ```
    #[must_use]
    pub const fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// A sum of points, 0 by default.
    ///
    /// # Examples
    ///
    /// See [`StatDef`].
    #[must_use]
    pub const fn additive() -> Self {
        Self::of(StatType::Additive, 0.0, StatFormat::Signed)
    }

    /// A factor, 1 by default.
    ///
    /// # Examples
    ///
    /// See [`StatDef`].
    #[must_use]
    pub const fn multiplicative() -> Self {
        Self::of(StatType::Multiplicative, 1.0, StatFormat::Percent)
    }

    /// On or off, off by default.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::stats::{Op, StatDef, aggregate};
    ///
    /// let glows = StatDef::flag();
    /// assert_eq!(aggregate(&glows, [(Op::Add, 3.0)]), 1.0);
    /// ```
    #[must_use]
    pub const fn flag() -> Self {
        let mut def = Self::of(StatType::Flag, 0.0, StatFormat::Number);
        def.min = 0.0;
        def.max = 1.0;
        def
    }

    /// The same, kept between `min` and `max`.
    ///
    /// # Examples
    ///
    /// See [`StatDef`].
    #[must_use]
    pub const fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// The same with `value` without modifiers.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::stats::StatDef;
    ///
    /// assert_eq!(StatDef::additive().starting_at(5.0).default, 5.0);
    /// ```
    #[must_use]
    pub const fn starting_at(mut self, value: f32) -> Self {
        self.default = value;
        self
    }

    /// The same, read as `format`.
    ///
    /// # Examples
    ///
    /// See [`StatDef`].
    #[must_use]
    pub const fn format(mut self, format: StatFormat) -> Self {
        self.format = format;
        self
    }

    /// Why the loader would refuse it, if it would.
    pub(super) fn check(&self) -> Result<(), DefineStatError> {
        let finite = self.default.is_finite() && self.min.is_finite() && self.max.is_finite();
        if !finite || self.min > self.max || !(self.min..=self.max).contains(&self.default) {
            return Err(DefineStatError {
                default: self.default,
                min: self.min,
                max: self.max,
            });
        }
        Ok(())
    }

    /// The definition as the loader takes it; the loader names the kind
    /// itself (see [`name_key`](crate::stats::name_key)).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::StatSpec;
    /// use enshwick::stats::StatDef;
    ///
    /// let data = StatDef::multiplicative().to_data();
    /// assert_eq!(data.ty, StatSpec::MULTIPLICATIVE);
    /// ```
    #[must_use]
    pub fn to_data(&self) -> StatSpec {
        enshwick_abi::headed!(StatSpec {
            ty: match self.ty {
                StatType::Additive => StatSpec::ADDITIVE,
                StatType::Multiplicative => StatSpec::MULTIPLICATIVE,
                StatType::Flag => StatSpec::FLAG,
            },
            format: enshwick_content::__internal::stat_format(self.format),
            reserved: 0,
            default: self.default,
            min: self.min,
            max: self.max,
            #[cfg(feature = "unstable")]
            group: self.group.map_or(StatSpec::GROUP_NONE, StatGroup::to_raw),
            #[cfg(not(feature = "unstable"))]
            group: StatSpec::GROUP_NONE,
            flags: if self.hidden { StatSpec::HIDDEN } else { 0 },
            reserved_display: 0,
        })
    }

    /// The definition the loader handed out, without its text; `None` for
    /// one that does not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::stats::StatDef;
    ///
    /// let haste = StatDef::multiplicative();
    /// assert_eq!(StatDef::from_data(&haste.to_data()), Some(haste));
    /// ```
    #[must_use]
    pub fn from_data(data: &StatSpec) -> Option<Self> {
        let ty = match data.ty {
            StatSpec::ADDITIVE => StatType::Additive,
            StatSpec::MULTIPLICATIVE => StatType::Multiplicative,
            StatSpec::FLAG => StatType::Flag,
            _ => return None,
        };
        let format = match data.format {
            StatSpec::SIGNED => StatFormat::Signed,
            StatSpec::PERCENT => StatFormat::Percent,
            _ => StatFormat::Number,
        };
        let def = Self {
            ty,
            default: data.default,
            min: data.min,
            max: data.max,
            format,
            #[cfg(feature = "unstable")]
            group: StatGroup::from_raw(data.group),
            hidden: data.flags & StatSpec::HIDDEN != 0,
        };
        def.check().is_ok().then_some(def)
    }
}

/// Why the loader refuses a [`StatDef`]: its default lies outside its range,
/// or a bound is not finite.
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[error("default {default} is not within {min}..{max}")]
pub(crate) struct DefineStatError {
    default: f32,
    min: f32,
    max: f32,
}

/// The stored name of a stat kind: [`key_of`](crate::stats::key_of) its full id.
///
/// # Examples
///
/// See [`key_of`](crate::stats::key_of).
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
    crate::Script,
)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct StatKey(pub u32);

/// A stat kind, from [`Load::define_stat`](../struct.Load.html#method.define_stat) or [`Load::open_stat`](../struct.Load.html#method.open_stat). Copy it
/// anywhere.
///
/// # Examples
///
/// See the [module](crate::stats).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stat {
    pub(super) index: u32,
    pub(super) key: StatKey,
}

impl crate::script::Object for Stat {
    const CLASS: &'static crate::script::Class = &crate::script::Class::of::<Self>(
        "Stat",
        "A stat kind from `define_stat` or `open_stat`, valid in this game process.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_KIND];
}

#[crate::script(kind)]
impl Stat {
    /// The kind's stored name.
    ///
    /// # Examples
    ///
    /// ```
    /// fn same(a: enshwick::stats::Stat, b: enshwick::stats::Stat) -> bool {
    ///     a.key() == b.key()
    /// }
    /// ```
    #[must_use]
    #[script]
    pub const fn key(self) -> StatKey {
        self.key
    }

    /// The kind's index in this process, as the loader's calls name it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn index(stat: enshwick::stats::Stat) -> u32 {
    ///     stat.index()
    /// }
    /// ```
    #[must_use]
    #[script]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// The kind the loader handed out as `stat`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::StatRef;
    /// use enshwick::stats::{Stat, StatKey};
    ///
    /// let stat = Stat::from_ref(StatRef { index: 3, key: 0x1234 });
    /// assert_eq!((stat.index(), stat.key()), (3, StatKey(0x1234)));
    /// ```
    #[must_use]
    pub const fn from_ref(stat: StatRef) -> Self {
        Self {
            index: stat.index,
            key: StatKey(stat.key),
        }
    }
}
