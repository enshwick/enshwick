use core::time::Duration;

use crate::abi::StatModifierData;
use crate::buffs::BuffTypeId;
use crate::item_data::ItemInstance;

use super::defs::{Stat, StatDef, StatKey, StatType};

/// How a modifier acts on a stat.
///
/// Modifiers apply in this order: every `Add` to the default, then every
/// `Multiply`, then the greatest `Max` as a floor; an `Override` replaces the
/// result, the last one in source order winning. The kind's range applies last.
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
    /// use enshwick::abi::StatModifierData;
    /// use enshwick::stats::Op;
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

/// The value of a stat with definition `def` under `modifiers`, given in source
/// order: worn items by their entity, then buffs by their id, then the
/// modifiers mods keep on the entity, by mod and key.
///
/// Values that are not finite are skipped. See [`Op`] for the order the
/// operations apply in.
///
/// # Examples
///
/// ```
/// use enshwick::stats::{Op, StatDef, aggregate};
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
/// use enshwick::stats::{Modifier, Op};
///
/// fn more_health(health: enshwick::stats::Stat) -> Modifier {
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

    pub(super) const fn to_data(self) -> StatModifierData {
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
    /// use enshwick::stats::{Modifier, Op};
    ///
    /// fn stored(stat: enshwick::stats::Stat) -> bool {
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
/// of kind [`ITEM_KIND`](crate::stats::ITEM_KIND): modifiers of its own and the
/// [affixes](crate::affixes) it rolled.
///
/// # Examples
///
/// ```
/// use enshwick::stats::{ItemModifier, ItemStats, Op, key_of};
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
/// See the [module](crate::stats).
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
/// use enshwick::stats::ModifyError;
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
/// [`Stats::sources`](crate::stats::Stats::sources).
///
/// # Examples
///
/// ```
/// use enshwick::stats::Origin;
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
    Buff(BuffTypeId),
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
/// See [`Stats::sources`](crate::stats::Stats::sources).
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
