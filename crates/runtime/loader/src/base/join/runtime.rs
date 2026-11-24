//! The join hook on the client.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use enshwick_core::abi::UiDialog;

use enshwick_core::abi::HookFlags;

use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;

/// The join check: a join from the game's menus to a server whose mods the
/// client lacks is refused before the game connects.
pub(crate) static JOIN_FUSE: Fuse = Fuse::new("join check");

/// Links the join check on `join`, the online provider's join the caller
/// found in its function table and checked; true when linked.
pub(crate) fn link(loader: &Loader, join: u64) -> bool {
    let target = Target::Found(pinned::net::JOIN_GAME.id(), Some(join));
    loader.link_feature(&JOIN_FUSE, target, &NEXT, detour, HookFlags::NONE)
}

/// The end of a session a join started before play: once the provider's
/// session ends, the client gets its own world back, even when the
/// interface does not read the stage.
pub(crate) static ENDED_FUSE: Fuse = Fuse::new("join end");

/// Links the join end on the provider's state tree transition, once the
/// build names the provider's states with a session and with none, and they
/// differ; true when linked.
pub(crate) fn link_ended(loader: &Loader) -> bool {
    // Both states' routes go through the transition, so a missing transition
    // is left to the link below to name. The session state is also the
    // transition's second witness: the leave's route alone (vtable slot 17)
    // does not prove the target, so the link needs both states.
    if loader.address(pinned::net::PROVIDER_TRANSITION).is_some() {
        match no_session_state(loader) {
            Ok(no_session) => {
                // The loader links once per process.
                let _first = STATE.no_session.set(no_session);
            }
            Err((id, reason)) => return loader.fuse_off(&ENDED_FUSE, id, reason),
        }
    }
    let target = Target::Pinned(pinned::net::PROVIDER_TRANSITION);
    loader.link_feature(
        &ENDED_FUSE,
        target,
        &TRANSITION_NEXT,
        transition,
        HookFlags::NONE,
    )
}

static NAMES: OnceLock<super::Names> = OnceLock::new();
static LAYOUT: OnceLock<super::Layout> = OnceLock::new();

/// Sets the build's join target layout, before the hook links.
pub(crate) fn set_layout(layout: super::Layout) {
    let _first = LAYOUT.set(layout);
}

/// The build's join target layout, once the join check linked it.
/// Why the join end turns off when the build's no-session state equals its
/// session state.
const SAME_STATE: &str = "is the session's state, not the one with none";

/// The build's provider state with no session, once the build names it and
/// the session state and the two differ; else the entry to name and why.
fn no_session_state(loader: &Loader) -> Result<u32, (&'static str, &'static str)> {
    let state = |id: pinned::Constant| {
        loader
            .build
            .constant(id.id())
            .and_then(|state| u32::try_from(state).ok())
            .ok_or((id.id(), crate::engine::builds::ABSENT))
    };
    let session = state(pinned::net::SESSION_STATE)?;
    let no_session = state(pinned::net::NO_SESSION_STATE)?;
    if no_session == session {
        return Err((pinned::net::NO_SESSION_STATE.id(), SAME_STATE));
    }
    Ok(no_session)
}

pub(crate) fn layout() -> Option<super::Layout> {
    LAYOUT.get().copied()
}

/// The join check's counts, one per process.
struct State {
    /// How many joins the check refused, so a caller that started a join
    /// tells a refusal from another failure.
    refusals: AtomicU64,
    /// The terrain ids this side serves, once the layer that serves them
    /// set them.
    terrain: OnceLock<TerrainIds>,
    /// The last join the provider took, until it ends.
    join: Join,
    /// The provider's state with no session, which its leave and every
    /// online error move its state tree to, as the build names it
    /// (`net.no_session_state`), never the state with a session
    /// (`net.session_state`): 1 and 2 on every kept client (client 26430625,
    /// read in the decompiler).
    no_session: OnceLock<u32>,
}

static STATE: State = State {
    refusals: AtomicU64::new(0),
    terrain: OnceLock::new(),
    join: Join::new(),
    no_session: OnceLock::new(),
};

/// The last join the provider took, one record for its whole life, so it
/// ends once, by whichever comes first of its session's end before play
/// and its play's end.
#[derive(Debug)]
struct Join {
    /// [`Join::NONE`], [`Join::TAKEN`] or [`Join::PLAYING`].
    phase: AtomicU8,
    /// Joins the game started so far, so a switch back started for one
    /// does nothing once a newer one came.
    started: AtomicU64,
}

impl Join {
    /// No join, or the last one ended.
    const NONE: u8 = 0;
    /// The provider took the join; play has not started.
    const TAKEN: u8 = 1;
    /// Play started in the joined game.
    const PLAYING: u8 = 2;

    const fn new() -> Self {
        Self {
            phase: AtomicU8::new(Self::NONE),
            started: AtomicU64::new(0),
        }
    }

    /// The game started a join.
    fn start(&self) {
        self.started.fetch_add(1, Ordering::AcqRel);
    }

    /// The number of the last join the game started.
    fn last(&self) -> u64 {
        self.started.load(Ordering::Acquire)
    }

    /// Whether no join started after the one numbered `last`.
    fn still(&self, last: u64) -> bool {
        self.last() == last
    }

    /// The provider took the join: set once its join returns, so the end
    /// of a session it replaced is not this one's.
    fn taken(&self) {
        self.phase.store(Self::TAKEN, Ordering::Release);
    }

    /// Play started; a join taken now plays.
    fn play_started(&self) {
        let _was_taken = self.phase.compare_exchange(
            Self::TAKEN,
            Self::PLAYING,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Play ended: whether it ended the join's game, once per join. A join
    /// taken during a game is not ended by that game's end.
    fn play_ended(&self) -> bool {
        self.ends(Self::PLAYING)
    }

    /// The provider's session ended: whether it ended a join before its
    /// play, once per join. One that ends during play waits for the play's
    /// end, since the joined world is still loaded.
    fn session_ended(&self) -> bool {
        self.ends(Self::TAKEN)
    }

    fn ends(&self, phase: u8) -> bool {
        self.phase
            .compare_exchange(phase, Self::NONE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// The terrain ids this side serves, as the layer that serves terrain hands
/// them to the join check, the lobby and the advertisement: a joining
/// client must name each terrain id as its host does.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainIds {
    /// The table served now, as the host's rules and lobby carry it.
    pub(crate) served: fn() -> String,
    /// Takes a host's table before the game connects, `None` for a host
    /// that serves none it can read; the player's text, naming who hosts,
    /// when a mod of this side now reads a wrong id.
    pub(crate) adopt: fn(Option<&str>, crate::base::join::Host) -> Option<String>,
    /// Serves this side's own table again.
    pub(crate) own_again: fn(),
    /// Play started (`true`) or ended: whether it ended in a game played
    /// with a host's table, once per such game.
    pub(crate) play_changed: fn(bool) -> bool,
}

/// Serving this side's own world again once a joined game ends: a panic
/// there turns off only that, and the hosted start still does it.
static LEFT_FUSE: Fuse = Fuse::new("own world after a joined game");

/// Sets the terrain ids this side serves; the first call holds.
pub(crate) fn set_terrain_ids(ids: TerrainIds) {
    let _first = STATE.terrain.set(ids);
}

/// The terrain ids this side serves, `None` while it serves no terrain.
pub(crate) fn terrain_ids() -> Option<TerrainIds> {
    STATE.terrain.get().copied()
}

/// Serves this side's own terrain ids again, when it serves terrain.
fn own_terrain_again() {
    if let Some(ids) = terrain_ids() {
        (ids.own_again)();
    }
}

/// Before a game this client hosts reads its resources: the mods the last
/// joined world turned off come back on first, since the client's own
/// terrain ids serve only the mods that are on.
pub(crate) fn hosting_starts() {
    crate::base::join::client::back_on();
    own_terrain_again();
}

/// The client's play started (`playing`) or ended, as the interface frame
/// reads the app's stage: once a joined game, or one played with a host's
/// terrain ids, ends, the client gets its own world back as before a game
/// it hosts, so one whose hosted start the loader does not see still serves
/// its own ids with its own mods.
pub(crate) fn play_changed(playing: bool) {
    let ended = if playing {
        STATE.join.play_started();
        false
    } else {
        STATE.join.play_ended()
    };
    let host_table = terrain_ids().is_some_and(|ids| (ids.play_changed)(playing));
    if ended || host_table {
        own_world_again();
    }
}

/// The session of the last join the provider took ended before play, as
/// it failed, was refused or was left: the client's own world comes back
/// at once, since no world of the join is loaded.
fn session_ended() {
    if STATE.join.session_ended() {
        crate::log!(INFO, "the joined session ended before play");
        own_world_again();
    }
}

/// Gives the client its own world back as before a game it hosts, on a
/// thread of its own, off the game's frame, unless a newer join started
/// first; on this thread when no thread starts.
fn own_world_again() {
    let last = STATE.join.last();
    let back = move || {
        if !STATE.join.still(last) {
            crate::log!(INFO, "a newer join came first: no own world again");
            return;
        }
        let _ran = LEFT_FUSE.run("the client's own world", || {
            crate::log!(INFO, "back from a joined game: own terrain ids again");
            hosting_starts();
        });
    };
    if !crate::engine::off_thread::start("own world", back) {
        back();
    }
}

/// The count of joins the check refused so far.
pub(crate) fn refusals() -> u64 {
    STATE.refusals.load(Ordering::Acquire)
}

/// Sets the names of all the client's mods.
pub(crate) fn set_names(names: super::Names) {
    let _first = NAMES.set(names);
}

/// The names of all the client's mods.
pub(crate) fn names() -> super::Names {
    NAMES.get().cloned().unwrap_or_default()
}

crate::engine::chains::chain_detour! {
    /// The online provider's join at the table's `net.join_game`: every join
    /// from the game's menus, to a dedicated server or into a hosted game's
    /// lobby, passes a target ([`super::Target`]) through it, and it ends in
    /// the provider's state change, whose `bool` it returns in `al` (client
    /// builds 410 and 426). The game calls it on the thread that joins, with
    /// `target` null or the join target, readable and writable for the call
    /// up to its password at the build's [`super::Layout::head_len`]. A
    /// refused join reaches the rest of the chain with the target's host
    /// cleared, so the game connects nowhere.
    fn detour(provider: *mut u8, target: *mut u8) -> u8;
    next NEXT;
    fuse &JOIN_FUSE, "the join check";
    |onward| {
        // Only a join with a target leaves the lobby: a null pass names no new
        // game and may come while the client stays in its lobby.
        if !target.is_null() {
            STATE.join.start();
            crate::base::lobby::left();
        }
        let Some(layout) = LAYOUT.get() else {
            return onward.call();
        };
        let allowed = target.is_null() || {
            let mut head: super::Target = [0; super::TARGET_BYTES];
            // SAFETY: `target` is non-null, checked just before, and is the
            // join target the game passes the hooked join, plain data the
            // caller holds for the call up to its password at `head_len`
            // (client builds 410 and 426, the build table's `join.target_*`
            // rows), which `Layout` keeps within `head`.
            unsafe {
                enshwick_core::raw::read_bytes_at(
                    target.cast_const(),
                    0,
                    &mut head[..layout.head_len()],
                );
            };
            check(layout, &head)
        };
        if allowed {
            super::interaction::joining(false);
            let state_changed = onward.call();
            if !target.is_null() {
                STATE.join.taken();
            }
            return state_changed;
        }
        crate::log!(WARN, "join refused by the join check");
        STATE.refusals.fetch_add(1, Ordering::AcqRel);
        super::interaction::joining(true);
        // The provider copies the target into its own state before it
        // returns, and the caller keeps its target, so the caller's copy
        // gets its host back (client builds 410 and 426).
        // SAFETY: `target` is non-null, since a null one sets `allowed`,
        // which returns early, and is the join target the game passes the
        // hooked join, whose `u64` host the build's layout places before its
        // password.
        let host: u64 =
            unsafe { enshwick_core::raw::read_at(target.cast_const(), layout.host_at()) };
        // SAFETY: the same host, writable for the length of this call, and
        // the game reads it only once the chain's rest runs.
        unsafe { enshwick_core::raw::write_at(target, layout.host_at(), 0_u64) };
        let state_changed = onward.call();
        // SAFETY: the caller's target outlives this call, which its caller
        // makes with the target in hand, and the game holds no reference to
        // it once the provider returned (client builds 410 and 426); its
        // host id is a u64 at the layout's `host_at`.
        unsafe { enshwick_core::raw::write_at(target, layout.host_at(), host) };
        state_changed
    }
}

crate::engine::chains::chain_detour! {
    /// The online provider's state tree transition at the table's
    /// `net.provider_transition`, which the provider's join ends in with a
    /// session (`net.session_state`) and its leave and every online error,
    /// as the game's dialog opens, with none ([`ends_session`]); it takes the
    /// provider's tree and the state's index and returns whether the
    /// transition started (client 26430625, read in the decompiler). The game
    /// calls it on the thread that moves the provider.
    fn transition(tree: *mut u8, state: u32) -> u64;
    next TRANSITION_NEXT;
    fuse &ENDED_FUSE, "the join end";
    |onward| {
        let started = onward.call();
        if started != 0 && ends_session(state) {
            session_ended();
        }
        started
    }
}

/// Whether a transition to `state` ends the provider's session: `state` is
/// the build's no-session state.
fn ends_session(state: u32) -> bool {
    STATE.no_session.get() == Some(&state)
}

/// Joins a game whose mods the check cannot read: the client's mods stay
/// on, as the world it joins may run them.
fn unchecked() -> bool {
    crate::base::net::joining(false);
    crate::base::join::client::forget_server_values();
    crate::base::join::client::back_on();
    own_terrain_again();
    true
}

/// The address to ask a join's server at, `None` for a join the check
/// cannot ask about, with the reason logged.
fn query_address(layout: &super::Layout, target: &super::Target) -> Option<std::net::SocketAddr> {
    match layout.destination(target) {
        super::Destination::Server { query } => Some(query),
        super::Destination::Lobby => {
            // A hosted game's lobby: the lobby module reads its host's
            // mods once the client enters it.
            crate::log!(INFO, "joining a hosted game's lobby");
            None
        }
        super::Destination::NoQueryPort { game } => {
            crate::log!(
                WARN,
                "joining {game}: the game names no query port, so the join check cannot ask the server"
            );
            None
        }
    }
}

/// Compares mod sets, turns off what the server lacks and tells the player;
/// false when the client must not join. The server sends its settings once
/// the client joined.
fn check(layout: &super::Layout, target: &super::Target) -> bool {
    let Some(names) = NAMES.get() else {
        return true;
    };
    let Some(address) = query_address(layout, target) else {
        return unchecked();
    };
    let reply = super::query_rules(address).ok();
    let Some(reply) = reply else {
        // A server that does not answer the query: nothing to compare.
        crate::log!(INFO, "joining {address}: no rules answer");
        return unchecked();
    };
    // The last world's choice goes: this one decides again.
    crate::base::join::client::back_on();
    // Read now: a mod a failed hook turned off no longer counts as the
    // client's.
    let mods = &crate::base::join::client::join_mods();
    let server = super::rule(&reply, crate::base::advertise::RULE)
        .map(|fingerprint| (fingerprint, super::mod_rules_value(&reply)));
    crate::base::net::joining(server.is_some());
    let ours = crate::base::advertise::fingerprint(env!("CARGO_PKG_VERSION"), mods);
    let ours_mods = crate::base::advertise::mod_list(mods);
    crate::log!(INFO, "joining {address}: server {server:?}, ours {ours}");
    let complete = super::mods_complete(&reply);
    if !complete {
        crate::log!(
            WARN,
            "joining {address}: the server lists only part of its mods; mods it does not list stay on"
        );
    }
    let plan = super::plan(
        &ours,
        &ours_mods,
        server.as_ref().map(|(f, m)| (f.as_str(), m.as_str())),
        complete,
    );
    if plan.joins() {
        crate::base::join::client::forget_server_values();
    }
    for id in plan.turned_off() {
        crate::base::join::client::turn_off_for_world(id);
    }
    if plan.joins() {
        // A server that names no terrain ids serves none of the client's
        // terrain mods, which the plan turned off, or runs an Enshwick from
        // before the rule: the client keeps its own ids.
        let terrain = server
            .as_ref()
            .and_then(|_| super::terrain_rules_value(&reply));
        let moved = terrain_ids()
            .and_then(|ids| (ids.adopt)(terrain.as_deref(), crate::base::join::Host::Server));
        if let Some(text) = moved {
            own_terrain_again();
            terrain_refused(&text);
            return false;
        }
        if let Some(note) = plan.note(names) {
            // The join goes on, and the note shows over the game.
            crate::base::dialogs::show("Enshwick", &note, "", 1 << UiDialog::OK, |_| {});
        }
        return true;
    }
    let server_mods = server.as_ref().map_or("", |(_, m)| m.as_str());
    refused(&plan.missing, server_mods, &ours_mods, complete, names);
    false
}

/// Tells the player that the client's mods give terrain other ids than the
/// server, which keeps the client out: the ground would show wrong.
#[cold]
fn terrain_refused(text: &str) {
    crate::log!(
        WARN,
        "the join is refused ({}): {}",
        crate::engine::errors::JOIN_TERRAIN,
        text.replace("\n\n", " ").replace('\n', ", ")
    );
    crate::base::dialogs::show("Enshwick", text, "", 1 << UiDialog::OK, |_| {});
}

/// Tells the player which of the server's mods keep the client out, on the
/// mismatch screen in the menu, or in one dialog where this build cannot
/// show the screen, and tells the client's mods.
#[cold]
fn refused(
    missing: &[crate::base::admission::Need],
    server_mods: &str,
    ours_mods: &str,
    complete: bool,
    names: &super::Names,
) {
    let needed: Vec<String> = missing
        .iter()
        .map(|need| format!("{} {}", need.id, need.version))
        .collect();
    crate::log!(
        WARN,
        "the server runs mods this game lacks: {}; the join is refused ({})",
        needed.join(", "),
        crate::engine::errors::JOIN_MISSING
    );
    let off = crate::base::join::client::join_mods_off();
    let off_ids: Vec<String> = off.iter().map(|(id, _)| id.clone()).collect();
    // Mods that are off on the client are the player's to turn on.
    let wanted: Vec<crate::base::join::missing::Wanted> = missing
        .iter()
        .filter(|need| !off_ids.contains(&need.id))
        .map(|need| crate::base::join::missing::Wanted {
            id: need.id.clone(),
            version: need.version.clone(),
        })
        .collect();
    if crate::base::dialogs::mismatch_screen_on() {
        let off = crate::base::advertise::mod_list(&off);
        crate::base::join::client::show_difference(&super::difference(
            server_mods,
            ours_mods,
            &off,
            complete,
        ));
        crate::base::join::client::report_join_missing(&wanted);
        return;
    }
    crate::log!(
        INFO,
        "the mismatch screen is off on this build; the list shows in a dialog"
    );
    let dialog =
        super::refusal::dialog(missing, &super::refusal::Off { this_game: off_ids }, names);
    // The join is refused at once, so the game keeps drawing; the mods
    // hear of it once the player closes the dialog.
    crate::base::dialogs::show(
        &dialog.title,
        &dialog.message,
        &dialog.detail,
        1 << UiDialog::OK,
        move |_| crate::base::join::client::report_join_missing(&wanted),
    );
}

#[cfg(test)]
mod tests {
    use enshwick_base::Relock;
    use enshwick_core::abi::Side;

    use super::*;
    use crate::engine::mod_index::ModIndex;

    #[test]
    fn a_join_that_ends_before_play_ends_once_at_its_session_end() {
        let join = Join::new();
        assert!(!join.session_ended(), "a hosted session's end");
        join.taken();
        assert!(join.session_ended(), "the joined session ends before play");
        assert!(!join.session_ended(), "only once per join");
        assert!(!join.play_ended(), "nor again at a stage end");
    }

    #[test]
    fn a_join_whose_session_ends_during_play_ends_only_at_the_play_s_end() {
        let join = Join::new();
        join.taken();
        join.play_started();
        assert!(
            !join.session_ended(),
            "the session ends while the joined world is still loaded"
        );
        assert!(join.play_ended(), "the world is gone");
        assert!(!join.play_ended(), "only once per join");
    }

    #[test]
    fn a_join_taken_during_a_game_is_not_ended_by_that_game_s_end() {
        let join = Join::new();
        join.taken();
        join.play_started();
        join.taken();
        assert!(!join.play_ended(), "the earlier game ends");
        assert!(join.session_ended(), "the new join's session ends");
    }

    #[test]
    fn a_switch_back_started_before_a_newer_join_does_nothing() {
        let join = Join::new();
        let last = join.last();
        assert!(join.still(last), "no join since");
        join.start();
        assert!(!join.still(last), "a newer join started");
    }

    #[test]
    fn the_join_end_links_on_the_provider_s_state_tree_transition() {
        let loader = crate::start::tests::pinning_loader(
            Side::CLIENT,
            &[pinned::net::PROVIDER_TRANSITION],
            // The kept clients' states swapped and moved (session 2 to 1,
            // no session 1 to 3), so only the build's values pass.
            &[
                (pinned::net::SESSION_STATE, 1),
                (pinned::net::NO_SESSION_STATE, 3),
            ],
        );
        assert!(link_ended(&loader));
        assert!(ends_session(3), "the build's no-session state");
        assert!(!ends_session(1), "a session");
        assert!(!ends_session(2), "the kept clients' session");
        let at = loader
            .address(pinned::net::PROVIDER_TRANSITION)
            .unwrap_or_default();
        let chains = loader.chains.relock();
        assert_eq!(chains.owners(at), [ModIndex::LOADER]);
        let features: Vec<_> = chains
            .features_at(at)
            .into_iter()
            .map(Fuse::feature)
            .collect();
        assert_eq!(features, [ENDED_FUSE.feature()]);
    }

    #[test]
    fn a_build_whose_leave_names_no_state_turns_off_only_the_join_end() {
        let loader = crate::start::tests::pinning_loader(
            Side::CLIENT,
            &[pinned::net::PROVIDER_TRANSITION],
            &[(pinned::net::SESSION_STATE, 2)],
        );
        assert!(!link_ended(&loader));
        assert!(ENDED_FUSE.is_off());
        assert!(!JOIN_FUSE.is_off());
        assert!(!LEFT_FUSE.is_off());
        assert_eq!(off_entry(), pinned::net::NO_SESSION_STATE.id());
        let at = loader
            .address(pinned::net::PROVIDER_TRANSITION)
            .unwrap_or_default();
        assert!(loader.chains.relock().owners(at).is_empty());
    }

    #[test]
    fn a_build_whose_leave_names_the_session_s_state_turns_off_only_the_join_end() {
        // A renumbered tree: the session's state is no longer 2.
        let loader = crate::start::tests::pinning_loader(
            Side::CLIENT,
            &[pinned::net::PROVIDER_TRANSITION],
            &[
                (pinned::net::SESSION_STATE, 4),
                (pinned::net::NO_SESSION_STATE, 4),
            ],
        );
        assert!(!link_ended(&loader));
        assert!(ENDED_FUSE.is_off());
        assert!(!JOIN_FUSE.is_off());
        assert_eq!(off_entry(), pinned::net::NO_SESSION_STATE.id());
        assert!(off_reason().ends_with(SAME_STATE), "{}", off_reason());
    }

    #[test]
    fn a_build_whose_join_names_no_state_turns_off_only_the_join_end_naming_the_session_state() {
        let loader = crate::start::tests::pinning_loader(
            Side::CLIENT,
            &[pinned::net::PROVIDER_TRANSITION],
            &[(pinned::net::NO_SESSION_STATE, 1)],
        );
        assert!(!link_ended(&loader));
        assert!(ENDED_FUSE.is_off());
        assert!(!JOIN_FUSE.is_off());
        assert_eq!(off_entry(), pinned::net::SESSION_STATE.id());
    }

    #[test]
    fn a_build_without_the_transition_turns_off_only_the_join_end_naming_the_transition() {
        assert!(!link_ended(&crate::start::tests::bare_loader()));
        assert!(ENDED_FUSE.is_off());
        assert!(!JOIN_FUSE.is_off());
        assert!(!LEFT_FUSE.is_off());
        assert_eq!(off_entry(), pinned::net::PROVIDER_TRANSITION.id());
    }

    /// The entry the join end went off for, as the status summary lists it.
    fn off_entry() -> String {
        off_reason()
            .split_once(' ')
            .map_or_else(String::new, |(id, _)| id.to_owned())
    }

    /// The join end's off reason as the Mods screen and the log show it.
    fn off_reason() -> String {
        let parts = crate::engine::diag::parts_off();
        let Some(off) = parts.iter().find(|p| p.part == ENDED_FUSE.feature()) else {
            panic!("the join end is not listed off: {parts:?}");
        };
        off.reason.clone()
    }
}
