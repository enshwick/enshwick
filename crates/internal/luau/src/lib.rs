//! Luau mods for the Enshwick Loader: each mod runs in a sandboxed Luau VM
//! of its own, and reaches the game only through the API's script
//! descriptions ([`enshwick::script`](mod@enshwick::script)).
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

mod bridge;
mod files;
mod run;

pub use files::Scripts;
pub use run::{Limits, ReloadError, Runtime, Usage, start};
