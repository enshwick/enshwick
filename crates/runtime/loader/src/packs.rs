//! The packs domain: data packs as mods.

#[cfg(windows)]
pub(crate) mod data_packs;

/// The packs domain's parts; data packs load as mods, with nothing for the
/// joins to call.
#[cfg(windows)]
pub(crate) static PARTS: &[&crate::engine::parts::Part] = &[];
