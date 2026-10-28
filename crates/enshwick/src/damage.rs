//! Damage filters: see and change damage before it reaches an entity's
//! health in the server's world, by cause and, for hits, by damage type.
//!
//! A hit's damage comes by type, after the target's resistances and before
//! its armour: the armour, a block and the hit the game raises all follow
//! what the filters leave. Falls, the shroud, drowning and freezing take
//! health without a hit and come as one amount; an administrator's kill
//! comes as an amount of 1 that a filter can only let through or cancel. A
//! filter scales, sets or cancels; filters of several mods run in load
//! order, each on what the one before left. Executions are hits; heals and
//! health set by the game's scripts are not filtered.
//!
//! Filters run on the game's worker threads, in parallel, inside the
//! systems that deal damage: they must not allocate, lock or log, and read
//! only what they hold themselves, such as settings in atomics.
//!
//! # Examples
//!
//! ```
//! use enshwick::damage::{DamageCause, DamageTypeNew};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on_damage(|damage| match damage.cause() {
//!         // Falls hurt half as much, fire twice as much.
//!         DamageCause::Fall => damage.scale(0.5),
//!         DamageCause::Hit => damage.scale_type(DamageTypeNew::MagicalFire, 2.0),
//!         _ => {}
//!     })
//! }
//! ```

use core::ffi::c_void;

use crate::Entity;
use crate::abi::DamageCheck;
use crate::{Load, Off};

/// A damage type, as the game sorts a hit's damage.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DamageTypeNew {
    /// Blades.
    PhysicalCut,
    /// Arrows and spears.
    PhysicalPierce,
    /// Clubs and hammers.
    PhysicalBlunt,
    /// Fire.
    MagicalFire,
    /// Ice.
    MagicalIce,
    /// The shroud.
    MagicalFog,
    /// Lightning.
    MagicalLightning,
    /// Healing: what the hit heals, not what it hurts.
    Healing,
    /// Poison.
    Poison,
    /// Damage to wood.
    Wood,
    /// Damage to stone.
    Stone,
    /// Damage to metal.
    Metal,
    /// Explosions.
    Explosion,
}

impl DamageTypeNew {
    /// Every type, in the game's order.
    pub const ALL: [Self; 13] = [
        Self::PhysicalCut,
        Self::PhysicalPierce,
        Self::PhysicalBlunt,
        Self::MagicalFire,
        Self::MagicalIce,
        Self::MagicalFog,
        Self::MagicalLightning,
        Self::Healing,
        Self::Poison,
        Self::Wood,
        Self::Stone,
        Self::Metal,
        Self::Explosion,
    ];

    const fn index(self) -> usize {
        self as usize
    }
}

/// What deals damage.
///
/// The name is enshwick's: the loader's hook points, not a game type. The
/// shroud's causes take the game's word for it, as its death causes and
/// damage types do.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DamageCause {
    /// A hit from a weapon, a spell, a creature or an explosion, by type.
    Hit,
    /// A fall.
    Fall,
    /// The shroud draining a character whose time in it ran out.
    Fog,
    /// The deadly shroud, which kills at once.
    DeadlyFog,
    /// A character out of breath under water.
    Drowning,
    /// A creature out of breath under water, which dies at once.
    DrowningDeath,
    /// A character frozen through, out of body heat.
    Freezing,
    /// An administrator's kill command: [`Damage::total`] is 1, and the kill
    /// goes ahead unless a filter [cancels](Damage::cancel) it.
    AdminKill,
    /// A cause this API does not name.
    Other(u32),
}

impl DamageCause {
    /// Every cause this API names, without [`Other`](Self::Other): the set
    /// a tally by cause iterates, so a newly named cause joins it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::damage::DamageCause;
    ///
    /// assert!(DamageCause::NAMED.contains(&DamageCause::Fall));
    /// ```
    pub const NAMED: &[Self] = &[
        Self::Hit,
        Self::Fall,
        Self::Fog,
        Self::DeadlyFog,
        Self::Drowning,
        Self::DrowningDeath,
        Self::Freezing,
        Self::AdminKill,
    ];

    /// The cause of the ABI's code, as [`DamageCheck`] numbers them.
    const fn from_code(code: u32) -> Self {
        match code {
            DamageCheck::HIT => Self::Hit,
            DamageCheck::FALL => Self::Fall,
            DamageCheck::FOG => Self::Fog,
            DamageCheck::DEADLY_FOG => Self::DeadlyFog,
            DamageCheck::DROWNING => Self::Drowning,
            DamageCheck::DROWNING_DEATH => Self::DrowningDeath,
            DamageCheck::FREEZING => Self::Freezing,
            DamageCheck::ADMIN_KILL => Self::AdminKill,
            other => Self::Other(other),
        }
    }
}

/// Damage about to reach an entity's health, for a filter registered with
/// [`Load::on_damage`] to see and change.
///
/// # Examples
///
/// See the [module](self).
#[derive(Debug)]
pub struct Damage<'a> {
    check: &'a mut DamageCheck,
}

impl Damage<'_> {
    /// What deals it.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn cause(&self) -> DamageCause {
        DamageCause::from_code(self.check.cause)
    }

    /// The entity that takes it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Entity 2 takes no damage at all.
    ///     load.on_damage(|damage| {
    ///         if damage.target().0 == 2 {
    ///             damage.cancel();
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub const fn target(&self) -> Entity {
        self.check.target
    }

    /// What dealt a hit: the attacker, or its projectile; `None` for other
    /// causes.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Nothing hurts itself.
    ///     load.on_damage(|damage| {
    ///         if damage.source() == Some(damage.target()) {
    ///             damage.cancel();
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub const fn source(&self) -> Option<Entity> {
        if self.check.source.is_none() {
            None
        } else {
            Some(self.check.source)
        }
    }

    /// Who dealt a hit: the attacker, also for its arrows and spells; `None`
    /// for other causes.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Entity 7 deals double damage, by hand or by bow.
    ///     load.on_damage(|damage| {
    ///         if damage.attacker().is_some_and(|a| a.0 == 7) {
    ///             damage.scale(2.0);
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub const fn attacker(&self) -> Option<Entity> {
        if self.check.attacker.is_none() {
            self.source()
        } else {
            Some(self.check.attacker)
        }
    }

    /// A hit's damage of type `ty`; 0 for other causes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::damage::DamageTypeNew;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Fire never deals more than 50 at once.
    ///     load.on_damage(|damage| {
    ///         let fire = damage.amount(DamageTypeNew::MagicalFire);
    ///         damage.set(DamageTypeNew::MagicalFire, fire.min(50.0));
    ///     })
    /// }
    /// ```
    #[must_use]
    pub const fn amount(&self, ty: DamageTypeNew) -> f32 {
        self.check.amounts[ty.index()]
    }

    /// Sets a hit's damage of type `ty`; does nothing for other causes. A
    /// negative or not finite amount counts as none.
    ///
    /// # Examples
    ///
    /// See [`Damage::amount`].
    pub const fn set(&mut self, ty: DamageTypeNew, amount: f32) {
        if self.check.cause == DamageCheck::HIT {
            self.check.amounts[ty.index()] = amount;
        }
    }

    /// The damage the health takes: for a hit, the sum of its types but
    /// healing, before the target's armour.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // No single blow deals more than 200.
    ///     load.on_damage(|damage| {
    ///         let total = damage.total();
    ///         if total > 200.0 {
    ///             damage.scale(200.0 / total);
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub fn total(&self) -> f32 {
        if self.check.cause == DamageCheck::HIT {
            DamageTypeNew::ALL
                .iter()
                .filter(|&&ty| ty != DamageTypeNew::Healing)
                .map(|&ty| self.amount(ty))
                .sum()
        } else {
            self.check.total
        }
    }

    /// Scales the damage by `factor`: every type of a hit but healing, or
    /// the amount of another cause.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn scale(&mut self, factor: f32) {
        if self.check.cause == DamageCheck::HIT {
            for ty in DamageTypeNew::ALL {
                if ty != DamageTypeNew::Healing {
                    self.check.amounts[ty.index()] *= factor;
                }
            }
        } else {
            self.check.total *= factor;
        }
    }

    /// Scales a hit's damage of type `ty` by `factor`; does nothing for
    /// other causes.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn scale_type(&mut self, ty: DamageTypeNew, factor: f32) {
        if self.check.cause == DamageCheck::HIT {
            self.check.amounts[ty.index()] *= factor;
        }
    }

    /// Takes all damage away: the health stays as it is. A hit still lands,
    /// with no damage, as against an immune target.
    ///
    /// # Examples
    ///
    /// See [`Damage::target`].
    pub fn cancel(&mut self) {
        self.scale(0.0);
    }
}

unsafe extern "system" fn run_damage<F>(check: *mut DamageCheck, user: *mut c_void)
where
    F: Fn(&mut Damage<'_>),
{
    // SAFETY: the loader passes a valid check for the duration of this call.
    let Some(check) = (unsafe { check.as_mut() }) else {
        return;
    };
    let mut working = *check;
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let done = crate::host::guard(
        "panic in a damage filter",
        || false,
        || {
            filter(&mut Damage {
                check: &mut working,
            });
            true
        },
    );
    if done {
        *check = working;
    }
}

impl Load<'_> {
    /// Runs `filter` on damage about to reach an entity's health in the
    /// server's world: hits by type, falls and the shroud. What it leaves is
    /// what the game applies. It runs on the game's worker threads, inside
    /// the systems that deal damage, in the dedicated server or a hosting
    /// client: it must not allocate, lock or log.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the damage sites.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_damage<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut Damage<'_>) + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().damage_filter)(host.handle(), run_damage::<F>, user),
            "damage filter",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cause_the_api_names_is_in_named() {
        let named: Vec<DamageCause> = (0..=u32::from(u8::MAX))
            .map(DamageCause::from_code)
            .filter(|cause| !matches!(cause, DamageCause::Other(_)))
            .collect();
        assert_eq!(named, DamageCause::NAMED);
    }
}
