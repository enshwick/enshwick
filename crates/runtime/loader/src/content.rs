//! The content domain: served resources, content changes, rates, recipes, item
//! kinds and textures.

#[cfg(windows)]
pub(crate) mod clones;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the running game takes mods' renames")
)]
pub(crate) mod content_renames;
pub mod data_changes;
pub mod item_kinds;
#[cfg(windows)]
pub(crate) mod rate_links;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game applies rates")
)]
pub(crate) mod rates;
pub mod recipes;
#[cfg(windows)]
pub(crate) mod served;
#[cfg(windows)]
pub(crate) mod standins;
#[cfg(windows)]
pub(crate) mod textures;
