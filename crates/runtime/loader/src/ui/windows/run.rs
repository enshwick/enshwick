//! Keeps the windows' waiting changes and what each client holds, and
//! carries them on the server's game thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::PlayerKey;
use enshwick_core::abi::{Side, Status};

use super::{
    Change, Held, MAX_PLAYERS, Told, Waiting, close_told, heard, holds, leaving, merged, message,
    note, queue,
};
use crate::engine::mod_index::ModIndex;
use crate::host::Loader;
use enshwick_base::Relock;

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
    /// Set once the log named a window past the bound, so a mod that shows
    /// one again and again fills no log.
    warned: AtomicBool,
}

static WINDOWS: Windows = Windows {
    waiting: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
    held: Mutex::new(Vec::new()),
    told: Mutex::new(Vec::new()),
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
    if loader.side == Side::CLIENT && !crate::engine::world::hosting() {
        return Status::MISSING;
    }
    let Some(change) = Change::of(change) else {
        return Status::BAD_ARGS;
    };
    if !enshwick_core::actions::is_valid_id(window) || keys.len() > MAX_PLAYERS {
        return Status::BAD_ARGS;
    }
    if !crate::actions::player_actions::declaring(loader).contains(&owner) {
        return Status::REFUSED;
    }
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

/// Queues `told` for the mods to hear after the step; past the bound it is
/// dropped, and the mod keeps that window until its own bound lets it go.
fn tell(told: Told) {
    if queue(&mut WINDOWS.told.relock(), core::iter::once(told)) {
        WINDOWS.any.store(true, Ordering::Release);
    }
}

/// Player `key`'s client closed mod `id`'s window `window`: the server
/// renders it for them no more, until the mod shows it again, and the mod
/// hears of it after the step. A close of a window the client does not
/// hold is dropped: the message is the client's own, at no rate.
pub(crate) fn closed(loader: &Loader, key: u64, id: &str, window: String) {
    let Some((index, _)) = loader.find_mod(id) else {
        return;
    };
    let told = close_told(
        &mut WINDOWS.held.relock(),
        key,
        ModIndex::new(index),
        window,
    );
    if let Some(told) = told {
        tell(told);
    }
}

/// Forgets the windows player `key`'s client held, when the player leaves;
/// the mods hear of it after the step.
pub(crate) fn left(key: u64) {
    WINDOWS.held.relock().retain(|(k, _, _)| *k != key);
    tell(Told::Left { key });
}

/// Tells the mods what `told` holds: a window's mod of its close, and
/// every mod with actions of a player who left.
fn tell_mods(loader: &Loader, told: Vec<Told>) {
    for told in told {
        match told {
            Told::Closed { key, owner, window } => {
                crate::actions::player_actions::window_closed(loader, key, Some((owner, &window)));
            }
            Told::Left { key } => crate::actions::player_actions::window_closed(loader, key, None),
        }
    }
}

/// What waits for the step, out of its locks, so a mod told of a close or
/// a leave may show a window, and the game may remove a player, while the
/// mods hear: the closes and leaves, and the merged changes.
fn taken() -> (Vec<Told>, Vec<Waiting>) {
    let told = core::mem::take(&mut *WINDOWS.told.relock());
    let waiting = merged(core::mem::take(&mut *WINDOWS.waiting.relock()));
    (told, waiting)
}

/// On the server's game thread after its step: carries each waiting change
/// to its player's client: a show or an update as the mod renders it for
/// that player, in their language, and a close where the mod renders
/// nothing for them. An update goes only to a client that holds the
/// window. A player not online is skipped, and so is one whose client
/// takes no messages, as one without Enshwick: the mod renders nothing for
/// it and the server notes nothing as held. Then tells the mods of the
/// players who left and of every window a client no longer holds: closed
/// by the client, let go for a newer one, or closed by a render; not one
/// its mod closed itself.
pub(crate) fn after_step(loader: &Loader) {
    if !WINDOWS.any.swap(false, Ordering::AcqRel) {
        return;
    }
    let (mut told, waiting) = taken();
    carry(loader, waiting, &mut told);
    let told = heard(told, &WINDOWS.held.relock());
    tell_mods(loader, told);
}

/// Carries `waiting` to the clients, adding to `told` the windows a client
/// closed for it.
fn carry(loader: &Loader, waiting: Vec<Waiting>, told: &mut Vec<Told>) {
    if waiting.is_empty() {
        return;
    }
    let online = crate::engine::world::players(loader).unwrap_or_default();
    for Waiting {
        owner,
        window,
        key,
        change,
    } in waiting
    {
        let Some(record) = loader.mods.get(owner.get()) else {
            continue;
        };
        if !crate::base::net::ready(key) {
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
            let language = crate::actions::player_actions::language_of(key);
            crate::actions::player_actions::window(loader, owner, player, language, &window)
                .unwrap_or_default()
        };
        if text.len() > enshwick_core::actions::window::MAX_BYTES
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
        if crate::base::net::to_player(key, &message) {
            let closed = note(&mut WINDOWS.held.relock(), key, owner, &message);
            told.extend(closed.filter(|_| change != Change::Close));
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
/// world goes away; the mods of `loader` hear that every player who held a
/// window or was waiting to be told of one left.
pub(crate) fn forget(loader: Option<&Loader>) {
    WINDOWS.waiting.relock().clear();
    let told = core::mem::take(&mut *WINDOWS.told.relock());
    WINDOWS.any.store(false, Ordering::Release);
    let held = core::mem::take(&mut *WINDOWS.held.relock());
    WINDOWS.warned.store(false, Ordering::Relaxed);
    if let Some(loader) = loader {
        tell_mods(loader, leaving(told, &held));
    }
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
        assert_eq!(
            *WINDOWS.told.relock(),
            [Told::Left { key: 7 }],
            "a leave is told whether or not a mod showed a window"
        );
        assert!(
            WINDOWS.any.load(Ordering::Acquire),
            "the step tells the mods"
        );
        forget(None);
        assert!(WINDOWS.told.relock().is_empty() && WINDOWS.held.relock().is_empty());
    }

    #[test]
    fn a_show_is_refused_for_a_bad_change_id_or_crowd_and_for_a_mod_without_actions() {
        let mut loader = crate::start::tests::bare_loader();
        let owner = ModIndex::new(0);
        let one = [PlayerKey::new(7)];
        let crowd = vec![PlayerKey::new(7); MAX_PLAYERS + 1];
        let bad = Status::BAD_ARGS;
        assert_eq!(
            show(&loader, owner, "panel", &one, 3),
            bad,
            "an unknown change"
        );
        assert_eq!(show(&loader, owner, "Bad Id", &one, 0), bad);
        assert_eq!(
            show(&loader, owner, "panel", &crowd, 0),
            bad,
            "too many players"
        );
        assert_eq!(
            show(&loader, owner, "panel", &crowd[..MAX_PLAYERS], 2),
            Status::REFUSED,
            "the bound itself passes, and a mod that declared no actions is refused"
        );
        loader.side = Side::CLIENT;
        assert_eq!(
            show(&loader, owner, "panel", &one, 0),
            Status::MISSING,
            "a client that hosts no game carries no windows"
        );
        assert!(WINDOWS.waiting.relock().is_empty());
    }

    #[test]
    fn what_waits_is_taken_out_of_its_locks_so_a_mod_told_of_it_may_queue_more() {
        let mine = ModIndex::new(0);
        let show = |key| Waiting {
            owner: mine,
            window: "panel".into(),
            key,
            change: Change::Show,
        };
        let closed = |key| Told::Closed {
            key,
            owner: mine,
            window: "panel".into(),
        };
        tell(closed(7));
        tell(closed(8));
        assert!(wait(core::iter::once(show(7))));

        let (told, waiting) = taken();
        assert_eq!(told, [closed(7), closed(8)]);
        assert_eq!(waiting, [show(7)]);
        let held = [(7, mine, "panel".to_owned())];
        assert_eq!(
            heard(told, &held),
            [closed(8)],
            "7 was shown the window again"
        );
        // As a mod's handler does while it hears: the game removes a player
        // it kicked, and it shows a window.
        left(9);
        assert!(wait(core::iter::once(show(8))));
        assert_eq!(*WINDOWS.told.relock(), [Told::Left { key: 9 }]);
        assert_eq!(*WINDOWS.waiting.relock(), [show(8)]);
    }
}
