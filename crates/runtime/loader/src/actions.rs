//! The actions domain: player actions, console commands, controls, chat,
//! interactions and vetoes.

pub mod admission;
pub mod bindings;
#[cfg(windows)]
pub(crate) mod build_reasons;
#[cfg(windows)]
pub(crate) mod chat;
pub mod chat_mute;
#[cfg(windows)]
pub(crate) mod client_commands;
#[cfg(windows)]
pub(crate) mod client_control;
#[cfg(windows)]
pub(crate) mod client_input;
#[cfg(windows)]
pub(crate) mod client_recipes;
#[cfg(windows)]
pub(crate) mod client_snapshot;
pub mod commands;
pub mod console;
#[cfg(windows)]
pub(crate) mod controls;
pub mod game_bindings;
#[cfg(windows)]
pub(crate) mod interaction_prompts;
#[cfg(windows)]
pub(crate) mod interactions;
pub mod linked_controls;
#[cfg(windows)]
pub(crate) mod local_held;
#[cfg(windows)]
pub(crate) mod player_actions;
#[cfg(windows)]
pub(crate) mod player_state;
pub mod reply;
#[cfg(windows)]
pub(crate) mod vetoes;

#[cfg(windows)]
use crate::base::remote::Message;
#[cfg(windows)]
use crate::engine::parts::{Part, ServerLink, fused, part, step};

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
        game_events: Some(vetoes::FOLLOWER),
        ..part!("vetoes", vetoes)
    },
    &Part {
        table_ids: console::TABLE_IDS,
        steps: &[step!(Input, "console lines", |s| console::after_step(
            s.loader
        ))],
        forget: Some(|_| {
            console::forget();
        }),
        ..part!("console", console)
    },
    &Part {
        seal: Some(|_| commands::seal()),
        steps: &[step!(Input, "chat commands", |s| commands::after_step(
            s.loader
        ))],
        forget: Some(|_| {
            commands::forget();
        }),
        ..part!("commands", commands)
    },
    // After the commands: the lines it raises and posts take their replies
    // in the same step, before the step's output.
    &Part {
        table_ids: chat::TABLE_IDS,
        seal: Some(|_| {
            chat::seal();
            crate::base::tell::on_chat(crate::base::tell::Chat {
                send: chat::send,
                delivery: chat::delivery,
            });
        }),
        link: Some(chat::link),
        steps: &[step!(Input, "chat sending", |_| chat::drain())],
        forget: Some(|_| chat::forget()),
        ..part!("chat", chat)
    },
    &Part {
        table_ids: interactions::TABLE_IDS,
        seal: Some(|_| {
            interactions::seal();
            crate::world::transactions::on_filter_off(interactions::filter_off);
        }),
        link: Some(interactions::link),
        steps: &[step!(Follow, "interaction steps", |s| {
            interactions::after_step(s.loader, s.step);
        })],
        joined: Some(fused!("the players' screens at join", |loader, _| {
            interactions::player_joined(loader);
        })),
        forget: Some(|_| {
            interactions::forget();
        }),
        ..part!("interactions", interactions)
    },
    &Part {
        table_ids: interaction_prompts::TABLE_IDS,
        seal: Some(|_| {
            let _prompts = interaction_prompts::seal();
        }),
        link: Some(interaction_prompts::link),
        ..part!("interaction_prompts", interaction_prompts)
    },
    &Part {
        table_ids: local_held::TABLE_IDS,
        link: Some(local_held::link),
        ..part!("local_held", local_held)
    },
    &Part {
        table_ids: game_bindings::TABLE_IDS,
        link: Some(game_bindings::link),
        ..part!("game_bindings", game_bindings)
    },
    &Part {
        table_ids: client_control::TABLE_IDS,
        link: Some(client_control::link),
        ..part!("client_control", client_control)
    },
    // After client_control: each links only on the group it resolved.
    &Part {
        link: Some(client_input::link),
        ..part!("client_input", client_input)
    },
    &Part {
        link: Some(client_recipes::link),
        ..part!("client_recipes", client_recipes)
    },
    &Part {
        seal: Some(|_| {
            controls::seal();
            crate::base::remote::on_client(crate::base::remote::CONTROLS, |message| {
                if let Message::Controls { id, name, controls } = message {
                    controls::carry(&id, &name, &controls);
                }
            });
        }),
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
        hosting: Some(|_| {
            controls::forget_carried();
        }),
        forget: Some(|_| {
            controls::forget();
        }),
        server_link: Some(|link| {
            if link == ServerLink::Forgotten {
                controls::forget_carried();
            }
        }),
        handover: Some(controls::hello),
        ..part!("controls", controls)
    },
    &Part {
        seal: Some(|_| {
            player_actions::seal();
            crate::base::remote::on_server(
                crate::base::remote::ACTION_RUN,
                |loader, key, message| {
                    if let Message::ActionRun {
                        id,
                        action,
                        args,
                        target,
                    } = message
                    {
                        player_actions::run(loader, key, &id, &action, &args, target);
                    }
                },
            );
        }),
        forget: Some(|_| {
            admission::forget();
        }),
        server_link: Some(|link| {
            if link == ServerLink::Answered {
                player_actions::joined();
            }
        }),
        ..part!("player_actions", player_actions)
    },
    &Part {
        steps: &[step!(Follow, "build refusals", |s| {
            build_reasons::after_step(s.loader);
        })],
        forget: Some(|_| {
            build_reasons::forget();
        }),
        ..part!("build_reasons", build_reasons)
    },
    &Part {
        world_event: Some(fused!("chat mutes at world events", |loader, event| {
            chat_mute::world_event(loader, event);
        })),
        ..part!("chat_mute", chat_mute)
    },
];
