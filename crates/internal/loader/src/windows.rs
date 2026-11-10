//! Carried windows on the server: a mod shows or closes a window for some
//! players, and after the step the server asks the mod for the window as
//! each player sees it and carries the text to that player's client, which
//! draws it without the mod. After a player's run the server renders the
//! mod's windows that player holds again, so a list shows its switches as
//! they are now.
//!
//! A hosting client's own player gets its windows the same way, through
//! the carrier's loop inside the process.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::PlayerKey;
use enshwick::abi::{Side, Status};

use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::remote::Message;

pub(crate) use enshwick::actions::window::MAX_CALL_PLAYERS as MAX_PLAYERS;
use enshwick::actions::window::MAX_WAITING;

/// The most windows a client keeps, the oldest going first, and so the
/// most the server notes as held per client.
pub(crate) const MAX_HELD: usize = 8;

/// What happens to a window on one client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    /// The mod showed it: it opens, or updates where it shows.
    Show,
    /// The server renders it again: it updates only where it shows.
    Update,
    /// It closes.
    Close,
}

/// A change of a mod's window for one player, waiting for the step.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Waiting {
    owner: ModIndex,
    window: String,
    key: u64,
    change: Change,
}

/// The windows' state on the server.
struct Windows {
    waiting: Mutex<Vec<Waiting>>,
    /// Set while `waiting` may hold anything, so an idle step takes no lock.
    any: AtomicBool,
    /// The windows each client got and was not told to close: account
    /// key, mod and window.
    held: Mutex<Vec<(u64, ModIndex, String)>>,
    /// Set once the log named a window past the bound, so a mod that shows
    /// one again and again fills no log.
    warned: AtomicBool,
}

static WINDOWS: Windows = Windows {
    waiting: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
    held: Mutex::new(Vec::new()),
    warned: AtomicBool::new(false),
};

/// Adds `more` to `waiting`, all or none: false past [`MAX_WAITING`].
fn queue(waiting: &mut Vec<Waiting>, more: impl ExactSizeIterator<Item = Waiting>) -> bool {
    if waiting.len() + more.len() > MAX_WAITING {
        return false;
    }
    waiting.extend(more);
    true
}

/// Queues `more` and notes that a step has work; false past the bound.
fn wait(more: impl ExactSizeIterator<Item = Waiting>) -> bool {
    if !queue(&mut WINDOWS.waiting.relock(), more) {
        return false;
    }
    WINDOWS.any.store(true, Ordering::Release);
    true
}

/// Queues mod `owner`'s window `window` to show to, or with `close` to
/// close on, the clients of `keys`, at most [`MAX_PLAYERS`], for the step;
/// the status `HostApi::action_window` answers.
pub(crate) fn show(
    loader: &Loader,
    owner: ModIndex,
    window: &str,
    keys: &[PlayerKey],
    close: bool,
) -> Status {
    if loader.side == Side::CLIENT && !crate::world::hosting() {
        return Status::MISSING;
    }
    if !enshwick::actions::is_valid_id(window) || keys.len() > MAX_PLAYERS {
        return Status::BAD_ARGS;
    }
    if !crate::actions::declaring(loader).contains(&owner) {
        return Status::REFUSED;
    }
    let change = if close { Change::Close } else { Change::Show };
    let more = keys.iter().map(|key| Waiting {
        owner,
        window: window.to_owned(),
        key: key.get(),
        change,
    });
    if wait(more) {
        Status::OK
    } else {
        Status::REFUSED
    }
}

/// Player `key` ran an action of mod `owner` from the interface: the mod's
/// windows their client holds update after the step.
pub(crate) fn ran(owner: ModIndex, key: u64) {
    let updates: Vec<Waiting> = WINDOWS
        .held
        .relock()
        .iter()
        .filter(|(k, o, _)| *k == key && *o == owner)
        .map(|(_, _, window)| Waiting {
            owner,
            window: window.clone(),
            key,
            change: Change::Update,
        })
        .collect();
    if !updates.is_empty() && !wait(updates.into_iter()) {
        crate::log!(DEBUG, "windows: too many wait to update {key:016x}'s");
    }
}

/// Player `key`'s client closed mod `id`'s window `window`: the server
/// renders it for them no more, until the mod shows it again.
pub(crate) fn closed(loader: &Loader, key: u64, id: String, window: String) {
    if let Some((index, _)) = loader.find_mod(&id) {
        let close = Message::WindowClose { id, window };
        note(
            &mut WINDOWS.held.relock(),
            key,
            ModIndex::new(index),
            &close,
        );
    }
}

/// The waiting changes with one for each window of a player, where the
/// first stood: the last show or close, which an update after it leaves as
/// it is, since a show renders anew and a closed window takes no update.
fn merged(waiting: Vec<Waiting>) -> Vec<Waiting> {
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

/// Forgets the windows player `key`'s client held, when the player leaves.
pub(crate) fn left(key: u64) {
    WINDOWS.held.relock().retain(|(k, _, _)| *k != key);
}

/// Notes what client `key` holds once `message` of mod `owner` reached it.
fn note(held: &mut Vec<(u64, ModIndex, String)>, key: u64, owner: ModIndex, message: &Message) {
    match message {
        Message::Window { window, open, .. } => {
            let at = held
                .iter()
                .position(|(k, o, w)| *k == key && *o == owner && w == window);
            if at.is_none() && *open {
                held.push((key, owner, window.clone()));
                let mine: Vec<usize> = (0..held.len()).filter(|&i| held[i].0 == key).collect();
                if mine.len() > MAX_HELD {
                    held.remove(mine[0]);
                }
            }
        }
        Message::WindowClose { window, .. } => {
            held.retain(|(k, o, w)| !(*k == key && *o == owner && w == window));
        }
        _ => {}
    }
}

/// Carries each waiting change to its player's client, on the server's
/// game thread after its step: a show or an update as the mod renders it
/// for that player, in their language, and a close where the mod renders
/// nothing for them. A player not online is skipped, and so is one whose
/// client takes no messages, as one without Enshwick: the mod renders
/// nothing for it and the server notes nothing as held.
pub(crate) fn after_step(loader: &Loader) {
    if !WINDOWS.any.swap(false, Ordering::AcqRel) {
        return;
    }
    let waiting = core::mem::take(&mut *WINDOWS.waiting.relock());
    for Waiting {
        owner,
        window,
        key,
        change,
    } in merged(waiting)
    {
        let Some(record) = loader.mods.get(owner.get()) else {
            continue;
        };
        if !crate::net::ready(key) {
            continue;
        }
        let Some((player, language)) = crate::actions::commander(loader, key) else {
            continue;
        };
        let text = if change == Change::Close {
            String::new()
        } else {
            crate::actions::window(loader, owner, &player, language, &window).unwrap_or_default()
        };
        let message = message(&record.id, window, change == Change::Show, text);
        if crate::net::to_player(key, &message) {
            note(&mut WINDOWS.held.relock(), key, owner, &message);
        } else {
            crate::log!(
                DEBUG,
                "windows: no room to carry {} to {key:016x}",
                record.id
            );
        }
    }
}

/// The message carrying `text`, the mod's window `window` as one player
/// sees it, opening it where `open`: a close for no text or a text past
/// the bound.
fn message(id: &str, window: String, open: bool, text: String) -> Message {
    if text.is_empty() || text.len() > enshwick::actions::window::MAX_BYTES {
        if !text.is_empty() && !WINDOWS.warned.swap(true, Ordering::Relaxed) {
            crate::log!(
                WARN,
                "windows: {id}:{window} is {} bytes, past the bound, and closes; later ones close unnamed until the world goes",
                text.len()
            );
        }
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

/// Forgets the waiting changes and what clients hold, as when the server's
/// world goes away.
pub(crate) fn forget() {
    WINDOWS.waiting.relock().clear();
    WINDOWS.any.store(false, Ordering::Release);
    WINDOWS.held.relock().clear();
    WINDOWS.warned.store(false, Ordering::Relaxed);
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
        let huge = "x".repeat(enshwick::actions::window::MAX_BYTES + 1);
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
        for n in 0..=MAX_HELD {
            note(&mut held, 9, mine, &shown(&format!("w{n}"), true));
        }
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
        let most = "x".repeat(enshwick::actions::window::MAX_BYTES);
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
    fn a_run_updates_the_windows_its_player_holds_of_that_mod_and_a_leave_forgets_them() {
        let (mine, other) = (ModIndex::new(0), ModIndex::new(1));
        WINDOWS.held.relock().extend([
            (7, mine, "panel".to_owned()),
            (7, other, "panel".to_owned()),
            (8, mine, "panel".to_owned()),
        ]);
        ran(mine, 7);
        assert!(
            WINDOWS.any.swap(false, Ordering::AcqRel),
            "the step has work"
        );
        let waiting = core::mem::take(&mut *WINDOWS.waiting.relock());
        assert_eq!(
            waiting,
            [Waiting {
                owner: mine,
                window: "panel".into(),
                key: 7,
                change: Change::Update,
            }]
        );
        ran(mine, 9);
        assert!(!WINDOWS.any.load(Ordering::Acquire), "9 holds nothing");
        assert!(WINDOWS.waiting.relock().is_empty());
        left(7);
        assert_eq!(*WINDOWS.held.relock(), [(8, mine, "panel".to_owned())]);
    }
}
