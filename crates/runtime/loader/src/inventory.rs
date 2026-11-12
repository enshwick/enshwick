//! The inventory domain: inventories, item data and records, stats, damage,
//! survivors, deaths and loot.

#[cfg(windows)]
pub(crate) mod damage;
#[cfg(windows)]
pub(crate) mod deaths;
#[cfg(windows)]
pub(crate) mod drops;
#[cfg(windows)]
pub(crate) mod item_actions;
#[cfg(windows)]
pub(crate) mod item_data;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the client's interface lists items")
)]
pub(crate) mod item_names;
pub mod item_records;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows build links the loot rate")
)]
pub(crate) mod loot_slot;
#[cfg(windows)]
pub(crate) mod spawns;
#[cfg(windows)]
pub(crate) mod stats;
#[cfg(windows)]
pub(crate) mod survivors;
#[cfg(windows)]
pub(crate) mod transactions;
