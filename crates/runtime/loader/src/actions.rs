//! The actions domain: player actions, console commands, controls, chat,
//! interactions and vetoes.

pub mod bindings;
#[cfg(windows)]
pub(crate) mod build_reasons;
#[cfg(windows)]
pub(crate) mod chat;
pub mod chat_mute;
pub mod commands;
pub mod console;
#[cfg(windows)]
pub(crate) mod controls;
#[cfg(windows)]
pub(crate) mod interactions;
pub mod linked_controls;
#[cfg(windows)]
pub(crate) mod local_held;
#[cfg(windows)]
pub(crate) mod player_actions;
#[cfg(windows)]
pub(crate) mod vetoes;
