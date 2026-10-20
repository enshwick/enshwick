//! The Enshwick loader, `mods/enshwick/enshwick.dll`.
//!
//! The `powrprof.dll` stub starts it on its own thread. It identifies the game
//! build, loads every mod in `mods/`, resolves their dependencies, initialises
//! them, arms their hook chains and answers their calls through the API table.
//! A game build it does not know leaves the game vanilla, unless a mod holds
//! the game for a world vanilla cannot open.

pub mod advertise;
#[cfg(windows)]
mod altars;
#[cfg(windows)]
mod bases;
#[cfg(windows)]
mod buffs;
pub mod builds;
pub mod chains;
#[cfg(windows)]
mod chat;
#[cfg(windows)]
mod clones;
#[cfg(windows)]
mod code;
pub mod configure;
#[cfg(windows)]
mod content;
#[cfg(windows)]
mod crash;
#[cfg(windows)]
mod damage;
#[cfg(windows)]
mod daytime;
#[cfg(windows)]
mod detour;
pub mod diag;
#[cfg(windows)]
mod ecs;
#[cfg(windows)]
mod emits;
#[cfg(windows)]
mod entities;
#[cfg(windows)]
mod events;
#[cfg(windows)]
mod game_events;
#[cfg(windows)]
mod game_settings;
pub mod holds;
#[cfg(windows)]
mod host;
#[cfg(windows)]
mod interactions;
#[cfg(windows)]
mod inventory;
#[cfg(windows)]
mod item_data;
pub mod join;
#[cfg(windows)]
mod knowledge;
#[cfg(windows)]
mod label;
pub mod log;
pub mod manifest;
#[cfg(windows)]
mod mid;
pub mod modlist;
#[cfg(windows)]
mod net;
#[cfg(windows)]
mod notices;
#[cfg(windows)]
mod patch;
#[cfg(windows)]
mod progress;
pub mod remote;
pub mod resolve;
#[cfg(windows)]
mod saves;
pub mod settings;
#[cfg(windows)]
mod spawns;
#[cfg(windows)]
mod start;
#[cfg(windows)]
mod stats;
#[cfg(windows)]
mod status;
#[cfg(windows)]
mod survivors;
#[cfg(windows)]
mod terrain;
#[cfg(windows)]
mod textures;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod vetoes;
#[cfg(windows)]
mod watch;
#[cfg(windows)]
mod weather;
#[cfg(windows)]
mod world;
#[cfg(windows)]
mod writes;

/// Starts the loader; called once by the `powrprof.dll` stub.
///
/// # Safety
///
/// Called by the stub, once, outside the Windows loader lock.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn enshwick_start(stub_abi: u32, missing_forwards: u32) {
    let _outcome = std::panic::catch_unwind(|| start::start(stub_abi, missing_forwards));
}
