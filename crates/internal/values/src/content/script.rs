//! Ids and GUIDs as a script sees them: text.

use super::{Guid, Id, is_id};
use crate::script::{Error, FromScript, IntoScript, Ty, Typed, Value};

impl Typed for Guid {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for Guid {
    fn into_value(self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromScript for Guid {
    fn from_value(value: Value) -> Result<Self, Error> {
        let text = String::from_value(value)?;
        Self::try_parse(&text).ok_or_else(|| Error::new(format!("{text:?} is no GUID")))
    }
}

impl Typed for Id {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for Id {
    fn into_value(self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromScript for Id {
    fn from_value(value: Value) -> Result<Self, Error> {
        let text = String::from_value(value)?;
        if !is_id(&text) {
            return Err(Error::new(format!(
                "{text:?} is no content id, <mod>:<name>"
            )));
        }
        let text = ScriptIds::keep(text);
        Self::try_new(text).ok_or_else(|| Error::new(format!("{text:?} is no content id")))
    }
}

/// The content id texts scripts passed, each kept once for the process, as
/// a pack's are: a handler that names an id on every event keeps no more.
struct ScriptIds(rustc_hash::FxHashSet<&'static str>);

static SCRIPT_IDS: std::sync::Mutex<ScriptIds> = std::sync::Mutex::new(ScriptIds(
    rustc_hash::FxHashSet::with_hasher(rustc_hash::FxBuildHasher),
));

impl ScriptIds {
    /// `text` for the process, the copy kept before when there is one.
    fn keep(text: String) -> &'static str {
        // The API's `Relock` logs through the loader, above this crate; the
        // set stays whole whatever panicked, its one write being an insert.
        // ast-grep-ignore: lock-through-relock
        let mut ids = SCRIPT_IDS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(&kept) = ids.0.get(text.as_str()) {
            return kept;
        }
        let kept: &'static str = Box::leak(text.into_boxed_str());
        ids.0.insert(kept);
        kept
    }
}
