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

#[cfg(windows)]
use crate::engine::parts::{Part, fused, part, step};

/// The inventory domain's parts, in the order the joins call each of their
/// phases. Stats seal before item data, as they open an item data kind of
/// their own, and step after it, as its step attaches the item stats they
/// work out: their step is a part of its own.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        table_ids: transactions::TABLE_IDS,
        link: Some(transactions::link),
        steps: &[step!(Follow, "inventory deliveries", |s| {
            transactions::deliver(s.now);
        })],
        end: Some(fused!("inventory requests at a game's end", |_| {
            transactions::forget();
        })),
        ..part!("inventory", transactions)
    },
    &Part {
        table_ids: damage::TABLE_IDS,
        link: Some(damage::link),
        ..part!("damage", damage)
    },
    &Part {
        table_ids: deaths::TABLE_IDS,
        link: Some(deaths::link),
        steps: &[step!(Follow, "death steps", |s| deaths::after_step(
            s.loader
        ))],
        ..part!("deaths", deaths)
    },
    &Part {
        seal: Some(|_| item_actions::seal()),
        link: Some(item_actions::link),
        forget: Some(|_| item_actions::forget()),
        ..part!("item_actions", item_actions)
    },
    &Part {
        table_ids: survivors::TABLE_IDS,
        link: Some(survivors::link),
        steps: &[step!(Follow, "survivor steps", |s| survivors::after_step(
            s.loader
        ))],
        forget: Some(|_| survivors::forget()),
        ..part!("survivors", survivors)
    },
    &Part {
        table_ids: stats::TABLE_IDS,
        seal: Some(|_| stats::seal()),
        joined: Some(fused!("stats at join", |loader, player| {
            stats::player_joined(loader, player);
        })),
        world_event: Some(fused!("stats at world events", |loader, event| {
            stats::world_event(loader, event);
        })),
        follows: Some(|_| stats::wanted()),
        forget: Some(|_| stats::forget()),
        ..part!("stats", stats)
    },
    &Part {
        table_ids: item_data::TABLE_IDS,
        seal: Some(|_| item_data::seal()),
        link: Some(item_data::link),
        steps: &[step!(Follow, "item data steps", |s| item_data::after_step(
            s.loader
        ))],
        joined: Some(fused!("item data at join", |loader, player| {
            item_data::player_joined(loader, player.entity);
        })),
        left: Some(fused!("item data at leave", |loader, player| {
            item_data::player_left(loader, player.entity);
        })),
        world_event: Some(fused!("item data at world events", |loader, event| {
            item_data::world_event(loader, event);
        })),
        follows: Some(|_| item_data::wanted()),
        ..part!("item_data", item_data)
    },
    &Part {
        steps: &[step!(Follow, "stat steps", |s| stats::after_step(
            s.loader, s.step, s.now
        ))],
        ..part!("stat steps", stats)
    },
    &Part {
        world_event: Some(fused!("item drops at world events", |_, event| {
            drops::world_event(event);
        })),
        ..part!("drops", drops)
    },
    &Part {
        table_ids: spawns::TABLE_IDS,
        forget: Some(|_| spawns::forget()),
        ..part!("spawns", spawns)
    },
];
