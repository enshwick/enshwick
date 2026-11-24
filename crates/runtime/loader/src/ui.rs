//! The ui domain: the client's interface, mods' windows and the Mods screen.

#[cfg(windows)]
pub(crate) mod interface;
pub mod listing;
pub mod mods_screen;
#[cfg(any(windows, test))]
pub(crate) mod place;
#[cfg(any(windows, test))]
pub(crate) mod sound;
pub mod widget_checks;
#[cfg(any(windows, test))]
pub(crate) mod window_node;
pub mod windows;

#[cfg(windows)]
use crate::base::remote::Message;
#[cfg(windows)]
use crate::engine::parts::{Part, ServerLink, fused, part, step};

/// The ui domain's parts, in the order the joins call each of their phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        table_ids: interface::TABLE_IDS,
        seal: Some(|_| {
            interface::seal();
            crate::base::dialogs::on_screens(crate::base::dialogs::Screens {
                show: |title, message, detail, buttons, on_answer| {
                    interface::show(title, message, detail, buttons, on_answer);
                },
                mismatch_screen_on: interface::mismatch_screen_on,
                window_shown: interface::window_shown,
                text: interface::text,
            });
            crate::base::net::on_control(crate::base::wire::UI_DEMO, |_, rest| {
                interface::open_demo(crate::base::wire::demo_position(rest));
            });
            crate::base::remote::on_client(crate::base::remote::WINDOW, |message| {
                if let Message::Window {
                    id,
                    window,
                    open,
                    text,
                } = message
                {
                    interface::carried::heard(&id, &window, open, &text);
                }
            });
            crate::base::remote::on_client(crate::base::remote::WINDOW_CLOSE, |message| {
                if let Message::WindowClose { id, window } = message {
                    interface::carried::closed(&id, &window);
                }
            });
        }),
        link: Some(interface::link),
        forget: Some(|loader| {
            interface::forget(loader);
        }),
        server_link: Some(|link| match link {
            ServerLink::Joining => {
                interface::demo::forget();
                interface::close_world_dialogs();
            }
            ServerLink::Lost | ServerLink::Forgotten => interface::carried::forget(),
            ServerLink::Answered => {}
        }),
        ..part!("ui", interface)
    },
    &Part {
        table_ids: sound::TABLE_IDS,
        link: Some(sound::link),
        ..part!("sound", sound)
    },
    &Part {
        seal: Some(|_| {
            crate::world::waypoints::watch_for(crate::world::waypoints::Maps {
                held: windows::holds_map,
                changed: windows::waypoint_changed,
                fuse: &windows::MAP_RUNS,
            });
            crate::actions::player_actions::on_ran(windows::ran);
            crate::base::remote::on_server(
                crate::base::remote::WINDOW_CLOSE,
                |loader, key, message| {
                    if let Message::WindowClose { id, window } = message {
                        windows::closed(loader, key, &id, window);
                    }
                },
            );
        }),
        steps: &[step!(Follow, "carried windows", |s| windows::after_step(
            s.loader
        ))],
        left: Some(fused!("windows at leave", |_, player| windows::left(
            player.key.get()
        ))),
        hello: Some(fused!("windows at hello", |_, key| windows::hello(key))),
        end: Some(|loader| windows::forget(Some(loader))),
        ..part!("windows", windows)
    },
];
