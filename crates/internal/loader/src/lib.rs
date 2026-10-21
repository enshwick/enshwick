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
pub mod archive;
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
pub mod conflicts;
#[cfg(windows)]
mod content;
#[cfg(windows)]
mod crash;
#[cfg(windows)]
mod damage;
#[cfg(windows)]
mod daytime;
#[cfg(windows)]
mod deaths;
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
mod game_texts;
pub mod holds;
#[cfg(windows)]
mod host;
#[cfg(windows)]
mod ids;
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
pub mod migrations;
pub mod modlist;
#[cfg(windows)]
mod net;
#[cfg(windows)]
mod notices;
#[cfg(windows)]
mod packs;
#[cfg(windows)]
mod patch;
#[cfg(target_arch = "x86_64")]
pub mod profile;
#[cfg(windows)]
mod progress;
#[cfg(windows)]
mod reload;
pub mod remote;
pub mod resolve;
#[cfg(windows)]
mod saves;
#[cfg(windows)]
mod services;
pub mod settings;
#[cfg(windows)]
mod spawns;
#[cfg(windows)]
mod standins;
mod start;
#[cfg(windows)]
pub use start::{DryMod, DryRun, dry_run};
#[cfg(windows)]
mod stats;
#[cfg(windows)]
mod status;
#[cfg(windows)]
mod survivors;
#[cfg(windows)]
mod terrain;
mod texts;
#[cfg(windows)]
mod textures;
#[cfg(windows)]
mod ui;
mod updates;
#[cfg(windows)]
mod vetoes;
#[cfg(windows)]
mod watch;
#[cfg(windows)]
mod weather;
#[cfg(windows)]
mod world;
pub mod worlds;
#[cfg(windows)]
mod writes;

/// Starts the loader; called once by the `powrprof.dll` stub, which holds
/// the dedicated server's own code until `release` is called.
///
/// # Safety
///
/// Called by the stub, once, outside the Windows loader lock; `release` is
/// valid from a stub of contract 2 or later.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn enshwick_start(
    stub_abi: u32,
    missing_forwards: u32,
    release: extern "system" fn() -> u32,
) {
    let release = (stub_abi >= 2).then_some(release);
    let _outcome = std::panic::catch_unwind(|| start::start(stub_abi, missing_forwards, release));
}
