//! What the fake loader of `enshwick-fake` reads inside this crate; only
//! the `fake` feature, which core's turns on, builds it.

/// The most markers one mod places on the map.
pub const MARKER_LIMIT: usize = crate::ui::MARKER_LIMIT;

/// Every one of the game's screens by name, the open-play screens a mod may
/// not extend among them.
pub const SCREENS: &[&str] = &crate::ui::screens::NAMES;
