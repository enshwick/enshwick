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

/// Fails when rustdoc builds this crate's examples one by one, which it does
/// without a word when one of them does not merge.
///
/// ```
/// assert_ne!(module_path!(), "rust_out", "an example does not merge: \
///     RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS='-Zunstable-options --merge-doctests=yes' \
///     cargo test --doc names it");
/// ```
#[cfg(doctest)]
struct MergedExamples;

// The layers: the engine below the domains of
// `crates/internal/core/domains.toml`, whose `[loader]` table holds the
// edges between them.
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
mod events;
#[cfg(windows)]
mod host;
#[cfg(windows)]
mod parts;
#[cfg(windows)]
mod start;

#[cfg(windows)]
#[doc(hidden)]
pub use content::{served::bench_text_lookup, textures::bench_texture_request};
#[cfg(windows)]
#[doc(hidden)]
pub use engine::pages::{forget as forget_read_only, read_only};
#[cfg(windows)]
pub use parts::table_ids;

/// The loader's `HostApi` table, for the benchmark of one entry call.
#[cfg(windows)]
#[doc(hidden)]
#[must_use]
pub fn host_api() -> &'static enshwick_core::abi::HostApi {
    &host::API
}
#[cfg(windows)]
pub use start::{DryMod, DryRun, DryRunError, dry_run};

/// Starts the loader; called once by the `powrprof.dll` stub, which holds
/// the dedicated server's own code until `release` is called.
///
/// Exported under [`enshwick_loader_start::EXPORT`], whose name carries the
/// start contract's version, so a stub of another contract finds nothing to
/// call.
///
/// # Safety
///
/// The stub's guarantees of [`enshwick_loader_start::Start`]: called outside
/// the Windows loader lock, and `release` is the stub's function that lets
/// the server's own code go on.
#[cfg(windows)]
#[unsafe(export_name = enshwick_loader_start::export_name!())]
pub unsafe extern "system" fn enshwick_start(
    missing_forwards: u32,
    release: extern "system" fn() -> u32,
) {
    if let Err(payload) = std::panic::catch_unwind(|| start::start(missing_forwards, release)) {
        engine::early::note(&format!(
            "the loader's start panicked: {}; mods that did not start stay off",
            engine::early::panic_text(payload.as_ref())
        ));
    }
}

// The stub calls the export through this type.
#[cfg(windows)]
const _: enshwick_loader_start::Start = enshwick_start;
