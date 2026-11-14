//! Luau mods for the Enshwick loader: each mod runs in a sandboxed Luau VM
//! of its own, and reaches the game only through the API's script
//! descriptions ([`enshwick::script`](mod@enshwick_core::script)).
//!
//! A script error, a Rust panic in a call it made, or a script past its
//! memory or time budget turns that mod off through the loader's fault; the
//! game and other mods run on.
//!
//! # Examples
//!
//! ```
//! use enshwick_luau::{Limits, Scripts};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let scripts = Scripts::new().with("main.luau", "enshwick.log.info('hello')");
//!     enshwick_luau::start(load, "you.hello", scripts, None, Limits::default())?;
//!     Ok(())
//! }
//! ```

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

mod bridge;
mod budget;
mod calls;
mod files;
mod points;
mod run;
mod subscribe;

pub use files::Scripts;
pub use run::{Limits, Runtime, StartError, Usage, start, start_pack};
