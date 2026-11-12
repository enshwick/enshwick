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
pub(crate) mod watch;
pub mod wire;
pub mod world_folders;
pub mod worlds;
