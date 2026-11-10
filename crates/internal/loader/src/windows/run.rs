//! Keeps the windows' waiting changes and what each client holds, and
//! carries them on the server's game thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::PlayerKey;
use enshwick::abi::{Side, Status};

use super::{Change, Held, MAX_PLAYERS, Told, Waiting, holds, merged, message, note, queue};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::remote::Message;

/// The windows' state on the server.
struct Windows {
    waiting: Mutex<Vec<Waiting>>,
    /// Set while `waiting` or `told` may hold anything, so an idle step
    /// takes no lock.
    any: AtomicBool,
    /// The windows each client got and was not told to close.
    held: Mutex<Held>,
    /// Closes and leaves the mods hear after the step.
    told: Mutex<Vec<Told>>,
    /// Set once a mod showed a window, so a server without windows queues
    /// nothing when a player leaves.
    shown: AtomicBool,
    /// Set once the log named a window past the bound, so a mod that shows
    /// one again and again fills no log.
    warned: AtomicBool,
}

static WINDOWS: Windows = Windows {
    waiting: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
    held: Mutex::new(Vec::new()),
    told: Mutex::new(Vec::new()),
    shown: AtomicBool::new(false),
    warned: AtomicBool::new(false),
};

/// Queues `more` and notes that a step has work; false past the bound.
fn wait(more: impl ExactSizeIterator<Item = Waiting>) -> bool {
    if !queue(&mut WINDOWS.waiting.relock(), more) {
        return false;
    }
    WINDOWS.any.store(true, Ordering::Release);
    true
}

/// Queues mod `owner`'s window `window` for the clients of `keys`, at most
/// [`MAX_PLAYERS`], for the step: to show, close or update as `change`
/// says, a `close` value of `HostApi::action_window`; the status that call
/// answers.
pub(crate) fn show(
    loader: &Loader,
    owner: ModIndex,
    window: &str,
    keys: &[PlayerKey],
    change: u32,
) -> Status {
    if loader.side == Side::CLIENT && !crate::world::hosting() {
        return Status::MISSING;
    }
    let Some(change) = Change::of(change) else {
        return Status::BAD_ARGS;
    };
    if !enshwick::actions::is_valid_id(window) || keys.len() > MAX_PLAYERS {
        return Status::BAD_ARGS;
    }
    if !crate::actions::declaring(loader).contains(&owner) {
        return Status::REFUSED;
    }
    let more = keys.iter().map(|key| Waiting {
        owner,
        window: window.to_owned(),
        key: key.get(),
        change,
    });
    if wait(more) {
        WINDOWS.shown.store(true, Ordering::Release);
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

/// Queues `told` for the mods to hear after the step, once a mod showed a
/// window; past the bound it is dropped, and the mod keeps that window
/// until its own bound lets it go.
fn tell(told: Told) {
    if !WINDOWS.shown.load(Ordering::Acquire) {
        return;
    }
    if queue(&mut WINDOWS.told.relock(), core::iter::once(told)) {
        WINDOWS.any.store(true, Ordering::Release);
    }
}

/// Player `key`'s client closed mod `id`'s window `window`: the server
/// renders it for them no more, until the mod shows it again, and the mod
/// hears of it after the step.
pub(crate) fn closed(loader: &Loader, key: u64, id: String, window: String) {
    if let Some((index, _)) = loader.find_mod(&id) {
        let owner = ModIndex::new(index);
        let close = Message::WindowClose {
            id,
            window: window.clone(),
        };
        note(&mut WINDOWS.held.relock(), key, owner, &close);
        tell(Told {
            key,
            owner: Some(owner),
            window,
        });
    }
}

/// Forgets the windows player `key`'s client held, when the player leaves;
/// the mods hear of it after the step.
pub(crate) fn left(key: u64) {
    WINDOWS.held.relock().retain(|(k, _, _)| *k != key);
    tell(Told {
        key,
        owner: None,
        window: String::new(),
    });
}

/// Tells the mods what `told` holds: a window's mod of its close, and
/// every mod with actions of a player who left.
fn tell_mods(loader: &Loader, told: Vec<Told>) {
    if told.is_empty() {
        return;
    }
    let declaring = crate::actions::declaring(loader);
    for Told { key, owner, window } in told {
        match owner {
            Some(owner) => crate::actions::window_closed(loader, owner, key, &window),
            None => {
                for &owner in &declaring {
                    crate::actions::window_closed(loader, owner, key, "");
                }
            }
        }
    }
}

/// On the server's game thread after its step: tells the mods of the
/// windows clients closed and the players who left, then carries each
/// waiting change to its player's client: a show or an update as the mod
/// renders it for that player, in their language, and a close where the
/// mod renders nothing for them. An update goes only to a client that
/// holds the window. A player not online is skipped, and so is one whose
/// client takes no messages, as one without Enshwick: the mod renders
/// nothing for it and the server notes nothing as held.
pub(crate) fn after_step(loader: &Loader) {
    if !WINDOWS.any.swap(false, Ordering::AcqRel) {
        return;
    }
    tell_mods(loader, core::mem::take(&mut *WINDOWS.told.relock()));
    let waiting = core::mem::take(&mut *WINDOWS.waiting.relock());
    if waiting.is_empty() {
        return;
    }
    let online = crate::world::players(loader).unwrap_or_default();
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
        if change == Change::Update && !holds(&WINDOWS.held.relock(), key, owner, &window) {
            continue;
        }
        let Some(player) = online.iter().find(|p| p.key == PlayerKey::new(key)) else {
            continue;
        };
        let text = if change == Change::Close {
            String::new()
        } else {
            let language = crate::actions::language_of(key);
            crate::actions::window(loader, owner, player, language, &window).unwrap_or_default()
        };
        if text.len() > enshwick::actions::window::MAX_BYTES
            && !WINDOWS.warned.swap(true, Ordering::Relaxed)
        {
            crate::log!(
                WARN,
                "windows: {}:{window} is {} bytes, past the bound, and closes; later ones close unnamed until the world goes",
                record.id,
                text.len()
            );
        }
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

/// Forgets the waiting changes and what clients hold, as when the server's
/// world goes away.
pub(crate) fn forget() {
    WINDOWS.waiting.relock().clear();
    WINDOWS.told.relock().clear();
    WINDOWS.any.store(false, Ordering::Release);
    WINDOWS.held.relock().clear();
    WINDOWS.shown.store(false, Ordering::Release);
    WINDOWS.warned.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            WINDOWS.told.relock().is_empty(),
            "no mod showed a window yet, so none hears of a leave"
        );
        WINDOWS.shown.store(true, Ordering::Release);
        left(8);
        let gone = Told {
            key: 8,
            owner: None,
            window: String::new(),
        };
        assert_eq!(*WINDOWS.told.relock(), [gone]);
        assert!(
            WINDOWS.any.load(Ordering::Acquire),
            "the step tells the mods"
        );
        forget();
        assert!(WINDOWS.told.relock().is_empty() && WINDOWS.held.relock().is_empty());
        assert!(!WINDOWS.shown.load(Ordering::Acquire));
    }
}
