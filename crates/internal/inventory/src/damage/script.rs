//! Damage as a script's filter sees it: a copy of the check with
//! [`Damage`]'s methods, which core's `Load` hands a script's filter.

use super::{Damage, DamageCause, DamageTypeNew};
use crate::Entity;
use crate::abi::DamageCheck;

/// The damage a script's filter sees and changes: a copy of the check,
/// written back once the filter returns, with [`Damage`]'s methods; one
/// per filter, filled again on every hit.
#[derive(Clone, Debug)]
pub struct ScriptDamage(pub(super) std::sync::Arc<std::sync::Mutex<DamageCheck>>);

impl ScriptDamage {
    /// Runs `with` on the copy as a [`Damage`].
    fn with<R>(&self, with: impl FnOnce(&mut Damage<'_>) -> R) -> R {
        use crate::Relock as _;
        let mut check = self.0.relock();
        with(&mut Damage { check: &mut check })
    }
}

impl crate::script::Object for ScriptDamage {
    const CLASS: &'static crate::script::Class = &crate::script::Class::lent::<Self>(
        "Damage",
        "Damage about to reach an entity's health, for a filter to see and change.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_DAMAGE];
}

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
