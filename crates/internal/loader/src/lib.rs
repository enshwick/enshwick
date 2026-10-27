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

#[cfg(windows)]
mod actions;
pub mod admission;
pub mod advertise;
#[cfg(windows)]
mod altars;
pub mod archive;
pub mod attribute_slots;
#[cfg(windows)]
mod bases;
#[cfg(windows)]
mod buffs;
pub mod builds;
pub mod carry;
#[cfg(windows)]
mod catalogue;
pub mod chains;
#[cfg(windows)]
mod chat;
#[cfg(windows)]
mod clones;
#[cfg(windows)]
mod code;
pub mod code_watch;
pub mod configure;
pub mod conflicts;
pub mod console;
#[cfg(windows)]
mod content;
#[cfg(windows)]
mod crash;
#[cfg(windows)]
mod damage;
pub mod data_changes;
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
pub mod errors;
#[cfg(windows)]
mod events;
pub mod features;
#[cfg(windows)]
mod fog;
pub mod fog_image;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the game's features run behind fuses")
)]
mod fuse;
#[cfg(windows)]
mod game_events;
#[cfg(windows)]
mod game_settings;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game's texts are read")
)]
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
pub mod item_kinds;
pub mod join;
#[cfg(windows)]
mod knowledge;
#[cfg(windows)]
mod label;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the client's join reads lobbies")
)]
mod lobby;
mod locks;
pub mod log;
pub mod manifest;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows build reads game memory")
)]
mod memory;
#[cfg(windows)]
mod mid;
pub mod migrations;
pub mod mod_index;
pub mod modlist;
#[cfg(windows)]
mod module;
#[cfg(windows)]
mod net;
#[cfg(windows)]
mod notices;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "used by the start, which runs on Windows")
)]
mod others;
#[cfg(windows)]
mod pacing;
#[cfg(windows)]
mod packs;
#[cfg(windows)]
mod pages;
#[cfg(windows)]
mod patch;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the loader's start reads executables")
)]
mod pe;
#[cfg_attr(
    not(windows),
    expect(
        dead_code,
        reason = "most ids name engine code only the Windows build reaches"
    )
)]
mod pinned;
#[cfg(windows)]
mod points;
#[cfg(target_arch = "x86_64")]
pub mod profile;
#[cfg(windows)]
mod progress;
#[cfg(windows)]
mod rate_links;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game applies rates")
)]
mod rates;
pub mod recipes;
pub mod registry;
#[cfg(windows)]
mod reload;
pub mod remote;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "written from the start, which runs on Windows")
)]
mod report;
pub mod resolve;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build hands the game memory")
)]
mod retire;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build asks the game's zones")
)]
mod rights;
#[cfg(windows)]
mod saves;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build reads the game's server")
)]
mod server_pin;
#[cfg(windows)]
mod services;
pub mod settings;
#[cfg(windows)]
mod spawns;
#[cfg(windows)]
mod standins;
#[cfg(windows)]
mod start;
#[cfg(windows)]
pub use start::{DryMod, DryRun, DryRunError, dry_run};
#[cfg(windows)]
mod stats;
#[cfg(windows)]
mod status;
#[cfg(windows)]
mod survivors;
#[cfg(windows)]
mod table_ids;
#[cfg(windows)]
pub use table_ids::TABLE_IDS;
#[cfg(windows)]
mod terrain;
mod texts;
#[cfg(windows)]
mod textures;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build writes the game's voxels")
)]
mod transactions;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod vetoes;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build reads the game's voxels")
)]
mod voxels;
#[cfg(windows)]
mod watch;
#[cfg(windows)]
mod weather;
pub mod wire;
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
    let _outcome = std::panic::catch_unwind(|| start::start(stub_abi, missing_forwards, release));
}
