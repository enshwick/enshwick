use crate::abi::{Header, StatDefData, StatRef, Str};
use crate::content::items::StatFormat;

/// What a stat's number is.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See [`StatDef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StatType {
    /// A sum of points, 0 by default, such as extra health.
    Additive,
    /// A factor, 1 by default, such as a damage multiplier.
    Multiplicative,
    /// On or off, off by default: any value but 0 is on.
    Flag,
}

/// A stat kind's definition, for [`Load::define_stat`](crate::Load::define_stat).
///
/// # Examples
///
/// ```
/// use enshwick::content::items::StatFormat;
/// use enshwick::stats::{StatDef, StatType};
///
/// let crit = StatDef::additive("stat.crit").range(0.0, 1.0).format(StatFormat::Percent);
/// assert_eq!(crit.ty, StatType::Additive);
/// assert_eq!((crit.default, crit.min, crit.max), (0.0, 0.0, 1.0));
/// let haste = StatDef::multiplicative("stat.haste");
/// assert_eq!(haste.default, 1.0);
/// ```
#[derive(Clone, Debug, PartialEq)]
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
    /// The text key that names the stat in the mod's texts.
    pub text: String,
    /// How the value reads.
    pub format: StatFormat,
}

impl StatDef {
    fn of(ty: StatType, default: f32, text: &str, format: StatFormat) -> Self {
        Self {
            ty,
            default,
            min: f32::MIN,
            max: f32::MAX,
            text: text.to_owned(),
            format,
        }
    }

    /// A sum of points, 0 by default, named by text key `text`.
    ///
    /// # Examples
    ///
    /// See [`StatDef`].
    #[must_use]
    pub fn additive(text: &str) -> Self {
        Self::of(StatType::Additive, 0.0, text, StatFormat::Signed)
    }

    /// A factor, 1 by default, named by text key `text`.
    ///
    /// # Examples
    ///
    /// See [`StatDef`].
    #[must_use]
    pub fn multiplicative(text: &str) -> Self {
        Self::of(StatType::Multiplicative, 1.0, text, StatFormat::Percent)
    }

    /// On or off, off by default, named by text key `text`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::stats::{Op, StatDef, aggregate};
    ///
    /// let glows = StatDef::flag("stat.glows");
    /// assert_eq!(aggregate(&glows, [(Op::Add, 3.0)]), 1.0);
    /// ```
    #[must_use]
    pub fn flag(text: &str) -> Self {
        let mut def = Self::of(StatType::Flag, 0.0, text, StatFormat::Number);
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
    /// assert_eq!(StatDef::additive("stat.base").starting_at(5.0).default, 5.0);
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
    pub(super) fn check(&self) -> Result<(), String> {
        let finite = self.default.is_finite() && self.min.is_finite() && self.max.is_finite();
        if !finite || self.min > self.max || !(self.min..=self.max).contains(&self.default) {
            return Err(format!(
                "default {} is not within {}..{}",
                self.default, self.min, self.max
            ));
        }
        Ok(())
    }

    /// The definition as the loader takes it, borrowing the text.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::StatDefData;
    /// use enshwick::stats::StatDef;
    ///
    /// let data = StatDef::multiplicative("stat.haste").to_data();
    /// assert_eq!(data.ty, StatDefData::MULTIPLICATIVE);
    /// ```
    #[must_use]
    pub fn to_data(&self) -> StatDefData {
        StatDefData {
            header: Header::of::<StatDefData>(),
            ty: match self.ty {
                StatType::Additive => StatDefData::ADDITIVE,
                StatType::Multiplicative => StatDefData::MULTIPLICATIVE,
                StatType::Flag => StatDefData::FLAG,
            },
            format: match self.format {
                StatFormat::Number => StatDefData::NUMBER,
                StatFormat::Signed => StatDefData::SIGNED,
                StatFormat::Percent => StatDefData::PERCENT,
            },
            reserved: 0,
            default: self.default,
            min: self.min,
            max: self.max,
            text: Str::new(&self.text),
        }
    }

    /// The definition the loader handed out; `None` for one that does not
    /// read.
    ///
    /// # Safety
    ///
    /// - `data.text` is null or points at its `len` bytes, alive and unchanged
    ///   for the call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::stats::StatDef;
    ///
    /// let haste = StatDef::multiplicative("stat.haste");
    /// let data = haste.to_data();
    /// // SAFETY: `data.text` points into `haste`, alive and unchanged here.
    /// assert_eq!(unsafe { StatDef::from_data(&data) }, Some(haste));
    /// ```
    #[must_use]
    pub unsafe fn from_data(data: &StatDefData) -> Option<Self> {
        let ty = match data.ty {
            StatDefData::ADDITIVE => StatType::Additive,
            StatDefData::MULTIPLICATIVE => StatType::Multiplicative,
            StatDefData::FLAG => StatType::Flag,
            _ => return None,
        };
        let format = match data.format {
            StatDefData::SIGNED => StatFormat::Signed,
            StatDefData::PERCENT => StatFormat::Percent,
            _ => StatFormat::Number,
        };
        // SAFETY: the caller keeps the text's bytes alive and unchanged for the
        // call, and `text` is copied before it returns.
        let text = unsafe { data.text.as_str() }?;
        let def = Self {
            ty,
            default: data.default,
            min: data.min,
            max: data.max,
            text: text.to_owned(),
            format,
        };
        def.check().is_ok().then_some(def)
    }
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
)]
pub struct StatKey(pub u32);

/// A stat kind, from [`Load::define_stat`](crate::Load::define_stat) or [`Load::open_stat`](crate::Load::open_stat). Copy it
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
