//! The join hook on the client.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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

/// The end of a session a join started: once the provider's session ends,
/// the client gets its own world back, even when play never started or the
/// interface does not read the stage.
static ENDED_FUSE: Fuse = Fuse::new("join end");

/// The provider's state with no session: its leave and every online error,
/// after the game's dialog, move its state tree there; a session, hosted or
/// joined, is state 2 (client 26430625, read in the decompiler).
const NO_SESSION: u32 = 1;

/// Links the join end on the provider's state tree transition; true when
/// linked.
pub(crate) fn link_ended(loader: &Loader) -> bool {
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
    /// Whether the provider took a join the check let through and its
    /// session has not ended since: set once the provider's join returns,
    /// so the end of a session it replaced is not this one's.
    joined: AtomicBool,
}

static STATE: State = State {
    refusals: AtomicU64::new(0),
    terrain: OnceLock::new(),
    joined: AtomicBool::new(false),
};

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
    /// The session of a join ended: whether a host's table is still served.
    /// A game played with it ends no more.
    pub(crate) left: fn() -> bool,
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
/// reads the app's stage: once a game played with a host's terrain ids
/// ends, the client gets its own world back as before a game it hosts, so
/// one whose hosted start the loader does not see still serves its own ids.
pub(crate) fn play_changed(playing: bool) {
    if terrain_ids().is_some_and(|ids| (ids.play_changed)(playing)) {
        // That game's session ends too, or ended already: it switches back once.
        STATE.joined.store(false, Ordering::Release);
        own_world_again();
    }
}

/// The provider's state tree moved to `state`: once the session of a join
/// ends, as it fails, is refused, is left or drops before play starts too,
/// a host's table still served goes as after a game played with it.
fn session_changed(state: u32) {
    if !join_ended(state) {
        return;
    }
    crate::log!(INFO, "the joined session ended");
    if terrain_ids().is_some_and(|ids| (ids.left)()) {
        own_world_again();
    }
}

/// Whether `state` ends the session of the last join the provider took,
/// once per join.
fn join_ended(state: u32) -> bool {
    state == NO_SESSION && STATE.joined.swap(false, Ordering::AcqRel)
}

/// Gives the client its own world back as before a game it hosts, on a
/// thread of its own, off the game's frame.
fn own_world_again() {
    crate::engine::off_thread::start("own world", || {
        let _ran = LEFT_FUSE.run("the client's own world", || {
            crate::log!(INFO, "back from a joined game: own terrain ids again");
            hosting_starts();
        });
    });
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
                STATE.joined.store(true, Ordering::Release);
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
    /// session (2) and its leave and every online error, after the game's
    /// dialog, with none ([`NO_SESSION`]); it takes the provider's tree and
    /// the state's index and returns whether the transition started (client
    /// 26430625, read in the decompiler). The game calls it on the thread
    /// that moves the provider.
    fn transition(tree: *mut u8, state: u32) -> u64;
    next TRANSITION_NEXT;
    fuse &ENDED_FUSE, "the join end";
    |onward| {
        let started = onward.call();
        session_changed(state);
        started
    }
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
            crate::ui::interface::show("Enshwick", &note, "", 1 << UiDialog::OK, |_| {});
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
    crate::ui::interface::show("Enshwick", text, "", 1 << UiDialog::OK, |_| {});
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
    if crate::ui::interface::mismatch_screen_on() {
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
    crate::ui::interface::show(
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
    fn a_session_end_ends_only_a_join_the_provider_took_and_only_once() {
        assert!(!join_ended(NO_SESSION), "a hosted session's end");
        STATE.joined.store(true, Ordering::Release);
        assert!(!join_ended(2), "the joined session starts");
        assert!(join_ended(NO_SESSION), "and ends before play");
        assert!(!join_ended(NO_SESSION), "only once per join");
    }

    #[test]
    fn the_join_end_links_on_the_provider_s_state_tree_transition() {
        let loader = crate::start::tests::pinning_loader(
            Side::CLIENT,
            &[pinned::net::PROVIDER_TRANSITION],
            &[],
        );
        assert!(link_ended(&loader));
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
    fn a_build_without_the_transition_turns_off_only_the_join_end() {
        assert!(!link_ended(&crate::start::tests::bare_loader()));
        assert!(ENDED_FUSE.is_off());
        assert!(!JOIN_FUSE.is_off());
        assert!(!LEFT_FUSE.is_off());
    }
}
