//! The fake's base entries: the build, hooks, mods, the host, messages,
//! pacing and services.

pub(crate) mod build;
#[cfg(test)]
mod headed_tests;
pub(crate) mod hooks;
pub(crate) mod host;
pub(crate) mod mods;
pub(crate) mod net;
pub(crate) mod pacing;
pub(crate) mod services;
