//! Loot as a script sees it: container kinds by their constants' names.

use super::Kinds;
use crate::script::{Error, FromScript, IntoScript, Ty, Typed, Value};

/// Every kind by its constant's name, for a script, which names the kinds
/// it means.
const KIND_NAMES: [(&str, Kinds); 3] = [
    ("CORPSES", Kinds::CORPSES),
    ("CHESTS", Kinds::CHESTS),
    ("DESTROYED", Kinds::DESTROYED),
];

impl Typed for Kinds {
    const TY: &'static Ty = &Ty::List(&Ty::String);
}

impl IntoScript for Kinds {
    fn into_value(self) -> Value {
        Value::List(
            KIND_NAMES
                .iter()
                .filter(|(_, kind)| self.contains(*kind))
                .map(|(name, _)| Value::String((*name).to_owned()))
                .collect(),
        )
    }
}

impl FromScript for Kinds {
    fn from_value(value: Value) -> Result<Self, Error> {
        let names: Vec<String> = FromScript::from_value(value)?;
        names.iter().try_fold(Self::default(), |kinds, name| {
            KIND_NAMES
                .iter()
                .find(|(known, _)| known == name)
                .map(|&(_, kind)| kinds.with(kind))
                .ok_or_else(|| Error::new(format!("{name:?} is no kind of loot container")))
        })
    }
}

/// `Load`'s script-only method of this module on the base handle, which
/// core's forwarder calls; a mod reaches it only through the script.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_loot_pass_script`.
    #[doc(hidden)]
    fn on_loot_pass_script(
        &mut self,
        handler: impl Fn(Vec<super::Container>) + Send + Sync + 'static,
    ) -> Result<(), crate::Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn on_loot_pass_script(
        &mut self,
        handler: impl Fn(Vec<super::Container>) + Send + Sync + 'static,
    ) -> Result<(), crate::Off> {
        use super::LoadMethods as _;
        self.on_loot_pass(move |pass| {
            let mut containers = Vec::new();
            pass.each(|container| containers.push(*container));
            handler(containers);
        })
    }
}
