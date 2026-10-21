//! Stats: named numbers on entities, such as extra health or a chance to
//! crit, that items, buffs and mods modify, worked out once per change and
//! read for free.
//!
//! A stat kind has a namespaced id (`<mod>:<name>`) and a [`StatDef`]: what
//! it is ([`StatType`]), its default, its range and the text key that names
//! it. A mod defines its own kinds with [`Load::stat`] and opens any other,
//! the platform's built-in ones in [`builtin`] among them, with
//! [`Load::open_stat`]; every mod that opens one id shares its values.
//!
//! [`Modifier`]s come from three sources: items an entity wears (their
//! [`ItemStats`]), buffs it has ([`Load::buff_modifiers`]) and modifiers a
//! mod keeps on it ([`Stats::set`]). The server sums them with [`aggregate`]
//! after each step in which a source changed, keeps the values, and raises
//! [`Load::on_stats_changed`] for each value that moved; [`Stats::value`]
//! reads a kept value without locks from any thread.
//!
//! The platform applies the built-in kinds to the game itself on the
//! server; a mod's own kinds mean what the mods that read them make of
//! them.
//!
//! # Examples
//!
//! ```
//! use core::time::Duration;
//! use enshwick_loader_api::stats::{Lasting, Modifier, Op, StatDef, builtin};
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     let luck = load.stat("luck", &StatDef::additive("stat.luck").range(0.0, 1.0))?;
//!     let health = load.open_stat(builtin::HEALTH_MAX)?;
//!     let stats = load.stats()?;
//!     load.add_command("bless", "more health and luck for a minute", move |ctx, _args| {
//!         let player = ctx.player.entity;
//!         let minute = Lasting::For(Duration::from_secs(60));
//!         let _health = stats.set(player, "bless", Modifier::new(health, Op::Add, 50.0), minute);
//!         let _luck = stats.set(player, "bless", Modifier::new(luck, Op::Add, 0.1), minute);
//!         format!("luck now {}", stats.value(player, luck)).into()
//!     })
//! }
//! ```

use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{
    Header, HostApi, StatChange as StatChangeData, StatDefData, StatModifierData, StatModify,
    StatRef, StatSource, Status, Str,
};
use crate::buffs::BuffId;
use crate::content::items::StatFormat;
use crate::ecs::Entity;
use crate::item_data::{ItemData, ItemInstance, SetError};
use crate::{Host, Load, Off};

/// The item data kind that holds every item's [`ItemStats`].
pub const ITEM_KIND: &str = "enshwick:stats";

/// The key stored for a full stat id: the game id of the id's GUID, as
/// [`Guid::named`](crate::content::Guid::named) derives it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
/// use enshwick_loader_api::stats::{StatKey, key_of};
///
/// assert_eq!(key_of("enshwick:health_max"), StatKey(Guid::named("enshwick:health_max").id()));
/// assert_ne!(key_of("a:b"), StatKey(0));
/// ```
#[must_use]
pub const fn key_of(id: &str) -> StatKey {
    StatKey(crate::content::Guid::hashed(id).id())
}

/// The platform's built-in stat kinds, which it applies to the game on the
/// server.
///
/// Open one with [`Load::open_stat`]. The maximum kinds add points to the
/// entity's maximum health, stamina and mana, as gear does. Damage kinds are factors on the hits
/// an entity deals, 1 without modifiers; resistances are the part of a
/// hit's damage the entity takes less, 0 without modifiers and 1 for none
/// at all. Both act on hits the entity deals or takes directly; the book's
/// chapter on stats lists what reaches the game.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::stats::builtin;
///
/// assert_eq!(builtin::DAMAGE_FIRE, "enshwick:damage_fire");
/// assert!(builtin::ALL.iter().any(|(id, _)| *id == builtin::RESISTANCE));
/// ```
pub mod builtin {
    use super::{StatDef, StatFormat};

    /// Extra maximum health, in points.
    pub const HEALTH_MAX: &str = "enshwick:health_max";
    /// Extra maximum stamina, in points.
    pub const STAMINA_MAX: &str = "enshwick:stamina_max";
    /// Extra maximum mana, in points.
    pub const MANA_MAX: &str = "enshwick:mana_max";
    /// Every hit's damage the entity deals, a factor.
    pub const DAMAGE: &str = "enshwick:damage";
    /// Cut damage the entity deals, a factor.
    pub const DAMAGE_CUT: &str = "enshwick:damage_cut";
    /// Pierce damage the entity deals, a factor.
    pub const DAMAGE_PIERCE: &str = "enshwick:damage_pierce";
    /// Blunt damage the entity deals, a factor.
    pub const DAMAGE_BLUNT: &str = "enshwick:damage_blunt";
    /// Fire damage the entity deals, a factor.
    pub const DAMAGE_FIRE: &str = "enshwick:damage_fire";
    /// Ice damage the entity deals, a factor.
    pub const DAMAGE_ICE: &str = "enshwick:damage_ice";
    /// Shroud damage the entity deals, a factor.
    pub const DAMAGE_SHROUD: &str = "enshwick:damage_shroud";
    /// Lightning damage the entity deals, a factor.
    pub const DAMAGE_LIGHTNING: &str = "enshwick:damage_lightning";
    /// Poison damage the entity deals, a factor.
    pub const DAMAGE_POISON: &str = "enshwick:damage_poison";
    /// The part of every hit's damage the entity takes less.
    pub const RESISTANCE: &str = "enshwick:resistance";
    /// The part of cut damage the entity takes less.
    pub const RESISTANCE_CUT: &str = "enshwick:resistance_cut";
    /// The part of pierce damage the entity takes less.
    pub const RESISTANCE_PIERCE: &str = "enshwick:resistance_pierce";
    /// The part of blunt damage the entity takes less.
    pub const RESISTANCE_BLUNT: &str = "enshwick:resistance_blunt";
    /// The part of fire damage the entity takes less.
    pub const RESISTANCE_FIRE: &str = "enshwick:resistance_fire";
    /// The part of ice damage the entity takes less.
    pub const RESISTANCE_ICE: &str = "enshwick:resistance_ice";
    /// The part of shroud damage the entity takes less.
    pub const RESISTANCE_SHROUD: &str = "enshwick:resistance_shroud";
    /// The part of lightning damage the entity takes less.
    pub const RESISTANCE_LIGHTNING: &str = "enshwick:resistance_lightning";
    /// The part of poison damage the entity takes less.
    pub const RESISTANCE_POISON: &str = "enshwick:resistance_poison";

    /// The greatest damage factor.
    const MAX_FACTOR: f32 = 100.0;

    /// The most points a maximum gains or loses.
    const MAX_POINTS: f32 = 100_000.0;

    const fn factor(text: &'static str) -> Def {
        Def {
            text,
            multiplicative: true,
            points: false,
            min: 0.0,
            max: MAX_FACTOR,
        }
    }

    const fn resistance(text: &'static str) -> Def {
        Def {
            text,
            multiplicative: false,
            points: false,
            min: -1.0,
            max: 1.0,
        }
    }

    const fn points(text: &'static str) -> Def {
        Def {
            text,
            multiplicative: false,
            points: true,
            min: -MAX_POINTS,
            max: MAX_POINTS,
        }
    }

    /// A built-in kind's definition, kept const.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Def {
        text: &'static str,
        multiplicative: bool,
        points: bool,
        min: f32,
        max: f32,
    }

    impl Def {
        /// The definition the platform gives the kind.
        ///
        /// # Examples
        ///
        /// ```
        /// use enshwick_loader_api::stats::builtin;
        ///
        /// let (_, def) = builtin::ALL[3];
        /// assert_eq!(def.to_def().default, 1.0);
        /// ```
        #[must_use]
        pub fn to_def(self) -> StatDef {
            let def = if self.multiplicative {
                StatDef::multiplicative(self.text)
            } else {
                StatDef::additive(self.text)
            };
            let format = if self.points {
                StatFormat::Signed
            } else {
                StatFormat::Percent
            };
            def.range(self.min, self.max).format(format)
        }
    }

    /// Every built-in kind with its definition, in the order the platform
    /// defines them.
    pub const ALL: [(&str, Def); 21] = [
        (HEALTH_MAX, points("enshwick.stat.health_max")),
        (STAMINA_MAX, points("enshwick.stat.stamina_max")),
        (MANA_MAX, points("enshwick.stat.mana_max")),
        (DAMAGE, factor("enshwick.stat.damage")),
        (DAMAGE_CUT, factor("enshwick.stat.damage_cut")),
        (DAMAGE_PIERCE, factor("enshwick.stat.damage_pierce")),
        (DAMAGE_BLUNT, factor("enshwick.stat.damage_blunt")),
        (DAMAGE_FIRE, factor("enshwick.stat.damage_fire")),
        (DAMAGE_ICE, factor("enshwick.stat.damage_ice")),
        (DAMAGE_SHROUD, factor("enshwick.stat.damage_shroud")),
        (DAMAGE_LIGHTNING, factor("enshwick.stat.damage_lightning")),
        (DAMAGE_POISON, factor("enshwick.stat.damage_poison")),
        (RESISTANCE, resistance("enshwick.stat.resistance")),
        (RESISTANCE_CUT, resistance("enshwick.stat.resistance_cut")),
        (
            RESISTANCE_PIERCE,
            resistance("enshwick.stat.resistance_pierce"),
        ),
        (
            RESISTANCE_BLUNT,
            resistance("enshwick.stat.resistance_blunt"),
        ),
        (RESISTANCE_FIRE, resistance("enshwick.stat.resistance_fire")),
        (RESISTANCE_ICE, resistance("enshwick.stat.resistance_ice")),
        (
            RESISTANCE_SHROUD,
            resistance("enshwick.stat.resistance_shroud"),
        ),
        (
            RESISTANCE_LIGHTNING,
            resistance("enshwick.stat.resistance_lightning"),
        ),
        (
            RESISTANCE_POISON,
            resistance("enshwick.stat.resistance_poison"),
        ),
    ];

    /// The per-type damage and resistance kinds, by the game's damage type
    /// index: cut, pierce, blunt, fire, ice, shroud, lightning, then poison
    /// at index 8; `None` for the types without one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::stats::builtin;
    ///
    /// assert_eq!(builtin::BY_DAMAGE_TYPE[3], Some((builtin::DAMAGE_FIRE, builtin::RESISTANCE_FIRE)));
    /// assert_eq!(builtin::BY_DAMAGE_TYPE[7], None);
    /// ```
    pub const BY_DAMAGE_TYPE: [Option<(&str, &str)>; 13] = [
        Some((DAMAGE_CUT, RESISTANCE_CUT)),
        Some((DAMAGE_PIERCE, RESISTANCE_PIERCE)),
        Some((DAMAGE_BLUNT, RESISTANCE_BLUNT)),
        Some((DAMAGE_FIRE, RESISTANCE_FIRE)),
        Some((DAMAGE_ICE, RESISTANCE_ICE)),
        Some((DAMAGE_SHROUD, RESISTANCE_SHROUD)),
        Some((DAMAGE_LIGHTNING, RESISTANCE_LIGHTNING)),
        None,
        Some((DAMAGE_POISON, RESISTANCE_POISON)),
        None,
        None,
        None,
        None,
    ];
}

/// What a stat's number is.
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

/// A stat kind's definition, for [`Load::stat`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::items::StatFormat;
/// use enshwick_loader_api::stats::{StatDef, StatType};
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
    /// use enshwick_loader_api::stats::{Op, StatDef, aggregate};
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
    /// use enshwick_loader_api::stats::StatDef;
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
    fn check(&self) -> Result<(), String> {
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
    /// use enshwick_loader_api::abi::StatDefData;
    /// use enshwick_loader_api::stats::StatDef;
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
    /// `data.text` points at text that is alive for the call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::stats::StatDef;
    ///
    /// let haste = StatDef::multiplicative("stat.haste");
    /// let data = haste.to_data();
    /// // SAFETY: the text is `haste`'s, alive here.
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
        // SAFETY: forwarded from the caller.
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

/// The stored name of a stat kind: [`key_of`] its full id.
///
/// # Examples
///
/// See [`key_of`].
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

/// A stat kind, from [`Load::stat`] or [`Load::open_stat`]. Copy it
/// anywhere.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stat {
    index: u32,
    key: StatKey,
}

impl Stat {
    /// The kind's stored name.
    ///
    /// # Examples
    ///
    /// ```
    /// fn same(a: enshwick_loader_api::stats::Stat, b: enshwick_loader_api::stats::Stat) -> bool {
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
    /// fn index(stat: enshwick_loader_api::stats::Stat) -> u32 {
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
    /// use enshwick_loader_api::abi::StatRef;
    /// use enshwick_loader_api::stats::{Stat, StatKey};
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

/// How a modifier acts on a stat. Modifiers apply in this order: every
/// `Add` to the default, then every `Multiply`, then the greatest `Max` as
/// a floor; an `Override` replaces the result, the last one in source
/// order winning. The kind's range applies last.
///
/// # Examples
///
/// See [`aggregate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum Op {
    /// Adds the value.
    Add,
    /// Multiplies by the value: 1.1 is 10 % more.
    Multiply,
    /// Raises the result to at least the value.
    Max,
    /// Sets the result.
    Override,
}

impl Op {
    /// The operation's number in [`StatModifierData::op`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::StatModifierData;
    /// use enshwick_loader_api::stats::Op;
    ///
    /// assert_eq!(Op::Max.to_raw(), StatModifierData::MAX);
    /// assert_eq!(Op::from_raw(StatModifierData::MAX), Some(Op::Max));
    /// ```
    #[must_use]
    pub const fn to_raw(self) -> u8 {
        match self {
            Self::Add => StatModifierData::ADD,
            Self::Multiply => StatModifierData::MULTIPLY,
            Self::Max => StatModifierData::MAX,
            Self::Override => StatModifierData::OVERRIDE,
        }
    }

    /// The operation numbered `raw`.
    ///
    /// # Examples
    ///
    /// See [`Op::to_raw`].
    #[must_use]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            StatModifierData::ADD => Some(Self::Add),
            StatModifierData::MULTIPLY => Some(Self::Multiply),
            StatModifierData::MAX => Some(Self::Max),
            StatModifierData::OVERRIDE => Some(Self::Override),
            _ => None,
        }
    }
}

/// The value of a stat with definition `def` under `modifiers`, given in
/// source order: worn items by their entity, then buffs by their id, then
/// the modifiers mods keep on the entity, by mod and key. Values that are
/// not finite are skipped. See [`Op`] for the order the operations apply
/// in.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::stats::{Op, StatDef, aggregate};
///
/// let health = StatDef::additive("stat.health").range(-100.0, 1000.0);
/// // (0 + 50 + 30) * 1.5, raised to at least 100.
/// assert_eq!(aggregate(&health, [(Op::Add, 50.0), (Op::Multiply, 1.5), (Op::Add, 30.0), (Op::Max, 100.0)]), 120.0);
/// // An override wins, the range still holds.
/// assert_eq!(aggregate(&health, [(Op::Add, 50.0), (Op::Override, 5000.0)]), 1000.0);
/// assert_eq!(aggregate(&health, []), 0.0);
/// ```
#[must_use]
pub fn aggregate(def: &StatDef, modifiers: impl IntoIterator<Item = (Op, f32)>) -> f32 {
    let mut add = 0.0f32;
    let mut factor = 1.0f32;
    let mut floor = f32::NEG_INFINITY;
    let mut replaced = None;
    for (op, value) in modifiers {
        if !value.is_finite() {
            continue;
        }
        match op {
            Op::Add => add += value,
            Op::Multiply => factor *= value,
            Op::Max => floor = floor.max(value),
            Op::Override => replaced = Some(value),
        }
    }
    let value = replaced.unwrap_or_else(|| ((def.default + add) * factor).max(floor));
    let value = if value.is_finite() {
        value.clamp(def.min, def.max)
    } else {
        def.default
    };
    match def.ty {
        StatType::Flag => f32::from(u8::from(value != 0.0)),
        StatType::Additive | StatType::Multiplicative => value,
    }
}

/// One modifier of one stat.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::stats::{Modifier, Op};
///
/// fn more_health(health: enshwick_loader_api::stats::Stat) -> Modifier {
///     Modifier::new(health, Op::Add, 50.0)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modifier {
    /// The stat.
    pub stat: Stat,
    /// What it does.
    pub op: Op,
    /// Its value.
    pub value: f32,
}

impl Modifier {
    /// `op` with `value` on `stat`.
    ///
    /// # Examples
    ///
    /// See [`Modifier`].
    #[must_use]
    pub const fn new(stat: Stat, op: Op, value: f32) -> Self {
        Self { stat, op, value }
    }

    const fn to_data(self) -> StatModifierData {
        StatModifierData {
            stat: self.stat.index,
            op: self.op.to_raw(),
            reserved: [0; 3],
            value: self.value,
        }
    }

    /// The same as an item keeps it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::stats::{Modifier, Op};
    ///
    /// fn stored(stat: enshwick_loader_api::stats::Stat) -> bool {
    ///     Modifier::new(stat, Op::Add, 5.0).to_item().stat == stat.key()
    /// }
    /// ```
    #[must_use]
    pub const fn to_item(self) -> ItemModifier {
        ItemModifier {
            stat: self.stat.key,
            op: self.op,
            value: self.value,
        }
    }
}

/// A modifier as an item keeps it, naming its stat by key, so it reads the
/// same in every process.
///
/// # Examples
///
/// See [`ItemStats`].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ItemModifier {
    /// The stat.
    pub stat: StatKey,
    /// What it does.
    pub op: Op,
    /// Its value.
    pub value: f32,
}

/// What a single item gives the entity that wears it, kept as its item data
/// of kind [`ITEM_KIND`]: modifiers of its own and the
/// [affixes](crate::affixes) it rolled.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::stats::{ItemModifier, ItemStats, Op, key_of};
///
/// let ring = ItemStats {
///     modifiers: vec![ItemModifier { stat: key_of("enshwick:resistance_fire"), op: Op::Add, value: 0.1 }],
///     ..ItemStats::default()
/// };
/// let bytes = postcard::to_allocvec(&ring)?;
/// assert_eq!(postcard::from_bytes::<ItemStats>(&bytes)?, ring);
/// # Ok::<(), postcard::Error>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ItemStats {
    /// The modifiers, in the order they apply.
    pub modifiers: Vec<ItemModifier>,
    /// The affixes it rolled, which apply after its own modifiers.
    pub affixes: Vec<crate::affixes::Rolled>,
    /// Whether it rolled affixes, even none; an item rolls once unless a mod
    /// rolls it again on purpose.
    pub rolled: bool,
}

/// How long a modifier a mod sets on an entity lasts.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Lasting {
    /// Until the mod removes it or the entity leaves the world.
    UntilRemoved,
    /// For a while of real time.
    For(Duration),
    /// For a player's account: kept with the world and given back each
    /// time the player joins, until the mod removes it.
    Saved,
}

/// Why a modifier was not set.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::stats::ModifyError;
///
/// assert_eq!(ModifyError::NotServer.to_string(), "only the server keeps modifiers");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ModifyError {
    /// Called on a client that hosts no game.
    #[error("only the server keeps modifiers")]
    NotServer,
    /// A saved modifier for an entity that is no player.
    #[error("only players keep saved modifiers")]
    NotPlayer,
    /// The loader refused for another reason.
    #[error("modifier refused (status {0})")]
    Refused(u32),
}

/// Where a modifier that counts toward a stat comes from, from
/// [`Stats::sources`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::stats::Origin;
///
/// fn worn(origin: Origin) -> bool {
///     matches!(origin, Origin::Item(_))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Origin {
    /// An item the entity wears.
    Item(ItemInstance),
    /// A buff the entity has.
    Buff(BuffId),
    /// A modifier a mod keeps on the entity: the mod's load-order index and
    /// the hash of its key.
    Mod {
        /// The mod's index in load order.
        index: u32,
        /// The hash of the modifier's key.
        key: u32,
    },
}

/// One modifier that counts toward a stat, with its origin.
///
/// # Examples
///
/// See [`Stats::sources`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Source {
    /// Where it comes from.
    pub origin: Origin,
    /// The stat's index in this process (see [`Stat::index`]).
    pub stat: u32,
    /// What it does.
    pub op: Op,
    /// Its value.
    pub value: f32,
}

/// A stat whose value changed, for [`Load::on_stats_changed`].
///
/// # Examples
///
/// See [`Load::on_stats_changed`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct StatChange {
    /// The entity.
    pub entity: Entity,
    /// The stat.
    pub stat: Stat,
    /// The value before.
    pub old: f32,
    /// The value now.
    pub new: f32,
}

/// What a stat kind is called and how it reads, from [`Stats::info`].
///
/// # Examples
///
/// See [`Stats::info`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StatInfo {
    /// The full id, `<mod>:<name>`.
    pub id: String,
    /// The definition.
    pub def: StatDef,
}

/// The FNV-1a 32 hash of a modifier's key.
const fn key_hash(key: &str) -> u32 {
    key_of(key).0
}

/// Stat values and modifiers; get it from [`Load::stats`]. Copy it anywhere.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Stats {
    host: Host,
    items: ItemData<ItemStats>,
}

impl Stats {
    /// The value of `stat` on `entity`: what the server worked out after the
    /// last step a source changed, or the default. Any thread, without
    /// locks; a client answers defaults.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn value(&self, entity: Entity, stat: Stat) -> f32 {
        let mut out = 0.0;
        let status =
            (self.host.api().stat_value)(self.host.handle(), entity.0, stat.index, &raw mut out);
        if status.is_ok() { out } else { 0.0 }
    }

    /// Whether `stat` is on for `entity`: its value is not 0.
    ///
    /// # Examples
    ///
    /// ```
    /// fn glows(stats: &enshwick_loader_api::stats::Stats, glow: enshwick_loader_api::stats::Stat, entity: enshwick_loader_api::ecs::Entity) -> bool {
    ///     stats.is_on(entity, glow)
    /// }
    /// ```
    #[must_use]
    pub fn is_on(&self, entity: Entity, stat: Stat) -> bool {
        self.value(entity, stat) != 0.0
    }

    /// Sets the mod's modifier `key` on `entity` for `modifier.stat`,
    /// replacing the one the mod set before under the same key and stat. The
    /// value follows after the next step. On the server, any thread.
    ///
    /// # Errors
    ///
    /// [`ModifyError`] on a client alone, or for a saved modifier on an
    /// entity that is no player.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn set(
        &self,
        entity: Entity,
        key: &str,
        modifier: Modifier,
        lasting: Lasting,
    ) -> Result<(), ModifyError> {
        let (flags, duration) = match lasting {
            Lasting::UntilRemoved => (0, 0),
            Lasting::For(d) => (0, u64::try_from(d.as_millis()).unwrap_or(u64::MAX).max(1)),
            Lasting::Saved => (StatModify::SAVED, 0),
        };
        self.modify(entity, key, modifier.to_data(), flags, duration)
    }

    /// Removes the mod's modifier `key` on `entity` for `stat`.
    ///
    /// # Errors
    ///
    /// As for [`Stats::set`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn unbless(stats: &enshwick_loader_api::stats::Stats, health: enshwick_loader_api::stats::Stat, player: enshwick_loader_api::ecs::Entity) {
    ///     let _removed = stats.remove(player, "bless", health);
    /// }
    /// ```
    pub fn remove(&self, entity: Entity, key: &str, stat: Stat) -> Result<(), ModifyError> {
        let data = StatModifierData {
            stat: stat.index,
            ..StatModifierData::default()
        };
        self.modify(entity, key, data, StatModify::REMOVE, 0)
    }

    fn modify(
        &self,
        entity: Entity,
        key: &str,
        modifier: StatModifierData,
        flags: u32,
        duration_ms: u64,
    ) -> Result<(), ModifyError> {
        let request = StatModify {
            header: Header::of::<StatModify>(),
            entity: entity.0,
            key: key_hash(key),
            modifier,
            flags,
            duration_ms,
        };
        match (self.host.api().stat_modify)(self.host.handle(), &raw const request) {
            Status::OK => Ok(()),
            Status::REFUSED if flags & StatModify::SAVED != 0 => Err(ModifyError::NotPlayer),
            Status::REFUSED | Status::WRONG_THREAD => Err(ModifyError::NotServer),
            other => Err(ModifyError::Refused(other.0)),
        }
    }

    /// What `item` gives the entity that wears it; empty for an item without
    /// any. On a client, what the server sent.
    ///
    /// # Examples
    ///
    /// ```
    /// fn bonuses(stats: &enshwick_loader_api::stats::Stats, item: enshwick_loader_api::item_data::ItemInstance) -> usize {
    ///     stats.item(item).modifiers.len()
    /// }
    /// ```
    #[must_use]
    pub fn item(&self, item: ItemInstance) -> ItemStats {
        self.items.get(item).unwrap_or_default()
    }

    /// Sets what `item` gives the entity that wears it; an empty
    /// [`ItemStats`] removes it. Clients that run Enshwick get it, and the
    /// entities that wear the item follow after the next step.
    ///
    /// # Errors
    ///
    /// [`SetError`] as for [`ItemData::set`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::stats::{ItemStats, Modifier, Op};
    ///
    /// fn enchant(stats: &enshwick_loader_api::stats::Stats, fire: enshwick_loader_api::stats::Stat, ring: enshwick_loader_api::item_data::ItemInstance) {
    ///     let mut bonuses = stats.item(ring);
    ///     bonuses.modifiers.push(Modifier::new(fire, Op::Add, 0.1).to_item());
    ///     let _set = stats.set_item(ring, &bonuses);
    /// }
    /// ```
    pub fn set_item(&self, item: ItemInstance, stats: &ItemStats) -> Result<(), SetError> {
        if *stats == ItemStats::default() {
            self.items.remove(item)
        } else {
            self.items.set(item, stats)
        }
    }

    /// Every modifier that counts toward `stat` on `entity`, or toward every
    /// stat for `None`, in the order they apply. On the server, any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::stats::Origin;
    ///
    /// fn from_items(stats: &enshwick_loader_api::stats::Stats, player: enshwick_loader_api::ecs::Entity) -> usize {
    ///     stats.sources(player, None).iter().filter(|s| matches!(s.origin, Origin::Item(_))).count()
    /// }
    /// ```
    #[must_use]
    pub fn sources(&self, entity: Entity, stat: Option<Stat>) -> Vec<Source> {
        let stat = stat.map_or(u32::MAX, |s| s.index);
        let mut out = vec![StatSource::default(); 32];
        loop {
            let mut count = 0u32;
            let status = (self.host.api().stat_sources)(
                self.host.handle(),
                entity.0,
                stat,
                out.as_mut_ptr(),
                u32::try_from(out.len()).unwrap_or(u32::MAX),
                &raw mut count,
            );
            let count = count as usize;
            if !status.is_ok() {
                return Vec::new();
            }
            if count <= out.len() {
                out.truncate(count);
                break;
            }
            out = vec![StatSource::default(); count];
        }
        out.iter()
            .filter_map(|s| {
                let origin = match s.kind {
                    StatSource::ITEM => Origin::Item(ItemInstance {
                        pide: Entity(s.id),
                        item: s.owner,
                    }),
                    StatSource::BUFF => Origin::Buff(BuffId(s.id)),
                    StatSource::ENTITY => Origin::Mod {
                        index: s.owner,
                        key: s.id,
                    },
                    _ => return None,
                };
                Some(Source {
                    origin,
                    stat: s.stat,
                    op: Op::from_raw(s.op)?,
                    value: s.value,
                })
            })
            .collect()
    }

    /// The id and definition of `stat`; `None` for a kind opened but never
    /// defined. Any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn name(stats: &enshwick_loader_api::stats::Stats, stat: enshwick_loader_api::stats::Stat) -> String {
    ///     stats.info(stat).map_or_else(String::new, |info| info.id)
    /// }
    /// ```
    #[must_use]
    pub fn info(&self, stat: Stat) -> Option<StatInfo> {
        let mut def = StatDef::additive("").to_data();
        let mut id = Str::EMPTY;
        let status =
            (self.host.api().stat_info)(self.host.handle(), stat.index, &raw mut def, &raw mut id);
        if !status.is_ok() {
            return None;
        }
        // SAFETY: the loader keeps its definitions' text and ids for the
        // process.
        let def = unsafe { StatDef::from_data(&def) }?;
        // SAFETY: as above.
        let id = unsafe { id.as_str() }?.to_owned();
        Some(StatInfo { id, def })
    }

    /// Watches `entity` for the items it wears and the buffs it has, or stops
    /// watching it. Players are always watched, and so is every entity a mod
    /// keeps a modifier on; watch other entities whose buffs should count,
    /// such as an enemy a mod curses. Any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn curse(stats: &enshwick_loader_api::stats::Stats, enemy: enshwick_loader_api::ecs::Entity) {
    ///     stats.watch(enemy, true);
    /// }
    /// ```
    pub fn watch(&self, entity: Entity, watch: bool) {
        let _status = (self.host.api().stat_watch)(self.host.handle(), entity.0, u8::from(watch));
    }
}

unsafe extern "system" fn run_changed<F>(change: *const StatChangeData, user: *mut c_void)
where
    F: Fn(&StatChange),
{
    // SAFETY: the loader passes a valid change for the duration of this call.
    let Some(change) = (unsafe { change.as_ref() }) else {
        return;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let change = StatChange {
        entity: Entity(change.entity),
        stat: Stat::from_ref(change.stat),
        old: change.old,
        new: change.new,
    };
    crate::host::guard("panic in a stats handler", || (), || handler(&change));
}

impl Load<'_> {
    fn open_stat_with(&mut self, id: &str, def: Option<&StatDef>) -> Result<Stat, Off> {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, stat_watch),
            "stats",
        )?;
        let data = def.map(StatDef::to_data);
        let def_ptr = data.as_ref().map_or(core::ptr::null(), core::ptr::from_ref);
        let mut out = StatRef::default();
        let status = (host.api().stat_open)(host.handle(), Str::new(id), def_ptr, &raw mut out);
        crate::load::ok(status, Stat::from_ref(out), || format!("stat {id:?}"))
    }

    /// Defines the mod's stat kind `name` (`<mod>:<name>`, or the full id in
    /// the mod's namespace), each side 1 to 64 of `[a-z0-9_-]`. Call it
    /// during `init`, on both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id or definition, a kind defined twice, after
    /// init, or with a loader older than the API.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn stat(&mut self, name: &str, def: &StatDef) -> Result<Stat, Off> {
        def.check()
            .map_err(|why| Off::new(format!("stat {name:?}: {why}")))?;
        self.open_stat_with(name, Some(def))
    }

    /// Opens stat kind `id` by its full id, another mod's or a built-in one
    /// from [`builtin`]. A kind no mod defines keeps 0 and reads nothing.
    /// Call it during `init`, on both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id, after init, or with a loader older than
    /// the API.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn open_stat(&mut self, id: &str) -> Result<Stat, Off> {
        self.open_stat_with(id, None)
    }

    /// The stat values and modifiers. Call it during `init`.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, or with a loader older than the API.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn stats(&mut self) -> Result<Stats, Off> {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, stat_watch),
            "stats",
        )?;
        Ok(Stats {
            host,
            items: self.item_data(ITEM_KIND)?,
        })
    }

    /// Gives every entity that has buff `buff` the `modifiers`, while it has
    /// it: a mod's own buff type, or a vanilla one. Buffs count on watched
    /// entities (see [`Stats::watch`]). Call it during `init`.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, or with a loader older than the API.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::buffs::BuffId;
    /// use enshwick_loader_api::stats::{Modifier, Op, builtin};
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let fire = load.open_stat(builtin::RESISTANCE_FIRE)?;
    ///     load.buff_modifiers(BuffId(0x89cb_0b34), &[Modifier::new(fire, Op::Add, 0.2)])
    /// }
    /// ```
    pub fn buff_modifiers(&mut self, buff: BuffId, modifiers: &[Modifier]) -> Result<(), Off> {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, stat_watch),
            "stats",
        )?;
        let data: Vec<StatModifierData> = modifiers.iter().map(|m| m.to_data()).collect();
        let status = (host.api().stat_buff)(
            host.handle(),
            buff.0,
            data.as_ptr(),
            u32::try_from(data.len()).unwrap_or(u32::MAX),
        );
        crate::load::ok(status, (), || format!("buff modifiers for {:#x}", buff.0))
    }

    /// Runs `handler` for every stat value that changed on a watched entity,
    /// on the server's game thread after the step that changed it.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, or with a loader older than the API.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let luck = load.open_stat("other-mod:luck")?;
    ///     load.on_stats_changed(move |change| {
    ///         if change.stat == luck {
    ///             enshwick_loader_api::info!("{:?} has luck {}", change.entity, change.new);
    ///         }
    ///     })
    /// }
    /// ```
    pub fn on_stats_changed<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&StatChange) + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, stat_watch),
            "stats",
        )?;
        crate::load::hand_over(
            handler,
            |user| (host.api().stats_changed)(host.handle(), run_changed::<F>, user),
            "stats handler",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    fn def() -> StatDef {
        StatDef::additive("t").range(-1000.0, 1000.0)
    }

    #[test]
    fn operations_apply_in_their_order_whatever_the_input_order() {
        let forward = [(Op::Add, 10.0), (Op::Multiply, 2.0), (Op::Add, 5.0)];
        let backward = [(Op::Add, 5.0), (Op::Multiply, 2.0), (Op::Add, 10.0)];
        assert!(close(aggregate(&def(), forward), 30.0));
        assert!(close(aggregate(&def(), backward), 30.0));
    }

    #[test]
    fn the_greatest_max_is_a_floor_and_the_last_override_wins() {
        let mods = [(Op::Add, 5.0), (Op::Max, 20.0), (Op::Max, 12.0)];
        assert!(close(aggregate(&def(), mods), 20.0));
        let mods = [(Op::Override, 1.0), (Op::Add, 500.0), (Op::Override, 7.0)];
        assert!(close(aggregate(&def(), mods), 7.0));
    }

    #[test]
    fn the_range_holds_and_bad_values_are_skipped() {
        assert!(close(aggregate(&def(), [(Op::Add, 1e9)]), 1000.0));
        assert!(close(
            aggregate(&def(), [(Op::Add, f32::NAN), (Op::Add, 2.0)]),
            2.0
        ));
        assert!(close(
            aggregate(&def(), [(Op::Multiply, f32::INFINITY)]),
            0.0
        ));
        let factor = StatDef::multiplicative("f").range(0.0, 10.0);
        assert!(close(
            aggregate(&factor, [(Op::Multiply, 1.5), (Op::Multiply, 2.0)]),
            3.0
        ));
        assert!(close(aggregate(&factor, [(Op::Add, -5.0)]), 0.0));
    }

    #[test]
    fn flags_read_as_zero_or_one() {
        let flag = StatDef::flag("f");
        assert!(close(aggregate(&flag, []), 0.0));
        assert!(close(
            aggregate(&flag, [(Op::Add, 1.0), (Op::Add, 1.0)]),
            1.0
        ));
        assert!(close(
            aggregate(&flag, [(Op::Add, 1.0), (Op::Override, 0.0)]),
            0.0
        ));
    }

    #[test]
    fn definitions_out_of_their_own_range_are_refused() {
        assert!(StatDef::additive("x").range(1.0, 2.0).check().is_err());
        assert!(StatDef::additive("x").range(2.0, 1.0).check().is_err());
        assert!(StatDef::multiplicative("x").check().is_ok());
        assert!(
            StatDef::additive("x")
                .starting_at(f32::NAN)
                .check()
                .is_err()
        );
    }

    #[test]
    fn built_in_kinds_are_valid_and_their_keys_differ() {
        let mut keys: Vec<_> = builtin::ALL.iter().map(|(id, _)| key_of(id)).collect();
        for (id, def) in builtin::ALL {
            assert!(def.to_def().check().is_ok(), "{id}");
            assert!(id.starts_with("enshwick:"));
        }
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), builtin::ALL.len());
    }

    #[test]
    fn item_stats_round_trip_through_postcard() {
        let stats = ItemStats {
            modifiers: vec![
                ItemModifier {
                    stat: key_of("a:b"),
                    op: Op::Multiply,
                    value: 1.25,
                },
                ItemModifier {
                    stat: key_of("a:c"),
                    op: Op::Override,
                    value: -3.0,
                },
            ],
            ..ItemStats::default()
        };
        let bytes = postcard::to_allocvec(&stats).unwrap();
        assert!(bytes.len() < 32);
        assert_eq!(postcard::from_bytes::<ItemStats>(&bytes).unwrap(), stats);
    }
}
