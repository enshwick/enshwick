//! The Enshwick loader, `mods/enshwick/enshwick.dll`.
//!
//! The `powrprof.dll` stub starts it on its own thread. It identifies the game
//! build, loads every mod in `mods/`, resolves their dependencies, initialises
//! them, arms their hook chains and answers their calls through the API table.
//! A game build it does not know leaves the game vanilla, unless a mod holds
//! the game for a world vanilla cannot open.

// Address sanitizer runs of the tests set `enshwick_asan` on a nightly
// compiler, to leave the walks over other threads' stacks unchecked.
#![cfg_attr(enshwick_asan, feature(sanitize))]

// The layers, lowest first: the engine, then the domains in the order of
// `crates/internal/core/domains.toml`, which its `[loader]` table holds.
pub mod actions;
pub mod base;
pub mod content;
pub mod engine;
pub mod inventory;
mod knowledge;
mod packs;
pub mod ui;
pub mod world;

#[cfg(windows)]
mod host;
#[cfg(windows)]
mod start;
#[cfg(windows)]
mod table_ids;

#[cfg(windows)]
#[doc(hidden)]
pub use engine::pages::{forget as forget_read_only, read_only};
#[cfg(windows)]
pub use start::{DryMod, DryRun, DryRunError, dry_run};
#[cfg(windows)]
pub use table_ids::TABLE_IDS;

/// Starts the loader; called once by the `powrprof.dll` stub, which holds
/// the dedicated server's own code until `release` is called.
///
/// # Safety
///
/// - Called by the stub, once per process.
/// - Called outside the Windows loader lock.
/// - When `stub_abi` is 2 or more, `release` is the stub's function that
///   lets the server's own code go on; below 2 it is never called.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn enshwick_start(
    stub_abi: u32,
    missing_forwards: u32,
    release: extern "system" fn() -> u32,
) {
    let release = (stub_abi >= 2).then_some(release);
    if let Err(payload) =
        std::panic::catch_unwind(|| start::start(stub_abi, missing_forwards, release))
    {
        engine::early::note(&format!(
            "the loader's start panicked: {}; mods that did not start stay off",
            engine::early::panic_text(payload.as_ref())
        ));
    }
}
