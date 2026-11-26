//! The limits on the content mods serve in one process, which the loader
//! and the fake host both keep: served content is never freed, as the game
//! reads it for as long as a resource names its hash.

/// The most contents served.
pub const MAX_ENTRIES: usize = 3072;
/// The most bytes served in all: 512 MiB.
pub const MAX_BYTES: usize = 512 << 20;
