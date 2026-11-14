//! The ui domain: the client's interface, mods' windows and the Mods screen.

#[cfg(windows)]
pub(crate) mod interface;
pub mod listing;
pub mod mods_screen;
pub mod widget_checks;
pub mod windows;

#[cfg(windows)]
use crate::engine::parts::{Part, fused, part, step};

/// The ui domain's parts, in the order the joins call each of their phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        table_ids: interface::TABLE_IDS,
        link: Some(interface::link),
        forget: Some(interface::forget),
        ..part!("ui", interface)
    },
    &Part {
        steps: &[step!(Follow, "carried windows", |s| windows::after_step(
            s.loader
        ))],
        left: Some(fused!("windows at leave", |_, player| windows::left(
            player.key.get()
        ))),
        hello: Some(fused!("windows at hello", |_, key| windows::hello(key))),
        end: Some(fused!("windows at a game's end", |loader| windows::forget(
            Some(loader)
        ))),
        ..part!("windows", windows)
    },
];
