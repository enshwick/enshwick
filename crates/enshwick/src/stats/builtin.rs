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
    /// use enshwick::stats::builtin;
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
/// use enshwick::stats::builtin;
///
/// assert_eq!(builtin::BY_DAMAGE_TYPE[3], Some((builtin::DAMAGE_FIRE, builtin::RESISTANCE_FIRE)));
/// assert_eq!(builtin::BY_DAMAGE_TYPE[7], None);
/// ```
pub const BY_DAMAGE_TYPE: [Option<(&str, &str)>; crate::damage::DamageTypeNew::ALL.len()] = [
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
