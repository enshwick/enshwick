//! Keeps the windows' waiting changes and what each client holds, and
//! carries them on the server's game thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::PlayerKey;
use enshwick_core::abi::{Side, WaypointData};

use super::maps::{HeldMap, kept, note_map, run_at};
use super::{
    Change, Held, MAX_PLAYERS, Route, Told, Waiting, close_told, greeted, heard, holds, leaving,
    merged, message, note, pend, queue, route,
};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// The windows' state on the server.
struct Windows {
    waiting: Mutex<Vec<Waiting>>,
    /// Set while `waiting` or `told` may hold anything, so an idle step
    /// takes no lock.
    any: AtomicBool,
    /// The windows each client got and was not told to close.
    held: Mutex<Held>,
    /// The windows shown to players whose client takes no messages yet,
    /// carried at its hello.
    pending: Mutex<Held>,
    /// Closes and leaves the mods hear after the step.
    told: Mutex<Vec<Told>>,
    /// Set once the log named a window past the bound, so a mod that shows
    /// one again and again fills no log.
    warned: AtomicBool,
    /// The maps of the windows clients hold, the newest last.
    maps: Mutex<Vec<HeldMap>>,
    /// Set while `maps` holds any, so a step without one takes no lock.
    any_map: AtomicBool,
}

static WINDOWS: Windows = Windows {
    waiting: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
    held: Mutex::new(Vec::new()),
    pending: Mutex::new(Vec::new()),
    told: Mutex::new(Vec::new()),
    warned: AtomicBool::new(false),
    maps: Mutex::new(Vec::new()),
    any_map: AtomicBool::new(false),
};

/// Keeps only the maps of windows clients still hold.
fn keep_maps() {
    let held = WINDOWS.held.relock();
    let mut maps = WINDOWS.maps.relock();
    kept(&mut maps, &held);
    WINDOWS.any_map.store(!maps.is_empty(), Ordering::Release);
}

/// Whether a client holds a window with a map, so the waypoints are
/// watched for it.
pub(crate) fn holds_map() -> bool {
    WINDOWS.any_map.load(Ordering::Acquire)
}

/// The maps' runs of waypoint changes: a panic in one turns off only these.
pub(crate) static MAP_RUNS: Fuse = Fuse::new("map runs");

/// Runs what the waypoint `change` asks of the newest map its player
/// holds: an interface run from that player, admitted and checked as a
/// press of a window's button is. A cleared waypoint runs nothing.
pub(crate) fn waypoint_changed(loader: &Loader, change: &WaypointData) {
    if change.set == 0 {
        return;
    }
    let Some(run) = run_at(&WINDOWS.maps.relock(), change.key, change.position) else {
        return;
    };
    let Some(record) = loader.mods.get(run.owner.get()) else {
        return;
    };
    let args = enshwick_core::actions::sheet::encode_args(&run.args);
    crate::actions::player_actions::run(loader, run.key, &record.id, &run.action, &args, None);
}

/// Queues `more` and notes that a step has work; false past the bound.
fn wait(more: impl ExactSizeIterator<Item = Waiting>) -> bool {
    if !queue(&mut WINDOWS.waiting.relock(), more) {
        return false;
    }
    WINDOWS.any.store(true, Ordering::Release);
    true
}

/// Why a mod's window was not queued for its clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ShowWindowError {
    /// A client that hosts no game carries no windows.
    #[error("no game hosted here carries windows")]
    NotServing,
    /// A change value the ABI has none of.
    #[error("no such window change")]
    BadChange,
    /// A malformed window id, or more players than a world holds.
    #[error("the window id or the players are malformed")]
    BadWindow,
    /// The mod declared no actions.
    #[error("the mod declared no actions")]
    NoActions,
    /// The windows waiting for the step fill their queue.
    #[error("too many windows wait")]
    Full,
}

/// Queues mod `owner`'s window `window` for the clients of `keys`, at most
/// [`MAX_PLAYERS`], for the step: to show, close or update as `change`
/// says, a `close` value of `HostApi::action_window`.
///
/// # Errors
///
/// [`ShowWindowError`] where no windows are carried, for a bad change, id or
/// crowd, a mod without actions or a full queue.
pub(crate) fn show(
    loader: &Loader,
    owner: ModIndex,
    window: &str,
    keys: &[PlayerKey],
    change: u32,
) -> Result<(), ShowWindowError> {
    if loader.side == Side::CLIENT && !crate::engine::world::hosting() {
        return Err(ShowWindowError::NotServing);
    }
    let Some(change) = Change::of(change) else {
        return Err(ShowWindowError::BadChange);
    };
    if !enshwick_core::actions::is_valid_id(window) || keys.len() > MAX_PLAYERS {
        return Err(ShowWindowError::BadWindow);
    }
    if !crate::actions::player_actions::declaring(loader).contains(&owner) {
        return Err(ShowWindowError::NoActions);
    }
    let more = keys.iter().map(|key| Waiting {
        owner,
        window: window.to_owned(),
        key: key.get(),
        change,
    });
    if wait(more) {
        Ok(())
    } else {
        Err(ShowWindowError::Full)
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
        keep_maps();
        tell(told);
    }
}

/// Forgets the windows player `key`'s client held and those waiting for
/// its hello, when the player leaves; the mods hear of it after the step.
pub(crate) fn left(key: u64) {
    WINDOWS.held.relock().retain(|(k, _, _)| *k != key);
    WINDOWS.pending.relock().retain(|(k, _, _)| *k != key);
    keep_maps();
    tell(Told::Left { key });
}

/// Player `key`'s client said hello and was welcomed, on a link that holds
/// no window: those it held and those shown to it while it took no
/// messages go after the step, before newer changes; the mods hear of
/// those with no room.
pub(crate) fn hello(key: u64) {
    let no_room = {
        let mut held = WINDOWS.held.relock();
        let mut pending = WINDOWS.pending.relock();
        let mut waiting = WINDOWS.waiting.relock();
        let no_room = greeted(&mut held, &mut pending, &mut waiting, key);
        if !waiting.is_empty() {
            WINDOWS.any.store(true, Ordering::Release);
        }
        no_room
    };
    if !no_room.is_empty() {
        crate::log!(DEBUG, "windows: too many wait to show {key:016x} theirs");
    }
    no_room.into_iter().for_each(tell);
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
/// window. A player not online is skipped. A change for a player whose
/// client takes no messages yet waits for its hello: the mod renders
/// nothing for it until then. Then tells the mods of the
/// players who left and of every window a client no longer holds: closed
/// by the client, let go for a newer one, or closed by a render; not one
/// its mod closed itself.
pub(crate) fn after_step(loader: &Loader) {
    if !WINDOWS.any.swap(false, Ordering::AcqRel) {
        return;
    }
    let (mut told, waiting) = taken();
    carry(loader, waiting, &mut told);
    keep_maps();
    let told = heard(told, &WINDOWS.held.relock());
    tell_mods(loader, told);
}

/// Carries `waiting` to the clients, adding to `told` the windows a client
/// closed for it, or let go while its hello was on its way.
fn carry(loader: &Loader, waiting: Vec<Waiting>, told: &mut Vec<Told>) {
    if waiting.is_empty() {
        return;
    }
    let online = crate::engine::world::players(loader).unwrap_or_default();
    for waiting in waiting {
        let Some(record) = loader.mods.get(waiting.owner.get()) else {
            continue;
        };
        let Some(player) = online.iter().find(|p| p.key == PlayerKey::new(waiting.key)) else {
            continue;
        };
        let ready = crate::base::net::welcomed(waiting.key);
        let route = route(&waiting, ready, || {
            holds(
                &WINDOWS.held.relock(),
                waiting.key,
                waiting.owner,
                &waiting.window,
            )
        });
        match route {
            Route::Skip => continue,
            Route::Pend => {
                let mut held = WINDOWS.held.relock();
                told.extend(pend(&mut held, &mut WINDOWS.pending.relock(), waiting));
                continue;
            }
            Route::Carry => {}
        }
        let Waiting {
            owner,
            window,
            key,
            change,
        } = waiting;
        let text = if change == Change::Close {
            String::new()
        } else {
            let language = crate::base::net::language_number(key);
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
            note_map(&mut WINDOWS.maps.relock(), key, owner, &message);
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

/// Forgets the waiting changes, the shows waiting for a hello and what
/// clients hold, as when the server's world goes away; the mods of
/// `loader` hear that every player who held a window, had one waiting for
/// their hello or was waiting to be told of one left.
pub(crate) fn forget(loader: Option<&Loader>) {
    WINDOWS.waiting.relock().clear();
    let told = core::mem::take(&mut *WINDOWS.told.relock());
    WINDOWS.any.store(false, Ordering::Release);
    let mut held = core::mem::take(&mut *WINDOWS.held.relock());
    held.append(&mut WINDOWS.pending.relock());
    WINDOWS.maps.relock().clear();
    WINDOWS.any_map.store(false, Ordering::Release);
    WINDOWS.warned.store(false, Ordering::Relaxed);
    if let Some(loader) = loader {
        tell_mods(loader, leaving(told, &held));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use core::ffi::c_void;

    use enshwick_core::abi::{ActionCall, PlayerInfo, Str, WINDOW_SHOW};

    use super::*;
    use crate::base::remote::Message;
    use crate::engine::registry::JoinFeatureError;

    /// One test at a time over [`WINDOWS`], the process's own, each from a
    /// world that went away.
    fn turn() -> std::sync::MutexGuard<'static, ()> {
        static TURN: Mutex<()> = Mutex::new(());
        let turn = TURN.relock();
        forget(None);
        turn
    }

    /// The mod [`Serving`] runs, its windows and the player it shows them to.
    pub(crate) const MOD: &str = "test.windows";
    pub(crate) const WINDOW: &str = "welcome";
    pub(crate) const KEY: u64 = 7;
    const TEXT: &str = "Welcome, Ann";

    /// The window ids the mod heard closed, in order; empty for a leave.
    static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// The mod's actions: every window reads [`TEXT`], and every close or
    /// leave it hears goes to [`HEARD`].
    unsafe extern "system" fn actions(call: *const ActionCall, _: *mut c_void, reply: *mut Str) {
        // SAFETY: the loader hands a live call for the length of this one.
        let call = unsafe { &*call };
        // SAFETY: the call's strings are borrowed for that call.
        let window = unsafe { call.action.as_str() };
        let kind = call.kind;
        if kind == ActionCall::WINDOW {
            // SAFETY: the loader hands a reply slot it reads after the call;
            // `TEXT` lives for the program.
            unsafe { *reply = Str::new(TEXT) };
        } else if kind == ActionCall::WINDOW_CLOSED {
            HEARD.relock().push(window.unwrap_or_default().to_owned());
        }
    }

    /// A server running [`MOD`], whose windows read [`TEXT`], with player
    /// [`KEY`] online, its client's messages recorded and without its hello
    /// until [`Serving::greet`]; dropped, it leaves the windows, the
    /// players, the peers and the carrier as it found them.
    pub(crate) struct Serving {
        pub(crate) loader: &'static Loader,
        _turn: std::sync::MutexGuard<'static, ()>,
        _link: std::sync::MutexGuard<'static, ()>,
    }

    #[expect(
        clippy::unused_self,
        reason = "each step takes the harness, so it runs only while the harness holds its locks"
    )]
    impl Serving {
        /// A server running [`MOD`] on this test's thread alone, so tests
        /// sharing a process keep their own loader or none.
        pub(crate) fn new() -> Self {
            let link = crate::base::carrier::tests::alone();
            let turn = turn();
            let loader =
                crate::engine::loader::install_for_test(crate::start::tests::loader_running(&[
                    MOD,
                ]));
            let registered = crate::actions::player_actions::register(
                ModIndex::new(0),
                actions,
                core::ptr::null_mut(),
            );
            assert!(
                matches!(registered, Ok(()) | Err(JoinFeatureError::Taken)),
                "{registered:?}"
            );
            let ann = PlayerInfo::without_character(PlayerKey::new(KEY), 0, "Ann");
            crate::engine::world::put_online(Some(vec![ann]));
            crate::base::carrier::host_for_test(KEY);
            crate::base::net::set_ready(KEY, false);
            HEARD.relock().clear();
            Self {
                loader,
                _turn: turn,
                _link: link,
            }
        }

        /// Shows [`WINDOW`] to [`KEY`] for the next step.
        pub(crate) fn show(&self) {
            let shown = show(
                self.loader,
                ModIndex::new(0),
                WINDOW,
                &[PlayerKey::new(KEY)],
                WINDOW_SHOW,
            );
            assert_eq!(shown, Ok(()));
        }

        /// [`KEY`]'s client says hello and is welcomed.
        pub(crate) fn greet(&self) {
            crate::base::net::set_ready(KEY, true);
            hello(KEY);
        }

        /// The messages [`KEY`]'s client got since the last look.
        pub(crate) fn sent(&self) -> Vec<Message> {
            crate::base::carrier::tests::sent_to_host()
                .into_iter()
                .filter_map(|(_, data)| crate::base::wire::remote_message(&data))
                .collect()
        }

        /// The windows [`KEY`]'s client holds and those waiting for its hello.
        pub(crate) fn windows(&self) -> (Held, Held) {
            (
                WINDOWS.held.relock().clone(),
                WINDOWS.pending.relock().clone(),
            )
        }

        /// What the mod heard closed since the last look.
        pub(crate) fn heard(&self) -> Vec<String> {
            core::mem::take(&mut *HEARD.relock())
        }

        /// [`MOD`]'s window [`WINDOW`] as its client gets it.
        pub(crate) fn opened() -> Message {
            Message::Window {
                id: MOD.to_owned(),
                window: WINDOW.to_owned(),
                open: true,
                text: TEXT.to_owned(),
            }
        }
    }

    impl Drop for Serving {
        fn drop(&mut self) {
            forget(None);
            crate::base::net::set_ready(KEY, false);
            crate::engine::world::put_online(None);
            crate::base::carrier::unhost_for_test();
            let _sent = crate::base::carrier::tests::sent_to_host();
        }
    }

    fn held_by_key() -> Held {
        vec![(KEY, ModIndex::new(0), WINDOW.to_owned())]
    }

    #[test]
    fn a_show_waits_for_a_client_without_its_hello_and_goes_to_it_after_the_hello() {
        let serving = Serving::new();
        serving.show();
        after_step(serving.loader);
        assert!(serving.sent().is_empty(), "no hello, nothing sent");
        assert_eq!(serving.windows(), (Vec::new(), held_by_key()));

        serving.greet();
        after_step(serving.loader);
        assert_eq!(serving.sent(), [Serving::opened()]);
        assert_eq!(serving.windows(), (held_by_key(), Vec::new()));
        assert!(serving.heard().is_empty(), "nothing closed");
    }

    #[test]
    fn a_show_goes_at_once_to_a_client_that_said_hello_and_not_to_a_player_offline() {
        let serving = Serving::new();
        serving.greet();
        crate::engine::world::put_online(Some(Vec::new()));
        serving.show();
        after_step(serving.loader);
        assert!(serving.sent().is_empty(), "the player is not online");
        assert_eq!(serving.windows(), (Vec::new(), Vec::new()));

        let ann = PlayerInfo::without_character(PlayerKey::new(KEY), 0, "Ann");
        crate::engine::world::put_online(Some(vec![ann]));
        serving.show();
        after_step(serving.loader);
        assert_eq!(serving.sent(), [Serving::opened()]);
        after_step(serving.loader);
        assert!(serving.sent().is_empty(), "an idle step sends nothing");
    }

    #[test]
    fn a_client_s_close_lets_its_window_go_and_its_mod_hears_after_the_step() {
        let serving = Serving::new();
        serving.greet();
        serving.show();
        after_step(serving.loader);
        assert_eq!(serving.sent(), [Serving::opened()]);

        closed(serving.loader, KEY, "you.unknown", WINDOW.to_owned());
        closed(serving.loader, KEY, MOD, "other".to_owned());
        assert_eq!(
            serving.windows().0,
            held_by_key(),
            "a close of an unknown mod or a window not held is dropped"
        );
        closed(serving.loader, KEY, MOD, WINDOW.to_owned());
        assert_eq!(serving.windows(), (Vec::new(), Vec::new()));
        assert!(serving.heard().is_empty(), "the mod hears after the step");
        after_step(serving.loader);
        assert_eq!(serving.heard(), [WINDOW]);
        assert!(
            serving.sent().is_empty(),
            "a client's own close goes back to no one"
        );
    }

    #[test]
    fn a_world_s_end_forgets_every_window_and_tells_the_mods_each_player_left() {
        let serving = Serving::new();
        serving.show();
        after_step(serving.loader);
        assert_eq!(serving.windows().1, held_by_key(), "waiting for the hello");
        forget(Some(serving.loader));
        assert_eq!(serving.windows(), (Vec::new(), Vec::new()));
        assert_eq!(serving.heard(), [""], "a leave, not a close");

        serving.greet();
        after_step(serving.loader);
        assert!(serving.sent().is_empty(), "a forgotten show is not carried");
        forget(Some(serving.loader));
        assert!(serving.heard().is_empty(), "no window, no one to tell");
    }

    #[test]
    fn a_run_updates_the_windows_its_player_holds_of_that_mod_and_a_leave_forgets_them() {
        let _turn = turn();
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
        let _turn = turn();
        let mut loader = crate::start::tests::bare_loader();
        let owner = ModIndex::new(0);
        let one = [PlayerKey::new(7)];
        let crowd = vec![PlayerKey::new(7); MAX_PLAYERS + 1];
        let bad = Err(ShowWindowError::BadChange);
        assert_eq!(
            show(&loader, owner, "panel", &one, 3),
            bad,
            "an unknown change"
        );
        assert_eq!(
            show(&loader, owner, "Bad Id", &one, 0),
            Err(ShowWindowError::BadWindow)
        );
        assert_eq!(
            show(&loader, owner, "panel", &crowd, 0),
            Err(ShowWindowError::BadWindow),
            "too many players"
        );
        assert_eq!(
            show(&loader, owner, "panel", &crowd[..MAX_PLAYERS], 2),
            Err(ShowWindowError::NoActions),
            "the bound itself passes, and a mod that declared no actions is refused"
        );
        loader.side = Side::CLIENT;
        assert_eq!(
            show(&loader, owner, "panel", &one, 0),
            Err(ShowWindowError::NotServing),
            "a client that hosts no game carries no windows"
        );
        assert!(WINDOWS.waiting.relock().is_empty());
    }

    #[test]
    fn what_waits_is_taken_out_of_its_locks_so_a_mod_told_of_it_may_queue_more() {
        let _turn = turn();
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
