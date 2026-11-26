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

#[cfg(feature = "unstable")]
pub(crate) mod script;

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
    pub const ALL: &'static [Self] = &[
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

    /// The cause the ABI numbers `code`, as [`DamageCheck`] and
    /// [`HostApi::damage_causes_off`](crate::abi::HostApi::damage_causes_off)
    /// do; [`Other`](Self::Other) for a number this API does not name.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::damage::DamageCause;
    ///
    /// assert_eq!(DamageCause::from_abi(DamageCause::Fog.to_abi()), DamageCause::Fog);
    /// assert_eq!(DamageCause::from_abi(99), DamageCause::Other(99));
    /// ```
    #[must_use]
    pub const fn from_abi(code: u32) -> Self {
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

    /// The cause's number in the ABI.
    ///
    /// # Examples
    ///
    /// See [`DamageCause::from_abi`].
    #[must_use]
    pub const fn to_abi(self) -> u32 {
        match self {
            Self::Impact => DamageCheck::IMPACT,
            Self::FallDamage => DamageCheck::FALL_DAMAGE,
            Self::Fog => DamageCheck::FOG,
            Self::DeadlyFog => DamageCheck::DEADLY_FOG,
            Self::Suffocated => DamageCheck::SUFFOCATED,
            Self::DrowningDeath => DamageCheck::DROWNING_DEATH,
            Self::Freezing => DamageCheck::FREEZING,
            Self::AdminKill => DamageCheck::ADMIN_KILL,
            Self::Other(code) => code,
        }
    }
}

/// The damage causes no filter of
/// [`Load::on_damage`](../struct.Load.html#method.on_damage) gets on the
/// running build, to tell the player or turn a feature of the mod off.
///
/// The loader found no site for them, or their site faulted; the list has
/// no set order. Ask after init, such as on `WorldLoaded`. With no filter
/// registered by any mod, every cause is off but a hit that stats arm.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Errors
///
/// [`Off`] during init and until the loader has linked and armed its hooks,
/// and without a loader.
///
/// # Examples
///
/// ```
/// use enshwick::damage::{DamageCause, causes_off};
///
/// fn falls_reach_us() -> Result<bool, enshwick::Off> {
///     Ok(!causes_off()?.contains(&DamageCause::FallDamage))
/// }
/// ```
#[cfg(feature = "unstable")]
pub fn causes_off() -> Result<Vec<DamageCause>, Off> {
    let host = crate::host().ok_or_else(|| Off::new("damage causes: no loader"))?;
    let mut codes = vec![0u32; DamageCause::NAMED.len()];
    loop {
        let mut count = 0u32;
        let capacity = u32::try_from(codes.len()).unwrap_or(u32::MAX);
        let status = (host.api().damage_causes_off)(
            host.handle(),
            codes.as_mut_ptr(),
            capacity,
            &raw mut count,
        );
        crate::load::ok(status, (), || "damage causes".to_owned())?;
        let count = count as usize;
        if count <= codes.len() {
            return Ok(codes[..count]
                .iter()
                .map(|&c| DamageCause::from_abi(c))
                .collect());
        }
        codes = vec![0u32; count];
    }
}

/// The free functions of [`damage`](self) as a script's namespace `damage`.
#[doc(hidden)]
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a unit marker, which never gains a field"
)]
pub struct DamageFns;

#[cfg(feature = "unstable")]
impl crate::script::Bound for DamageFns {
    const NAME: &'static str = "damage";
    const DOC: &'static str = "Damage filters, and the damage causes they do not get.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_DAMAGE_FNS];

    fn resolve(_load: &mut impl crate::Loading) -> Result<Self, Off> {
        Ok(Self)
    }
}

#[cfg(feature = "unstable")]
#[crate::script(damage_fns)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a free function reads nothing of it"
)]
impl DamageFns {
    /// The damage causes no damage filter gets on the running build, as
    /// [`causes_off`] lists them.
    ///
    /// # Errors
    ///
    /// As [`causes_off`].
    #[script]
    pub fn causes_off(self) -> Result<Vec<DamageCause>, Off> {
        causes_off()
    }
}

/// Damage about to reach an entity's health, for a filter registered with
/// [`Load::on_damage`](../struct.Load.html#method.on_damage) to see and change.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// See the [module](self).
#[cfg(feature = "unstable")]
#[derive(Debug)]
pub struct Damage<'a> {
    hit: &'a mut Hit,
}

/// The most damage types a filter reads; the game has 13 (server 26430253).
/// A hit's types past it reach the filter's caller unchanged.
#[cfg(feature = "unstable")]
const MAX_TYPES: usize = 64;

/// A [`DamageCheck`] with its amounts held by value, so a filter works on a
/// copy the loader takes back only once the filter returns.
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hit {
    cause: u32,
    target: Entity,
    source: Entity,
    attacker: Entity,
    total: f32,
    amounts: [f32; MAX_TYPES],
    /// How many of `amounts` the hit carries; 0 for other causes.
    count: usize,
}

#[cfg(feature = "unstable")]
impl Hit {
    /// A copy of `check` and the amounts it points at.
    ///
    /// # Safety
    ///
    /// For a hit, `check.amounts` is null or valid for reads of
    /// `check.amount_count` floats for the duration of the call.
    unsafe fn of(check: &DamageCheck) -> Self {
        let mut hit = Self {
            cause: check.cause,
            target: check.target,
            source: check.source,
            attacker: check.attacker,
            total: check.total,
            amounts: [0.0; MAX_TYPES],
            count: 0,
        };
        if check.cause == DamageCheck::IMPACT && !check.amounts.is_null() {
            hit.count = usize::try_from(check.amount_count)
                .unwrap_or(usize::MAX)
                .min(MAX_TYPES);
            // SAFETY: the caller guarantees `amounts` is valid for
            // `amount_count` floats, and `count` is at most that.
            let given = unsafe { core::slice::from_raw_parts(check.amounts, hit.count) };
            hit.amounts[..hit.count].copy_from_slice(given);
        }
        hit
    }

    /// Writes what a filter changed back into `check`.
    ///
    /// # Safety
    ///
    /// As [`Hit::of`] for the same `check`, and the amounts are valid for
    /// writes too.
    unsafe fn write_to(&self, check: &mut DamageCheck) {
        check.total = self.total;
        if self.count > 0 {
            // SAFETY: `count` came from this `check` through `Hit::of`, and
            // the caller guarantees its amounts are valid for writes.
            let given = unsafe { core::slice::from_raw_parts_mut(check.amounts, self.count) };
            given.copy_from_slice(self.amounts());
        }
    }

    fn amounts(&self) -> &[f32] {
        &self.amounts[..self.count]
    }

    fn amounts_mut(&mut self) -> &mut [f32] {
        &mut self.amounts[..self.count]
    }
}

/// The index of healing, which the totals leave out.
#[cfg(feature = "unstable")]
const HEALING: usize = DamageTypeNew::Healing.index();

#[cfg(feature = "unstable")]
impl Damage<'_> {
    /// What deals it.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn cause(&self) -> DamageCause {
        DamageCause::from_abi(self.hit.cause)
    }

    /// The entity that takes it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Entity 2 takes no damage at all.
    ///     load.on_damage(|damage| {
    ///         if damage.target().get() == 2 {
    ///             damage.cancel();
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub const fn target(&self) -> Entity {
        self.hit.target
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
        if self.hit.source.is_none() {
            None
        } else {
            Some(self.hit.source)
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
    ///         if damage.attacker().is_some_and(|a| a.get() == 7) {
    ///             damage.scale(2.0);
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub const fn attacker(&self) -> Option<Entity> {
        if self.hit.attacker.is_none() {
            self.source()
        } else {
            Some(self.hit.attacker)
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
    pub fn amount(&self, ty: DamageTypeNew) -> f32 {
        self.amount_at(ty.index())
    }

    /// Sets a hit's damage of type `ty`; does nothing for other causes. A
    /// negative or not finite amount counts as none.
    ///
    /// # Examples
    ///
    /// See [`Damage::amount`].
    pub fn set(&mut self, ty: DamageTypeNew, amount: f32) {
        self.set_at(ty.index(), amount);
    }

    /// How many damage types a hit carries on the running game build, which
    /// may have more than [`DamageTypeNew`] names; 0 for other causes.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // A damage type newer than this mod deals nothing.
    ///     load.on_damage(|damage| {
    ///         for index in enshwick::damage::DamageTypeNew::ALL.len()..damage.type_count() {
    ///             damage.set_at(index, 0.0);
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub const fn type_count(&self) -> usize {
        self.hit.count
    }

    /// A hit's damage of the type at `index`, in the game's order of its
    /// damage types, from 0; 0 for an index past
    /// [`type_count`](Self::type_count) and for other causes.
    ///
    /// # Examples
    ///
    /// See [`Damage::type_count`].
    #[must_use]
    pub fn amount_at(&self, index: usize) -> f32 {
        self.hit.amounts().get(index).copied().unwrap_or(0.0)
    }

    /// Sets a hit's damage of the type at `index`, as [`Damage::set`] does
    /// for a named type; does nothing for an index past
    /// [`type_count`](Self::type_count) and for other causes.
    ///
    /// # Examples
    ///
    /// See [`Damage::type_count`].
    pub fn set_at(&mut self, index: usize, amount: f32) {
        if let Some(slot) = self.hit.amounts_mut().get_mut(index) {
            *slot = amount;
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
        if self.hit.cause == DamageCheck::IMPACT {
            self.hit
                .amounts()
                .iter()
                .enumerate()
                .filter(|&(index, _)| index != HEALING)
                .map(|(_, amount)| amount)
                .sum()
        } else {
            self.hit.total
        }
    }

    /// Scales the damage by `factor`: every type of a hit but healing, or
    /// the amount of another cause.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn scale(&mut self, factor: f32) {
        if self.hit.cause == DamageCheck::IMPACT {
            for (index, amount) in self.hit.amounts_mut().iter_mut().enumerate() {
                if index != HEALING {
                    *amount *= factor;
                }
            }
        } else {
            self.hit.total *= factor;
        }
    }

    /// Scales a hit's damage of type `ty` by `factor`; does nothing for
    /// other causes.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn scale_type(&mut self, ty: DamageTypeNew, factor: f32) {
        if let Some(amount) = self.hit.amounts_mut().get_mut(ty.index()) {
            *amount *= factor;
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
    // SAFETY: for a hit the loader points `amounts` at `amount_count` floats
    // it owns for the duration of this call.
    let mut hit = unsafe { Hit::of(check) };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let done = crate::host::guard(
        "panic in a damage filter",
        || false,
        || {
            filter(&mut Damage { hit: &mut hit });
            true
        },
    );
    if done {
        // SAFETY: the damage filter contract of `HostApi::damage_filter`: for
        // a hit the loader lends `amount_count` floats at `amounts`, for
        // reads and writes, for the duration of this call.
        unsafe { hit.write_to(check) };
    }
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
#[cfg(feature = "unstable")]
pub trait LoadMethods {
    /// As `Load::on_damage`.
    #[doc(hidden)]
    fn on_damage<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut Damage<'_>) + Send + Sync + 'static;
}

#[cfg(feature = "unstable")]
impl LoadMethods for crate::BaseLoad {
    fn on_damage<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut Damage<'_>) + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().damage_filter)(host.handle(), Some(run_damage::<F>), user),
            "damage filter",
        )
    }
}

#[cfg(test)]
#[cfg(feature = "unstable")]
mod tests {
    use super::*;

    #[test]
    fn every_cause_the_api_names_is_in_named() {
        let named: Vec<DamageCause> = (0..=u32::from(u8::MAX))
            .map(DamageCause::from_abi)
            .filter(|cause| !matches!(cause, DamageCause::Other(_)))
            .collect();
        assert_eq!(named, DamageCause::NAMED);
    }
}
