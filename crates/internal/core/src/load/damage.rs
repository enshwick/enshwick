//! `Load`'s methods of a module below core, forwarded to the base handle.

#[cfg(feature = "unstable")]
use crate::Entity;
#[cfg(feature = "unstable")]
use crate::abi::DamageCheck;
#[cfg(feature = "unstable")]
use crate::damage::Damage;
#[cfg(feature = "unstable")]
use crate::{Load, Off};
#[cfg(feature = "unstable")]
use core::ffi::c_void;
#[cfg(feature = "unstable")]
use enshwick_inventory::__internal::ScriptDamage;
use enshwick_inventory::__load::*;

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Runs `filter` on damage about to reach an entity's health in the
    /// server's world: hits by type, falls and the shroud. What it leaves is
    /// what the game applies. It runs on the game's worker threads, inside
    /// the systems that deal damage, in the dedicated server or a hosting
    /// client: it must not allocate, lock or log.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the damage sites.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::damage::{DamageCause, DamageTypeNew};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_damage(|damage| match damage.cause() {
    ///         // Falls hurt half as much, fire twice as much.
    ///         DamageCause::FallDamage => damage.scale(0.5),
    ///         DamageCause::Impact => damage.scale_type(DamageTypeNew::MagicalFire, 2.0),
    ///         _ => {}
    ///     })
    /// }
    /// ```
    pub fn on_damage<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut Damage<'_>) + Send + Sync + 'static;
}

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
#[crate::script(damage_filter)]
impl Load<'_> {
    /// Runs `filter` on damage about to reach an entity's health, as
    /// [`Load::on_damage`] does: it gets the damage as an object whose
    /// methods change it.
    ///
    /// # Errors
    ///
    /// As [`Load::on_damage`].
    #[script(name = "on_damage")]
    pub(crate) fn on_damage_script(
        &mut self,
        filter: impl Fn(std::sync::Arc<ScriptDamage>) + Send + Sync + 'static,
    ) -> Result<(), Off>;
}
