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
//! The filters need the `unstable` feature and may change in any release:
//! the order of health, stamina and mana in the game's attributes, and who
//! the game records as a killer, are not yet seen in the running game. The
//! damage types are stable.
//!
//! # Examples
//!
//! ```
//! use enshwick::damage::DamageTypeNew;
//!
//! assert_eq!(DamageTypeNew::ALL[DamageTypeNew::MagicalFire.index()], DamageTypeNew::MagicalFire);
//! ```

#[cfg(feature = "unstable")]
use core::ffi::c_void;

#[cfg(feature = "unstable")]
use crate::Entity;
#[cfg(feature = "unstable")]
use crate::Off;
#[cfg(feature = "unstable")]
use crate::abi::DamageCheck;

/// A damage type, as the game sorts a hit's damage.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
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
    pub const ALL: [Self; crate::abi::DAMAGE_TYPES] = [
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

    /// The type's place in a hit's damage amounts,
    /// [`DamageCheck::amounts`](crate::abi::DamageCheck::amounts), the game's
    /// order of its damage types.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::damage::DamageTypeNew;
    ///
    /// assert_eq!(DamageTypeNew::ALL[DamageTypeNew::Healing.index()], DamageTypeNew::Healing);
    /// ```
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// What deals damage.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// The name is enshwick's: the loader's hook points, not a game type. A
/// cause the game names among its death causes (`DeathCause`) takes that
/// name; the others are enshwick's and say so.
///
/// # Examples
///
/// See the [module](self).
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum DamageCause {
    /// A hit from a weapon, a spell, a creature or an explosion, by type.
    Impact,
    /// A fall.
    FallDamage,
    /// The shroud draining a character whose time in it ran out.
    Fog,
    /// The deadly shroud, which kills at once.
    DeadlyFog,
    /// A character out of breath under water.
    Suffocated,
    /// A creature out of breath under water, which dies at once. The name
    /// is enshwick's, not the game's.
    DrowningDeath,
    /// A character frozen through, out of body heat. The name is
    /// enshwick's, not the game's.
    Freezing,
    /// An administrator's kill command: [`Damage::total`] is 1, and the kill
    /// goes ahead unless a filter [cancels](Damage::cancel) it. The name is
    /// enshwick's, not the game's.
    AdminKill,
    /// A cause this API does not name.
    Other(u32),
}

#[cfg(feature = "unstable")]
impl DamageCause {
    /// Every cause this API names, without [`Other`](Self::Other): the set
    /// a tally by cause iterates, so a newly named cause joins it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::damage::DamageCause;
    ///
    /// assert!(DamageCause::NAMED.contains(&DamageCause::FallDamage));
    /// ```
    pub const NAMED: &[Self] = &[
        Self::Impact,
        Self::FallDamage,
        Self::Fog,
        Self::DeadlyFog,
        Self::Suffocated,
        Self::DrowningDeath,
        Self::Freezing,
        Self::AdminKill,
    ];

    /// The cause of the ABI's code, as [`DamageCheck`] numbers them.
    const fn from_code(code: u32) -> Self {
        match code {
            DamageCheck::IMPACT => Self::Impact,
            DamageCheck::FALL_DAMAGE => Self::FallDamage,
            DamageCheck::FOG => Self::Fog,
            DamageCheck::DEADLY_FOG => Self::DeadlyFog,
            DamageCheck::SUFFOCATED => Self::Suffocated,
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
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// See the [module](self).
#[cfg(feature = "unstable")]
#[derive(Debug)]
pub struct Damage<'a> {
    check: &'a mut DamageCheck,
}

#[cfg(feature = "unstable")]
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
        if self.check.cause == DamageCheck::IMPACT {
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
        if self.check.cause == DamageCheck::IMPACT {
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
        if self.check.cause == DamageCheck::IMPACT {
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
        if self.check.cause == DamageCheck::IMPACT {
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

#[cfg(feature = "unstable")]
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

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_damage`.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    fn on_damage<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut Damage<'_>) + Send + Sync + 'static;

    /// As `Load::on_damage_script`.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    fn on_damage_script(
        &mut self,
        filter: impl Fn(std::sync::Arc<ScriptDamage>) + Send + Sync + 'static,
    ) -> Result<(), Off>;
}

impl LoadMethods for crate::BaseLoad {
    #[cfg(feature = "unstable")]
    fn on_damage<F>(&mut self, filter: F) -> Result<(), Off>
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

    #[cfg(feature = "unstable")]
    fn on_damage_script(
        &mut self,
        filter: impl Fn(std::sync::Arc<ScriptDamage>) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        use crate::Relock as _;
        use core::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        /// Frees the kept copy on a panic too, which the hit's guard catches.
        struct Free<'a>(&'a AtomicBool);
        impl Drop for Free<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let kept = std::sync::OnceLock::new();
        let busy = AtomicBool::new(false);
        self.on_damage(move |damage| {
            let fresh = || Arc::new(ScriptDamage(Arc::new(std::sync::Mutex::new(*damage.check))));
            // A hit while the kept copy is out, on another thread or inside
            // the filter, gets a copy of its own.
            if busy.swap(true, Ordering::Acquire) {
                let copy = fresh();
                filter(Arc::clone(&copy));
                *damage.check = *copy.0.relock();
                return;
            }
            let _free = Free(&busy);
            let copy: &Arc<ScriptDamage> = kept.get_or_init(fresh);
            *copy.0.relock() = *damage.check;
            filter(Arc::clone(copy));
            *damage.check = *copy.0.relock();
        })
    }
}

/// The damage a script's filter sees and changes: a copy of the check,
/// written back once the filter returns, with [`Damage`]'s methods; one
/// per filter, filled again on every hit.
#[cfg(feature = "unstable")]
#[derive(Clone, Debug)]
pub(crate) struct ScriptDamage(std::sync::Arc<std::sync::Mutex<DamageCheck>>);

#[cfg(feature = "unstable")]
impl ScriptDamage {
    /// Runs `with` on the copy as a [`Damage`].
    fn with<R>(&self, with: impl FnOnce(&mut Damage<'_>) -> R) -> R {
        use crate::Relock as _;
        let mut check = self.0.relock();
        with(&mut Damage { check: &mut check })
    }
}

#[cfg(feature = "unstable")]
impl crate::script::Object for ScriptDamage {
    const CLASS: &'static crate::script::Class = &crate::script::Class::lent::<Self>(
        "Damage",
        "Damage about to reach an entity's health, for a filter to see and change.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_DAMAGE];
}

#[cfg(feature = "unstable")]
#[crate::script(damage)]
impl ScriptDamage {
    /// What deals it, as [`Damage::cause`] says.
    #[script(name = "cause")]
    pub(crate) fn cause_script(&self) -> DamageCause {
        self.with(|d| d.cause())
    }

    /// The entity that takes it, as [`Damage::target`] says.
    #[script(name = "target")]
    pub(crate) fn target_script(&self) -> Entity {
        self.with(|d| d.target())
    }

    /// What dealt a hit, as [`Damage::source`] says.
    #[script(name = "source")]
    pub(crate) fn source_script(&self) -> Option<Entity> {
        self.with(|d| d.source())
    }

    /// Who dealt a hit, as [`Damage::attacker`] says.
    #[script(name = "attacker")]
    pub(crate) fn attacker_script(&self) -> Option<Entity> {
        self.with(|d| d.attacker())
    }

    /// A hit's damage of type `ty`, as [`Damage::amount`] reads it.
    #[script(name = "amount")]
    pub(crate) fn amount_script(&self, ty: DamageTypeNew) -> f32 {
        self.with(|d| d.amount(ty))
    }

    /// Sets a hit's damage of type `ty`, as [`Damage::set`] does.
    #[script(name = "set")]
    pub(crate) fn set_script(&self, ty: DamageTypeNew, amount: f32) {
        self.with(|d| d.set(ty, amount));
    }

    /// The damage the health takes, as [`Damage::total`] sums it.
    #[script(name = "total")]
    pub(crate) fn total_script(&self) -> f32 {
        self.with(|d| d.total())
    }

    /// Scales the damage by `factor`, as [`Damage::scale`] does.
    #[script(name = "scale")]
    pub(crate) fn scale_script(&self, factor: f32) {
        self.with(|d| d.scale(factor));
    }

    /// Scales a hit's damage of type `ty`, as [`Damage::scale_type`] does.
    #[script(name = "scale_type")]
    pub(crate) fn scale_type_script(&self, ty: DamageTypeNew, factor: f32) {
        self.with(|d| d.scale_type(ty, factor));
    }

    /// Takes all damage away, as [`Damage::cancel`] does.
    #[script(name = "cancel")]
    pub(crate) fn cancel_script(&self) {
        self.with(|d| {
            d.cancel();
        });
    }
}

#[cfg(test)]
#[cfg(feature = "unstable")]
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
