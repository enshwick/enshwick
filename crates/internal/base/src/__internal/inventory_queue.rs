//! The limit on the inventory requests that wait at once, which the loader
//! and the fake host both keep.

/// The most requests waiting, so mods that submit faster than rows run them
/// cannot grow the queue without end; past it a submit answers busy.
pub const MAX_QUEUED: usize = 4096;
