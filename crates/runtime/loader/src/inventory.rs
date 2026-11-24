//! The inventory domain: inventories, item data and records, stats, damage,
//! survivors, deaths and loot.

#[cfg(windows)]
pub(crate) mod containers;
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
#[cfg(windows)]
pub(crate) mod item_names;
pub mod item_records;
#[cfg(windows)]
pub(crate) mod stacks;
#[cfg(windows)]
pub(crate) mod stats;
#[cfg(windows)]
pub(crate) mod survivors;
#[cfg(windows)]
pub(crate) mod transactions;

#[cfg(windows)]
use crate::engine::parts::{EventFollower, Part, ServerLink, fused, part, step};

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
        end: Some(|_| {
            transactions::end();
        }),
        seal: Some(|_| {
            crate::content::item_info::on_lookup(crate::content::item_info::Lookup {
                at: transactions::item_info_at,
                info: transactions::item_info,
            });
        }),
        ..part!("inventory", transactions)
    },
    &Part {
        table_ids: containers::TABLE_IDS,
        seal: Some(|_| containers::seal()),
        link: Some(containers::link),
        ..part!("containers", containers)
    },
    &Part {
        table_ids: damage::TABLE_IDS,
        seal: Some(|_| damage::seal()),
        link: Some(damage::link),
        ..part!("damage", damage)
    },
    &Part {
        table_ids: deaths::TABLE_IDS,
        link: Some(deaths::link),
        steps: &[step!(Follow, "death steps", |s| deaths::after_step(
            s.loader
        ))],
        forget: Some(|_| {
            deaths::forget();
        }),
        ..part!("deaths", deaths)
    },
    &Part {
        seal: Some(|_| item_actions::seal()),
        link: Some(item_actions::link),
        game_events: Some(item_actions::FOLLOWER),
        forget: Some(|_| {
            item_actions::forget();
        }),
        ..part!("item_actions", item_actions)
    },
    &Part {
        table_ids: survivors::TABLE_IDS,
        seal: Some(|_| survivors::seal()),
        link: Some(survivors::link),
        steps: &[step!(Follow, "survivor steps", |s| survivors::after_step(
            s.loader
        ))],
        forget: Some(|_| {
            survivors::forget();
        }),
        ..part!("survivors", survivors)
    },
    &Part {
        table_ids: stats::TABLE_IDS,
        seal: Some(|_| stats::seal()),
        joined: Some(fused!("stats at join", |loader, player| {
            stats::player_joined(loader, player);
        })),
        hello: Some(fused!("stats at hello", |_, key| stats::hello(key))),
        world_event: Some(fused!("stats at world events", |loader, event| {
            stats::world_event(loader, event);
        })),
        follows: Some(|_| stats::wanted()),
        // While a mod uses stats, a player's equipment changing makes every
        // watched entity look again.
        game_events: Some(EventFollower {
            types: |loader| {
                let id = crate::engine::catalogue::ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT;
                stats::wanted()
                    .then(|| crate::engine::catalogue::component_hash(loader, id))
                    .flatten()
                    .map(|hash| vec![hash])
            },
            screen: None,
            follow: Some(|_, _| stats::equipment_changed()),
            fuse: None,
        }),
        forget: Some(|_| stats::forget()),
        ..part!("stats", stats)
    },
    &Part {
        table_ids: item_data::TABLE_IDS,
        seal: Some(|_| {
            item_data::seal();
            crate::base::net::on_control(crate::base::wire::ITEM_DATA, |_, records| {
                item_data::received(records);
            });
        }),
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
        forget: Some(|_| {
            item_data::forget_server();
        }),
        server_link: Some(|link| {
            if link == ServerLink::Forgotten {
                item_data::forget_client();
            }
        }),
        handover: Some(|key| {
            item_data::hello()
                .iter()
                .all(|records| crate::base::net::item_data(key, records))
        }),
        ..part!("item_data", item_data)
    },
    &Part {
        steps: &[step!(Follow, "stat steps", |s| stats::after_step(
            s.loader, s.step, s.now
        ))],
        ..part!("stat steps", stats)
    },
    &Part {
        link: Some(drops::link),
        hosting: Some(|loader| {
            drops::start(loader);
        }),
        ..part!("drops", drops)
    },
];
