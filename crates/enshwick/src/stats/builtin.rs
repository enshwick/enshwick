use super::{StatDef, StatFormat};
use crate::content::Id;

/// Extra maximum health, in points.
pub const HEALTH_MAX: Id = Id::new("enshwick:health_max");
/// Extra maximum stamina, in points.
pub const STAMINA_MAX: Id = Id::new("enshwick:stamina_max");
/// Extra maximum mana, in points.
pub const MANA_MAX: Id = Id::new("enshwick:mana_max");
/// Every hit's damage the entity deals, a factor.
pub const DAMAGE: Id = Id::new("enshwick:damage");
/// Cut damage the entity deals, a factor.
pub const DAMAGE_CUT: Id = Id::new("enshwick:damage_cut");
/// Pierce damage the entity deals, a factor.
pub const DAMAGE_PIERCE: Id = Id::new("enshwick:damage_pierce");
/// Blunt damage the entity deals, a factor.
pub const DAMAGE_BLUNT: Id = Id::new("enshwick:damage_blunt");
/// Fire damage the entity deals, a factor.
pub const DAMAGE_FIRE: Id = Id::new("enshwick:damage_fire");
/// Ice damage the entity deals, a factor.
pub const DAMAGE_ICE: Id = Id::new("enshwick:damage_ice");
/// Shroud damage the entity deals, a factor.
pub const DAMAGE_SHROUD: Id = Id::new("enshwick:damage_shroud");
/// Lightning damage the entity deals, a factor.
pub const DAMAGE_LIGHTNING: Id = Id::new("enshwick:damage_lightning");
/// Poison damage the entity deals, a factor.
pub const DAMAGE_POISON: Id = Id::new("enshwick:damage_poison");
/// The part of every hit's damage the entity takes less.
pub const RESISTANCE: Id = Id::new("enshwick:resistance");
/// The part of cut damage the entity takes less.
pub const RESISTANCE_CUT: Id = Id::new("enshwick:resistance_cut");
/// The part of pierce damage the entity takes less.
pub const RESISTANCE_PIERCE: Id = Id::new("enshwick:resistance_pierce");
/// The part of blunt damage the entity takes less.
pub const RESISTANCE_BLUNT: Id = Id::new("enshwick:resistance_blunt");
/// The part of fire damage the entity takes less.
pub const RESISTANCE_FIRE: Id = Id::new("enshwick:resistance_fire");
/// The part of ice damage the entity takes less.
pub const RESISTANCE_ICE: Id = Id::new("enshwick:resistance_ice");
/// The part of shroud damage the entity takes less.
pub const RESISTANCE_SHROUD: Id = Id::new("enshwick:resistance_shroud");
/// The part of lightning damage the entity takes less.
pub const RESISTANCE_LIGHTNING: Id = Id::new("enshwick:resistance_lightning");
/// The part of poison damage the entity takes less.
pub const RESISTANCE_POISON: Id = Id::new("enshwick:resistance_poison");

/// The greatest damage factor.
const MAX_FACTOR: f32 = 100.0;

/// The most points a maximum gains or loses.
const MAX_POINTS: f32 = 100_000.0;

const FACTOR: Def = Def {
    multiplicative: true,
    points: false,
    min: 0.0,
    max: MAX_FACTOR,
};

const RESISTANCE_DEF: Def = Def {
    multiplicative: false,
    points: false,
    min: -1.0,
    max: 1.0,
};

const POINTS: Def = Def {
    multiplicative: false,
    points: true,
    min: -MAX_POINTS,
    max: MAX_POINTS,
};

/// A built-in kind's definition, kept const.
///
/// # Examples
///
/// See [`Def::to_def`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Def {
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
    /// use enshwick::stats::builtin;
    ///
    /// let (_, def) = builtin::ALL[3];
    /// assert_eq!(def.to_def().default, 1.0);
    /// ```
    #[must_use]
    pub fn to_def(self) -> StatDef {
        let def = if self.multiplicative {
            StatDef::multiplicative()
        } else {
            StatDef::additive()
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
pub const ALL: [(Id, Def); 21] = [
    (HEALTH_MAX, POINTS),
    (STAMINA_MAX, POINTS),
    (MANA_MAX, POINTS),
    (DAMAGE, FACTOR),
    (DAMAGE_CUT, FACTOR),
    (DAMAGE_PIERCE, FACTOR),
    (DAMAGE_BLUNT, FACTOR),
    (DAMAGE_FIRE, FACTOR),
    (DAMAGE_ICE, FACTOR),
    (DAMAGE_SHROUD, FACTOR),
    (DAMAGE_LIGHTNING, FACTOR),
    (DAMAGE_POISON, FACTOR),
    (RESISTANCE, RESISTANCE_DEF),
    (RESISTANCE_CUT, RESISTANCE_DEF),
    (RESISTANCE_PIERCE, RESISTANCE_DEF),
    (RESISTANCE_BLUNT, RESISTANCE_DEF),
    (RESISTANCE_FIRE, RESISTANCE_DEF),
    (RESISTANCE_ICE, RESISTANCE_DEF),
    (RESISTANCE_SHROUD, RESISTANCE_DEF),
    (RESISTANCE_LIGHTNING, RESISTANCE_DEF),
    (RESISTANCE_POISON, RESISTANCE_DEF),
];

/// The per-type damage and resistance kinds, by the game's damage type
/// index: cut, pierce, blunt, fire, ice, shroud, lightning, then poison
/// at index 8; `None` for the types without one.
///
/// # Examples
///
/// ```
/// use enshwick::stats::builtin;
///
/// assert_eq!(builtin::BY_DAMAGE_TYPE[3], Some((builtin::DAMAGE_FIRE, builtin::RESISTANCE_FIRE)));
/// assert_eq!(builtin::BY_DAMAGE_TYPE[7], None);
/// ```
pub const BY_DAMAGE_TYPE: [Option<(Id, Id)>; crate::damage::DamageTypeNew::ALL.len()] = [
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
