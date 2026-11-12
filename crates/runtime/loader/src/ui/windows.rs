//! Carried windows on the server: windows a mod shows on players' clients.
//!
//! A mod shows or closes a window for some players, and after the step the
//! server asks the mod for the window as each player sees it and carries
//! the text to that player's client, which draws it without the mod. After
//! a player's run the server renders the
//! mod's windows that player holds again, so a list shows its switches as
//! they are now.
//!
//! A hosting client's own player gets its windows the same way, through
//! the carrier's loop inside the process.
//!
//! A mod may also update a window only where it still shows, and hears
//! when a client closed one or its player left. Merging the changes and
//! noting what a client holds is pure logic here; the run module keeps
//! them and carries them on the game thread.

use enshwick_core::abi::{WINDOW_CLOSE, WINDOW_SHOW, WINDOW_UPDATE};

use crate::base::remote::Message;
use crate::engine::mod_index::ModIndex;

#[cfg(windows)]
mod run;
#[cfg(windows)]
pub(crate) use run::{after_step, closed, forget, left, ran, show};

pub use enshwick_core::actions::window::MAX_CALL_PLAYERS as MAX_PLAYERS;
use enshwick_core::actions::window::MAX_WAITING;

pub use enshwick_core::actions::window::MAX_HELD;

/// What happens to a window on one client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The mod showed it: it opens, or updates where it shows.
    Show,
    /// The mod updates it, or the server renders it again after a run: it
    /// updates only where it shows.
    Update,
    /// It closes.
    Close,
}

impl Change {
    /// The change a `close` value of `HostApi::action_window` asks for;
    /// `None` for a value this loader does not know.
    #[must_use]
    pub const fn of(close: u32) -> Option<Self> {
        match close {
            WINDOW_SHOW => Some(Self::Show),
            WINDOW_CLOSE => Some(Self::Close),
            WINDOW_UPDATE => Some(Self::Update),
            _ => None,
        }
    }
}

/// A change of a mod's window for one player, waiting for the step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waiting {
    /// The mod whose window it is.
    pub owner: ModIndex,
    /// The window's id.
    pub window: String,
    /// The player's account key.
    pub key: u64,
    /// What happens to the window there.
    pub change: Change,
}

/// What the mods hear after the step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Told {
    /// A client closed a window, or let its oldest go for a newer one: the
    /// window's mod hears it.
    Closed {
        /// The player's account key.
        key: u64,
        /// The mod whose window closed.
        owner: ModIndex,
        /// The window's id.
        window: String,
    },
    /// A player left: every mod with actions hears it.
    Left {
        /// The player's account key.
        key: u64,
    },
}

/// The windows clients got and were not told to close: account key, mod
/// and window.
pub type Held = Vec<(u64, ModIndex, String)>;

/// Adds `more` to `waiting`, all or none: false past [`MAX_WAITING`].
pub fn queue<T>(waiting: &mut Vec<T>, more: impl ExactSizeIterator<Item = T>) -> bool {
    if waiting.len() + more.len() > MAX_WAITING {
        return false;
    }
    waiting.extend(more);
    true
}

/// Whether client `key` holds mod `owner`'s window `window`.
#[must_use]
pub fn holds(held: &[(u64, ModIndex, String)], key: u64, owner: ModIndex, window: &str) -> bool {
    held.iter()
        .any(|(k, o, w)| *k == key && *o == owner && w == window)
}

/// The waiting changes with one for each window of a player, where the
/// first stood.
///
/// It is the last show or close, which an update after it leaves as it is,
/// since a show renders anew and a closed window takes no update.
#[must_use]
pub fn merged(waiting: Vec<Waiting>) -> Vec<Waiting> {
    let mut merged: Vec<Waiting> = Vec::with_capacity(waiting.len());
    for next in waiting {
        let same = |w: &&mut Waiting| {
            w.key == next.key && w.owner == next.owner && w.window == next.window
        };
        match merged.iter_mut().find(same) {
            Some(_) if next.change == Change::Update => {}
            Some(first) => first.change = next.change,
            None => merged.push(next),
        }
    }
    merged
}

/// What mod `owner` hears when client `key` says it closed the window
/// `window`, which `held` then holds no more.
///
/// `None` for a window the client did not hold, so a client names closes
/// only of what the server carried to it.
pub fn close_told(held: &mut Held, key: u64, owner: ModIndex, window: String) -> Option<Told> {
    let before = held.len();
    held.retain(|(k, o, w)| !(*k == key && *o == owner && *w == window));
    (held.len() != before).then_some(Told::Closed { key, owner, window })
}

/// What the mods hear of `told` once a step's changes reached the clients,
/// which then hold `held`.
///
/// Not the close of a window its client holds again, shown again in the
/// same step or pushed out and shown again: it would make the mod forget a
/// window that shows.
#[must_use]
pub fn heard(mut told: Vec<Told>, held: &[(u64, ModIndex, String)]) -> Vec<Told> {
    told.retain(|told| match told {
        Told::Closed { key, owner, window } => !holds(held, *key, *owner, window),
        Told::Left { .. } => true,
    });
    told
}

/// What the mods hear when the world goes, with `told` waiting and clients
/// holding `held`: each of those players left, once, so no mod keeps their
/// windows into the next world.
#[must_use]
pub fn leaving(told: Vec<Told>, held: &[(u64, ModIndex, String)]) -> Vec<Told> {
    let mut keys: Vec<u64> = Vec::new();
    let told_keys = told.into_iter().map(|told| match told {
        Told::Closed { key, .. } | Told::Left { key } => key,
    });
    for key in told_keys.chain(held.iter().map(|(key, _, _)| *key)) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.into_iter().map(|key| Told::Left { key }).collect()
}

/// Notes what client `key` holds once `message` of mod `owner` reached it.
///
/// The window it closed there, which its mod hears as closed: the one the
/// message closes, or the client's oldest let go past [`MAX_HELD`] for the
/// one it opens.
pub fn note(held: &mut Held, key: u64, owner: ModIndex, message: &Message) -> Option<Told> {
    match message {
        Message::Window { window, open, .. } => {
            let at = held
                .iter()
                .position(|(k, o, w)| *k == key && *o == owner && w == window);
            if at.is_none() && *open {
                held.push((key, owner, window.clone()));
                let mine: Vec<usize> = (0..held.len()).filter(|&i| held[i].0 == key).collect();
                if mine.len() > MAX_HELD {
                    let (key, owner, window) = held.remove(mine[0]);
                    return Some(Told::Closed { key, owner, window });
                }
            }
        }
        Message::WindowClose { window, .. } => {
            return close_told(held, key, owner, window.clone());
        }
        _ => {}
    }
    None
}

/// The message carrying `text`, the mod's window `window` as one player
/// sees it, opening it where `open`: a close for no text or a text past
/// the bound.
#[must_use]
pub fn message(id: &str, window: String, open: bool, text: String) -> Message {
    if text.is_empty() || text.len() > enshwick_core::actions::window::MAX_BYTES {
        return Message::WindowClose {
            id: id.to_owned(),
            window,
        };
    }
    Message::Window {
        id: id.to_owned(),
        window,
        open,
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(key: u64) -> Waiting {
        Waiting {
            owner: ModIndex::new(0),
            window: "panel".into(),
            key,
            change: Change::Show,
        }
    }

    #[test]
    fn changes_wait_up_to_the_bound_and_a_batch_past_it_is_refused_whole() {
        let mut queued = Vec::new();
        assert!(queue(
            &mut queued,
            (0..200_u32).map(|k| waiting(u64::from(k)))
        ));
        assert!(
            !queue(&mut queued, (0..57_u32).map(|k| waiting(u64::from(k)))),
            "257 would wait"
        );
        assert_eq!(queued.len(), 200, "nothing of a refused batch waits");
        assert!(queue(
            &mut queued,
            (0..56_u32).map(|k| waiting(u64::from(k)))
        ));
        assert_eq!(queued.len(), MAX_WAITING);
    }

    #[test]
    fn an_empty_or_oversized_render_closes_the_window() {
        let shown = message("you.admin", "panel".into(), true, "W\tPanel\n".into());
        assert!(
            matches!(shown, Message::Window { ref text, open: true, .. } if text == "W\tPanel\n")
        );
        let empty = message("you.admin", "panel".into(), false, String::new());
        assert!(matches!(empty, Message::WindowClose { .. }));
        let huge = "x".repeat(enshwick_core::actions::window::MAX_BYTES + 1);
        assert!(matches!(
            message("you.admin", "panel".into(), true, huge),
            Message::WindowClose { .. }
        ));
    }

    #[test]
    fn the_server_notes_what_a_client_holds_so_an_update_reaches_only_those() {
        let (mine, other) = (ModIndex::new(0), ModIndex::new(1));
        let mut held = Vec::new();
        let shown = |window: &str, open| message("you.admin", window.into(), open, "W\tw\n".into());
        note(&mut held, 7, mine, &shown("panel", true));
        note(&mut held, 7, mine, &shown("panel", true));
        note(&mut held, 7, mine, &shown("bans", false));
        note(&mut held, 8, other, &shown("panel", true));
        assert_eq!(
            held,
            [
                (7, mine, "panel".to_owned()),
                (8, other, "panel".to_owned())
            ],
            "an update opens nothing and a show again adds no second entry"
        );
        note(
            &mut held,
            7,
            mine,
            &message("you.admin", "panel".into(), true, String::new()),
        );
        assert_eq!(held, [(8, other, "panel".to_owned())]);
        let let_go: Vec<Told> = (0..=MAX_HELD)
            .filter_map(|n| note(&mut held, 9, mine, &shown(&format!("w{n}"), true)))
            .collect();
        let oldest = Told::Closed {
            key: 9,
            owner: mine,
            window: "w0".to_owned(),
        };
        assert_eq!(let_go, [oldest], "the mod hears of the window let go");
        assert_eq!(held.iter().filter(|(k, _, _)| *k == 9).count(), MAX_HELD);
        assert!(
            !held.contains(&(9, mine, "w0".to_owned())),
            "the oldest goes"
        );
    }

    #[test]
    fn a_close_takes_only_its_own_window_of_the_mod() {
        let mine = ModIndex::new(0);
        let mut held = vec![
            (7, mine, "panel".to_owned()),
            (7, mine, "bans".to_owned()),
            (8, mine, "panel".to_owned()),
        ];
        let close = message("you.admin", "panel".into(), true, String::new());
        note(&mut held, 7, mine, &close);
        assert_eq!(
            held,
            [(7, mine, "bans".to_owned()), (8, mine, "panel".to_owned())]
        );
    }

    #[test]
    fn a_window_of_exactly_the_bound_is_carried() {
        let most = "x".repeat(enshwick_core::actions::window::MAX_BYTES);
        assert!(matches!(
            message("you.admin", "panel".into(), true, most),
            Message::Window { .. }
        ));
    }

    #[test]
    fn the_changes_of_one_window_for_one_player_merge_into_its_last_show_or_close() {
        let change = |owner, key, window: &str, change| Waiting {
            owner: ModIndex::new(owner),
            window: window.into(),
            key,
            change,
        };
        let waiting = vec![
            change(0, 7, "panel", Change::Show),
            change(0, 7, "panel", Change::Update),
            change(0, 8, "panel", Change::Update),
            change(0, 7, "bans", Change::Show),
            change(0, 8, "panel", Change::Update),
            change(0, 7, "bans", Change::Close),
            change(0, 9, "panel", Change::Close),
            change(1, 7, "panel", Change::Update),
            change(0, 9, "panel", Change::Update),
            change(0, 7, "bans", Change::Show),
            change(0, 6, "panel", Change::Show),
            change(0, 6, "panel", Change::Close),
        ];
        assert_eq!(
            merged(waiting),
            [
                change(0, 7, "panel", Change::Show),
                change(0, 8, "panel", Change::Update),
                change(0, 7, "bans", Change::Show),
                change(0, 9, "panel", Change::Close),
                change(1, 7, "panel", Change::Update),
                change(0, 6, "panel", Change::Close),
            ],
            "one render each: a show still opens, a closed window takes no update"
        );
    }

    #[test]
    fn a_client_s_close_is_told_only_for_a_window_the_server_carried_to_it() {
        let (mine, other) = (ModIndex::new(0), ModIndex::new(1));
        let mut held = vec![(7, mine, "panel".to_owned()), (8, mine, "bans".to_owned())];
        for (key, owner, window) in [(7, mine, "bans"), (7, other, "panel"), (9, mine, "panel")] {
            assert_eq!(
                close_told(&mut held, key, owner, window.to_owned()),
                None,
                "{key} holds no {window} of that mod"
            );
        }
        assert_eq!(
            held.len(),
            2,
            "a close of a window not held forgets nothing"
        );
        let told = Told::Closed {
            key: 7,
            owner: mine,
            window: "panel".to_owned(),
        };
        assert_eq!(
            close_told(&mut held, 7, mine, "panel".to_owned()),
            Some(told)
        );
        assert_eq!(held, [(8, mine, "bans".to_owned())]);
        assert_eq!(
            close_told(&mut held, 7, mine, "panel".to_owned()),
            None,
            "a second close of the same window tells nothing"
        );
    }

    #[test]
    fn a_close_is_told_only_for_a_window_the_client_no_longer_holds_after_the_step() {
        let mine = ModIndex::new(0);
        let closed = |key, window: &str| Told::Closed {
            key,
            owner: mine,
            window: window.to_owned(),
        };
        let told = vec![
            closed(7, "panel"),
            closed(7, "bans"),
            closed(8, "panel"),
            Told::Left { key: 7 },
        ];
        // 7's panel was shown again and carried in the step, or pushed out
        // and shown again; 8's show did not reach its client.
        let held = vec![(7, mine, "panel".to_owned())];
        assert_eq!(
            heard(told, &held),
            [closed(7, "bans"), closed(8, "panel"), Told::Left { key: 7 }],
            "only a window the client holds again keeps it, and a leave is always told"
        );
    }

    #[test]
    fn when_the_world_goes_every_player_with_a_window_or_a_tell_leaves_once() {
        let mine = ModIndex::new(0);
        let told = vec![
            Told::Closed {
                key: 7,
                owner: mine,
                window: "panel".to_owned(),
            },
            Told::Left { key: 9 },
        ];
        let held = vec![(8, mine, "panel".to_owned()), (8, mine, "bans".to_owned())];
        assert_eq!(
            leaving(told, &held),
            [
                Told::Left { key: 7 },
                Told::Left { key: 9 },
                Told::Left { key: 8 }
            ]
        );
    }

    #[test]
    fn a_close_the_server_carries_for_a_held_window_is_told_to_its_mod() {
        let mine = ModIndex::new(0);
        let mut held = vec![(7, mine, "panel".to_owned())];
        let empty = message("you.admin", "panel".into(), false, String::new());
        assert_eq!(
            note(&mut held, 7, mine, &empty),
            Some(Told::Closed {
                key: 7,
                owner: mine,
                window: "panel".to_owned(),
            }),
            "the mod hears of the window its render closed"
        );
        assert!(held.is_empty());
        assert_eq!(
            note(&mut held, 7, mine, &empty),
            None,
            "a close of a window not held tells nothing"
        );
    }

    #[test]
    fn a_close_value_names_its_change_and_an_unknown_one_names_none() {
        assert_eq!(Change::of(0), Some(Change::Show));
        assert_eq!(Change::of(1), Some(Change::Close));
        assert_eq!(Change::of(2), Some(Change::Update));
        assert_eq!(Change::of(3), None);
        assert_eq!(Change::of(u32::MAX), None);
    }

    #[test]
    fn a_client_holds_a_window_by_its_key_its_mod_and_its_id() {
        let (mine, other) = (ModIndex::new(0), ModIndex::new(1));
        let held = vec![(7, mine, "panel".to_owned())];
        assert!(holds(&held, 7, mine, "panel"));
        assert!(!holds(&held, 8, mine, "panel"), "another player");
        assert!(!holds(&held, 7, other, "panel"), "another mod");
        assert!(!holds(&held, 7, mine, "bans"), "another window");
    }
}
