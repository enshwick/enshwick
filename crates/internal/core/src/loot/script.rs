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
