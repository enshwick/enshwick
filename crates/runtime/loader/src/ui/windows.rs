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
//! A player may join the game before their client's hello reaches the
//! server: the client says hello when a link opens, holding no window, and
//! says it again every few seconds until the server welcomes it. A window
//! shown to that player waits for the welcome, so one shown at join is not
//! lost, and at the welcome the windows the client held on a link that
//! started over show again; a client without Enshwick never says hello,
//! and its player's shows wait, bounded, until the player leaves.
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
pub(crate) use run::{after_step, closed, forget, hello, left, ran, show};

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

/// Where the step takes a change for a player who is online.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Nowhere: an update of a window the client does not hold.
    Skip,
    /// To the changes waiting for the client's hello: it takes no messages
    /// yet.
    Pend,
    /// To the client.
    Carry,
}

/// Where the step takes `waiting`, its player's client `ready` to take
/// messages; `holds` says whether the client holds the window, asked only
/// for an update to a ready client.
#[must_use]
pub fn route(waiting: &Waiting, ready: bool, holds: impl FnOnce() -> bool) -> Route {
    if !ready {
        Route::Pend
    } else if waiting.change == Change::Update && !holds() {
        Route::Skip
    } else {
        Route::Carry
    }
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
        Message::Window { window, open, .. } if *open && !holds(held, key, owner, window) => {
            hold(held, key, owner, window.clone())
        }
        Message::WindowClose { window, .. } => close_told(held, key, owner, window.clone()),
        _ => None,
    }
}

/// Notes in `held` that client `key` holds mod `owner`'s window `window`,
/// which it did not hold; the window the client let go for it past
/// [`MAX_HELD`], its oldest.
fn hold(held: &mut Held, key: u64, owner: ModIndex, window: String) -> Option<Told> {
    held.push((key, owner, window));
    if held.iter().filter(|(k, _, _)| *k == key).count() <= MAX_HELD {
        return None;
    }
    let oldest = held.iter().position(|(k, _, _)| *k == key)?;
    let (key, owner, window) = held.remove(oldest);
    Some(Told::Closed { key, owner, window })
}

/// Keeps in `pending` the change `waiting` for a client that takes no
/// messages yet, to carry at its hello.
///
/// A show waits; a close takes the window back from `pending` and from
/// `held`, where a client whose link started over left it; an update
/// waits for nothing, since the hello shows the window anew. The answer
/// is the window let go for a newer one past [`MAX_HELD`], which its mod
/// hears as closed, as when a client lets its oldest go.
pub fn pend(held: &mut Held, pending: &mut Held, waiting: Waiting) -> Option<Told> {
    let Waiting {
        owner,
        window,
        key,
        change,
    } = waiting;
    match change {
        Change::Show if !holds(pending, key, owner, &window) => hold(pending, key, owner, window),
        Change::Close => {
            let _closed_by_its_mod = close_told(held, key, owner, window.clone());
            let _taken_back = close_told(pending, key, owner, window);
            None
        }
        Change::Show | Change::Update => None,
    }
}

/// Shows client `key`'s windows again at its hello, before the changes
/// `waiting`.
///
/// A client says hello on a new link that holds no window, so those `held`
/// noted and those `pending` kept go, oldest first, before the changes the
/// mods made after them. All go or none past [`MAX_WAITING`]; the answer is
/// then the windows that will not show, which their mods hear as closed.
pub fn greeted(
    held: &mut Held,
    pending: &mut Held,
    waiting: &mut Vec<Waiting>,
    key: u64,
) -> Vec<Told> {
    let theirs = |(k, _, _): &mut (u64, ModIndex, String)| *k == key;
    let shows: Vec<Waiting> = held
        .extract_if(.., theirs)
        .chain(pending.extract_if(.., theirs))
        .map(|(key, owner, window)| Waiting {
            owner,
            window,
            key,
            change: Change::Show,
        })
        .collect();
    if waiting.len() + shows.len() <= MAX_WAITING {
        waiting.splice(0..0, shows);
        return Vec::new();
    }
    shows
        .into_iter()
        .map(|w| Told::Closed {
            key: w.key,
            owner: w.owner,
            window: w.window,
        })
        .collect()
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

    #[test]
    fn a_show_before_the_hello_waits_and_a_close_takes_it_back() {
        let (mine, other) = (ModIndex::new(0), ModIndex::new(1));
        let change = |owner, key, window: &str, change| Waiting {
            owner,
            window: window.into(),
            key,
            change,
        };
        let mut held = vec![(7, mine, "bans".to_owned()), (8, mine, "bans".to_owned())];
        let mut pending = Vec::new();
        for waiting in [
            change(mine, 7, "panel", Change::Show),
            change(mine, 7, "panel", Change::Show),
            change(mine, 7, "panel", Change::Update),
            change(other, 7, "panel", Change::Show),
            change(mine, 8, "panel", Change::Update),
            change(mine, 7, "rules", Change::Show),
            change(mine, 7, "rules", Change::Close),
            change(mine, 9, "gone", Change::Close),
            change(mine, 7, "bans", Change::Close),
        ] {
            assert_eq!(pend(&mut held, &mut pending, waiting), None);
        }
        assert_eq!(
            pending,
            [
                (7, mine, "panel".to_owned()),
                (7, other, "panel".to_owned())
            ],
            "a show waits once, an update waits for nothing and a close takes its show back"
        );
        assert_eq!(
            held,
            [(8, mine, "bans".to_owned())],
            "a close takes back a window a client whose link started over left held"
        );
    }

    #[test]
    fn past_the_bound_a_waiting_show_lets_the_player_s_oldest_go() {
        let mine = ModIndex::new(0);
        let mut pending = vec![(8, mine, "other".to_owned())];
        let let_go: Vec<Told> = (0..=MAX_HELD)
            .filter_map(|n| {
                pend(
                    &mut Vec::new(),
                    &mut pending,
                    Waiting {
                        owner: mine,
                        window: format!("w{n}"),
                        key: 7,
                        change: Change::Show,
                    },
                )
            })
            .collect();
        assert_eq!(
            let_go,
            [Told::Closed {
                key: 7,
                owner: mine,
                window: "w0".to_owned(),
            }],
            "the mod hears of the window that will not show"
        );
        assert_eq!(pending.iter().filter(|(k, _, _)| *k == 7).count(), MAX_HELD);
        assert!(
            pending.contains(&(8, mine, "other".to_owned())),
            "another player's stays"
        );
    }

    /// Mod `owner`'s change `change` of window `window` for client 7.
    fn change_of(owner: ModIndex, window: &str, change: Change) -> Waiting {
        Waiting {
            owner,
            window: window.into(),
            key: 7,
            change,
        }
    }

    #[test]
    fn a_hello_shows_again_what_the_client_held_then_what_waited_before_newer_changes() {
        let (mine, other) = (ModIndex::new(0), ModIndex::new(1));
        let mut held = vec![(7, mine, "bans".to_owned()), (8, mine, "bans".to_owned())];
        let mut pending = vec![
            (7, mine, "panel".to_owned()),
            (8, mine, "panel".to_owned()),
            (7, other, "rules".to_owned()),
        ];
        let mut waiting = vec![change_of(mine, "panel", Change::Close)];
        assert!(greeted(&mut held, &mut pending, &mut waiting, 7).is_empty());
        assert_eq!(
            waiting,
            [
                change_of(mine, "bans", Change::Show),
                change_of(mine, "panel", Change::Show),
                change_of(other, "rules", Change::Show),
                change_of(mine, "panel", Change::Close),
            ]
        );
        assert_eq!(
            merged(waiting.clone())[1],
            change_of(mine, "panel", Change::Close),
            "a close the mod made after its show wins"
        );
        assert_eq!(held, [(8, mine, "bans".to_owned())]);
        assert_eq!(pending, [(8, mine, "panel".to_owned())]);
        waiting.clear();
        assert!(greeted(&mut held, &mut pending, &mut waiting, 7).is_empty());
        assert!(waiting.is_empty(), "a second hello shows nothing again");
    }

    #[test]
    fn shows_a_hello_has_no_room_for_are_told_closed_and_none_go() {
        let mine = ModIndex::new(0);
        let mut held = vec![(7, mine, "bans".to_owned())];
        let mut pending = vec![(7, mine, "panel".to_owned())];
        let mut waiting = vec![change_of(mine, "other", Change::Update); MAX_WAITING - 1];
        let closed = |window: &str| Told::Closed {
            key: 7,
            owner: mine,
            window: window.to_owned(),
        };
        assert_eq!(
            greeted(&mut held, &mut pending, &mut waiting, 7),
            [closed("bans"), closed("panel")]
        );
        assert_eq!(waiting.len(), MAX_WAITING - 1);
        assert!(held.is_empty() && pending.is_empty());
    }

    #[test]
    fn a_change_waits_for_a_client_without_its_hello_and_an_update_goes_only_where_held() {
        let mine = ModIndex::new(0);
        let change = |change| change_of(mine, "panel", change);
        let unasked = || -> bool { panic!("holds asked for a change that does not need it") };
        for each in [Change::Show, Change::Update, Change::Close] {
            assert_eq!(
                route(&change(each), false, unasked),
                Route::Pend,
                "{each:?}"
            );
        }
        assert_eq!(route(&change(Change::Update), true, || true), Route::Carry);
        assert_eq!(route(&change(Change::Update), true, || false), Route::Skip);
        assert_eq!(route(&change(Change::Show), true, unasked), Route::Carry);
        assert_eq!(route(&change(Change::Close), true, unasked), Route::Carry);
    }
}
