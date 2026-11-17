//! What the fake loader of `enshwick-fake` reads inside this crate; only
//! the `fake` feature, which core's turns on, builds it.

/// The most markers one mod places on the map.
pub const MARKER_LIMIT: usize = crate::ui::MARKER_LIMIT;

/// Every one of the game's screens by name, the open-play screens a mod may
/// not extend among them.
pub const SCREENS: &[&str] = &crate::ui::screens::NAMES;

/// The placement of the raw values a screen extension's spec carries, as
/// the loader reads them; `None` for an edge or alignment the ABI lacks.
#[cfg(feature = "unstable")]
#[must_use]
pub fn placement(
    edge: u32,
    align: u32,
    shift: f32,
    inset: f32,
) -> Option<crate::ui::PanelPlacement> {
    crate::ui::PanelPlacement::from_raw(edge, align, shift, inset)
}
