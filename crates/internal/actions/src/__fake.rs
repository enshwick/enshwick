//! What the fake loader of `enshwick-fake` reads inside this crate; only
//! the `fake` feature, which core's turns on, builds it.

use crate::actions::ActionTarget;

/// The action target `raw` names, as the safe layer reads it from the
/// loader; `None` for none.
#[must_use]
pub fn action_target(raw: &crate::abi::ActionTarget) -> Option<ActionTarget> {
    ActionTarget::from_raw(raw)
}

/// `target` as the safe layer hands it to the loader.
#[must_use]
pub fn raw_action_target(target: ActionTarget) -> crate::abi::ActionTarget {
    target.to_raw()
}
