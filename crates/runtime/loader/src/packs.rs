//! The packs domain: data packs as mods.

#[cfg_attr(
    not(windows),
    expect(
        dead_code,
        reason = "only the Windows build loads packs; tests off it reach discovery and the hash"
    )
)]
pub(crate) mod data_packs;

/// The packs domain's parts; data packs load as mods, with nothing for the
/// joins to call.
#[cfg(windows)]
pub(crate) static PARTS: &[&crate::engine::parts::Part] = &[];
