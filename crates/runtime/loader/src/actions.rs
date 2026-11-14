//! The actions domain: player actions, console commands, controls, chat,
//! interactions and vetoes.

pub mod admission;
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
pub mod reply;
#[cfg(windows)]
pub(crate) mod vetoes;

#[cfg(windows)]
use crate::engine::parts::{Part, fused, part, step};

/// Why a reflected type or field fails when the build's reflection lacks
/// it, where `ABSENT` names a missing table entry.
#[cfg(windows)]
const UNREFLECTED: &str = "the build's reflection lacks it";

/// The actions domain's parts, in the order the joins call each of their
/// phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        table_ids: vetoes::TABLE_IDS,
        seal: Some(vetoes::seal),
        link: Some(vetoes::link),
        ..part!("vetoes", vetoes)
    },
    &Part {
        table_ids: console::TABLE_IDS,
        steps: &[step!(Input, "console lines", |s| console::after_step(
            s.loader
        ))],
        ..part!("console", console)
    },
    &Part {
        seal: Some(|_| commands::seal()),
        steps: &[step!(Input, "chat commands", |s| commands::after_step(
            s.loader
        ))],
        forget: Some(|_| commands::forget()),
        ..part!("commands", commands)
    },
    // After the commands: the lines it raises and posts take their replies
    // in the same step, before the step's output.
    &Part {
        table_ids: chat::TABLE_IDS,
        link: Some(chat::link),
        steps: &[step!(Input, "chat sending", |_| chat::drain())],
        forget: Some(|_| chat::forget()),
        ..part!("chat", chat)
    },
    &Part {
        table_ids: interactions::TABLE_IDS,
        link: Some(interactions::link),
        steps: &[step!(Follow, "interaction steps", |s| {
            interactions::after_step(s.loader, s.step);
        })],
        joined: Some(fused!("the players' screens at join", |loader, _| {
            interactions::player_joined(loader);
        })),
        forget: Some(|_| interactions::forget()),
        ..part!("interactions", interactions)
    },
    &Part {
        table_ids: local_held::TABLE_IDS,
        link: Some(local_held::link),
        ..part!("local_held", local_held)
    },
    &Part {
        link: Some(controls::link),
        steps: &[
            step!(Follow, "control firings", |s| controls::after_step(
                s.loader
            )),
            step!(Follow, "carried controls", |s| controls::resend_on_rights(
                s.loader
            )),
        ],
        left: Some(fused!("controls at leave", |_, player| controls::left(
            player.key.get()
        ))),
        hosting: Some(|_| controls::forget_carried()),
        forget: Some(|_| controls::forget()),
        ..part!("controls", controls)
    },
    &Part {
        forget: Some(|_| admission::forget()),
        ..part!("player_actions", player_actions)
    },
    &Part {
        steps: &[step!(Follow, "build refusals", |s| {
            build_reasons::after_step(s.loader);
        })],
        ..part!("build_reasons", build_reasons)
    },
    &Part {
        world_event: Some(fused!("chat mutes at world events", |loader, event| {
            chat_mute::world_event(loader, event);
        })),
        ..part!("chat_mute", chat_mute)
    },
];
