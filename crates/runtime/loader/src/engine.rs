//! The hooking engine and raw game access every domain stands on: detours,
//! mid-function hooks, code patches, the build's symbols, memory reads, crash
//! reports, the loader's log, texts and settings.

pub mod archive;
pub mod attribute_slots;
pub mod builds;
#[cfg(windows)]
pub(crate) mod catalogue;
pub mod chains;
#[cfg(windows)]
pub(crate) mod code;
pub mod code_watch;
pub mod conflicts;
#[cfg(windows)]
pub(crate) mod crash;
#[cfg(windows)]
pub(crate) mod detour;
pub mod diag;
pub mod early;
#[cfg(windows)]
pub(crate) mod ecs;
pub mod errors;
pub mod features;
pub(crate) mod filters;
pub mod fuse;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game's texts are read")
)]
pub(crate) mod game_texts;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build draws the interface")
)]
pub(crate) mod interface_parts;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game hands mods kept text")
)]
pub(crate) mod kept_text;
#[cfg(windows)]
pub(crate) mod live;
#[cfg(windows)]
pub(crate) mod loader;
pub mod log;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows build reads game memory")
)]
pub(crate) mod memory;
#[cfg(windows)]
pub(crate) mod mid;
pub mod mod_index;
#[cfg(windows)]
pub(crate) mod module;
pub(crate) mod off_thread;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "used by the start, which runs on Windows")
)]
pub(crate) mod others;
#[cfg(windows)]
pub(crate) mod pacing;
#[cfg(windows)]
pub(crate) mod pages;
#[cfg(windows)]
pub(crate) mod parts;
#[cfg(windows)]
pub(crate) mod patch;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the loader's start reads executables")
)]
pub(crate) mod pe;
pub mod pending;
pub(crate) mod per_test;
#[cfg_attr(
    not(windows),
    expect(
        dead_code,
        reason = "most ids name engine code only the Windows build reaches"
    )
)]
pub(crate) mod pinned;
pub(crate) mod players;
#[cfg(target_arch = "x86_64")]
pub mod profile;
pub mod registry;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build hands the game memory")
)]
pub(crate) mod retire;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game tries code patches again")
)]
pub(crate) mod retry;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build reads the game's server")
)]
pub(crate) mod server_pin;
pub mod settings;
#[cfg(windows)]
pub(crate) mod shipped;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows build spawns entities")
)]
pub(crate) mod spawn_request;
#[cfg(windows)]
pub(crate) mod static_halves;
pub(crate) mod texts;
pub mod ticks;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the game's detours undo their writes")
)]
pub(crate) mod undo;
#[cfg(any(windows, test))]
pub(crate) mod unwind_info;
#[cfg(windows)]
pub(crate) mod world;

#[cfg(windows)]
use parts::{Part, part};

/// The engine's parts, in the order the joins call each of their phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        link: Some(crash::link),
        ..part!("crash", crash)
    },
    &Part {
        table_ids: world::TABLE_IDS,
        link: Some(world::link),
        ..part!("world", world)
    },
    &Part {
        table_ids: code::TABLE_IDS,
        ..part!("code", code)
    },
    &Part {
        table_ids: ecs::TABLE_IDS,
        ..part!("ecs", ecs)
    },
    &Part {
        table_ids: pacing::TABLE_IDS,
        ..part!("pacing", pacing)
    },
    &Part {
        table_ids: static_halves::TABLE_IDS,
        ..part!("static_halves", static_halves)
    },
];
