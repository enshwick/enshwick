//! What the fake loader of `enshwick-fake` reads inside this crate, and
//! the one check it hands down; only the `fake` feature, which only that
//! crate turns on, builds it.

use std::sync::OnceLock;

pub use enshwick_base::__fake::*;
pub use enshwick_content::__fake::*;
pub use enshwick_world::__fake::*;

pub use crate::__private::rules;
use crate::abi::{HostApi, Manifest};
#[cfg(feature = "unstable")]
use crate::actions::ActionTarget;

/// The most markers one mod places on the map.
pub const MARKER_LIMIT: usize = crate::ui::MARKER_LIMIT;

/// Every one of the game's screens by name, the open-play screens a mod may
/// not extend among them.
pub const SCREENS: &[&str] = &crate::ui::screens::NAMES;

/// Why the fake refuses the manifest of a mod whose init runs on a table,
/// or `None` when it takes it.
pub type ManifestCheck = fn(*const HostApi, &Manifest) -> Option<String>;

// The fake sits above this crate, so it hands its check down when it
// installs; the first one stays.
static MANIFEST_CHECK: OnceLock<ManifestCheck> = OnceLock::new();

/// Asks `check` about every mod's manifest before its init from now on;
/// the first call wins.
pub fn check_manifests(check: ManifestCheck) {
    MANIFEST_CHECK.get_or_init(|| check);
}

/// Why the fake's check refuses `manifest` on `api`; `None` when it takes
/// it or no fake installed one.
pub(crate) fn refused(api: *const HostApi, manifest: &Manifest) -> Option<String> {
    MANIFEST_CHECK.get().and_then(|check| check(api, manifest))
}

/// The action target `raw` names, as the safe layer reads it from the
/// loader; `None` for none.
#[cfg(feature = "unstable")]
#[must_use]
pub fn action_target(raw: &crate::abi::ActionTarget) -> Option<ActionTarget> {
    ActionTarget::from_raw(raw)
}

/// `target` as the safe layer hands it to the loader.
#[cfg(feature = "unstable")]
#[must_use]
pub fn raw_action_target(target: ActionTarget) -> crate::abi::ActionTarget {
    target.to_raw()
}
