//! What every domain shares: finding, admitting and joining mods, the wire
//! between server and clients, services, points, events and saves.

pub mod admission;
pub mod advertise;
#[cfg(windows)]
pub(crate) mod calls;
#[cfg(windows)]
pub(crate) mod carrier;
pub mod configure;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game reads the marker")
)]
pub(crate) mod developer_tools;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game discovers mods")
)]
pub(crate) mod discovery;
#[cfg(windows)]
pub(crate) mod emits;
#[cfg(windows)]
pub(crate) mod game_events;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "players read the greeting only in the game")
)]
pub(crate) mod greeting;
#[cfg(windows)]
pub(crate) mod handed;
pub mod holds;
#[cfg(windows)]
pub(crate) mod ids;
pub mod join;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the client's join reads lobbies")
)]
pub(crate) mod lobby;
pub mod manifest;
#[cfg(windows)]
pub(crate) mod net;
pub mod notice_queue;
#[cfg(windows)]
pub(crate) mod notices;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running server tells a joining player")
)]
pub(crate) mod off_line;
#[cfg(windows)]
pub(crate) mod points;
pub mod record;
pub mod releases;
pub mod remote;
pub mod resolve;
#[cfg(windows)]
pub(crate) mod saves;
#[cfg(windows)]
pub(crate) mod services;
#[cfg(windows)]
pub(crate) mod status;
#[cfg(windows)]
pub(crate) mod stop;
#[cfg(windows)]
pub(crate) mod watch;
pub mod wire;
pub mod world_folders;
pub mod worlds;

#[cfg(windows)]
use crate::engine::parts::{Part, fused, part, step};

/// The base layer's parts, in the order the joins call each of their phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        seal: Some(services::seal),
        ..part!("services", services)
    },
    &Part {
        seal: Some(points::seal),
        ..part!("points", points)
    },
    &Part {
        seal: Some(calls::seal),
        ..part!("calls", calls)
    },
    &Part {
        table_ids: game_events::TABLE_IDS,
        link: Some(game_events::link),
        ..part!("game_events", game_events)
    },
    &Part {
        table_ids: saves::TABLE_IDS,
        link: Some(saves::link),
        steps: &[step!(Output, "world saving", |s| saves::after_step(
            s.loader
        ))],
        hosting: Some(|_| saves::hosting_begins()),
        forget: Some(|_| saves::forget()),
        ..part!("saves", saves)
    },
    &Part {
        link: Some(|_| net::link()),
        left: Some(fused!("the wire at leave", |_, player| net::forget_peer(
            player.key.get()
        ))),
        forget: Some(|_| net::forget()),
        ..part!("net", net)
    },
    &Part {
        left: Some(fused!("remote edits at leave", |_, player| {
            remote::forget_player(player.key.get());
        })),
        ..part!("remote", remote)
    },
    &Part {
        table_ids: admission::TABLE_IDS,
        steps: &[step!(Input, "join admission at the step", |s| {
            admission::after_step(s.loader);
        })],
        joined: Some(fused!("join admission at join", |_, player| {
            admission::joined(player.key.get());
        })),
        left: Some(fused!("join admission at leave", |_, player| {
            admission::left(player.key.get());
        })),
        forget: Some(|_| admission::forget()),
        ..part!("admission", admission)
    },
    &Part {
        table_ids: carrier::TABLE_IDS,
        link: Some(carrier::link),
        steps: &[step!(Output, "own channel", |s| carrier::after_step(
            s.loader, s.now
        ))],
        ..part!("carrier", carrier)
    },
    &Part {
        joined: Some(fused!("the greeting at join", |loader, player| {
            greeting::joined(loader, player.key.get());
        })),
        ..part!("greeting", greeting)
    },
    &Part {
        hosting: Some(|_| join::client::back_on()),
        ..part!("join", join)
    },
    &Part {
        table_ids: emits::TABLE_IDS,
        ..part!("emits", emits)
    },
    &Part {
        table_ids: notices::TABLE_IDS,
        ..part!("notices", notices)
    },
    &Part {
        table_ids: stop::TABLE_IDS,
        ..part!("stop", stop)
    },
];
