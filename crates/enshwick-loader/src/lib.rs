//! The Enshwick loader, `mods/enshwick/enshwick.dll`.
//!
//! The `powrprof.dll` stub starts it on its own thread. It identifies the game
//! build, loads every mod in `mods/`, resolves their dependencies, initialises
//! them, arms their hook chains and answers their calls through the API table.
//! A game build it does not know leaves the game vanilla.

pub mod advertise;
pub mod builds;
pub mod chains;
#[cfg(windows)]
mod crash;
#[cfg(windows)]
mod detour;
#[cfg(windows)]
mod events;
#[cfg(windows)]
mod host;
pub mod join;
pub mod log;
pub mod manifest;
#[cfg(windows)]
mod patch;
pub mod resolve;
pub mod settings;
#[cfg(windows)]
mod start;
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
