//! The ui domain: the client's interface, mods' windows, notices and the Mods
//! screen.

#[cfg(windows)]
pub(crate) mod interface;
pub mod listing;
pub mod mods_screen;
pub mod notice_queue;
#[cfg(windows)]
pub(crate) mod notices;
pub mod widget_checks;
pub mod windows;
