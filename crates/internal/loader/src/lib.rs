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
pub mod builds;
pub mod chains;
#[cfg(windows)]
mod chat;
#[cfg(windows)]
mod clones;
#[cfg(windows)]
mod code;
#[cfg(windows)]
mod content;
#[cfg(windows)]
mod crash;
#[cfg(windows)]
mod detour;
pub mod diag;
#[cfg(windows)]
mod ecs;
#[cfg(windows)]
mod events;
pub mod holds;
#[cfg(windows)]
mod host;
#[cfg(windows)]
mod interactions;
#[cfg(windows)]
mod inventory;
pub mod join;
#[cfg(windows)]
mod label;
pub mod log;
pub mod manifest;
#[cfg(windows)]
mod mid;
#[cfg(windows)]
mod patch;
pub mod resolve;
pub mod settings;
#[cfg(windows)]
mod start;
#[cfg(windows)]
mod status;
#[cfg(windows)]
mod survivors;
#[cfg(windows)]
mod watch;
#[cfg(windows)]
mod world;

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
