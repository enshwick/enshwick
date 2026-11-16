//! Whose world an engine pass hands the loader: the running server's, or on
//! a client its own. A client runs both while it hosts a game, each on its
//! own thread. The hosted server's thread runs its whole life: the state
//! changes from Initialize on and every step of its world, also those that
//! move its events while it loads, before the post-step link first sees it
//! (`ecs.event_merge` of the server's world comes only through `world.step`
//! from the server's step, which the server's update calls beside
//! `server.set_state`; client 26430625, read in the decompiler, and each
//! hosted start logs both threads to show it live). So the loader tells
//! worlds by that thread from the start: a world it moves is the server's
//! or nobody's, every other world the client's own. Should the post-step
//! link see the server step on another thread, that thread counts from
//! then on, with a warning.

use std::sync::atomic::{AtomicU32, Ordering};

use enshwick_core::abi::Side;

use crate::engine::off_thread::current_thread;

/// The hosted server a client runs, as far as its engine passes need it.
#[derive(Debug)]
pub(super) struct Hosted {
    /// The hosted server's thread, from its start until it leaves the
    /// game's server list; 0 for none.
    thread: AtomicU32,
}

impl Hosted {
    pub(super) const fn new() -> Self {
        Self {
            thread: AtomicU32::new(0),
        }
    }

    /// A hosted server starts on `thread`.
    fn starts(&self, thread: u32) {
        self.thread.store(thread, Ordering::Release);
    }

    /// The hosted server on `thread` left the game's server list; a newer
    /// server's thread stays.
    fn left(&self, thread: u32) {
        let _newer = self
            .thread
            .compare_exchange(thread, 0, Ordering::AcqRel, Ordering::Acquire);
    }

    /// The post-step link saw the hosted server step on `thread`: a start
    /// recorded on another thread moves there; the thread it left, if it
    /// did.
    fn stepped_on(&self, thread: u32) -> Option<u32> {
        let recorded = self.thread.load(Ordering::Acquire);
        if recorded == 0 || recorded == thread {
            return None;
        }
        self.thread
            .compare_exchange(recorded, thread, Ordering::AcqRel, Ordering::Acquire)
            .ok()
    }

    /// Whether `thread` runs a hosted server, from its start until it left.
    fn runs_on(&self, thread: u32) -> bool {
        thread != 0 && self.thread.load(Ordering::Acquire) == thread
    }
}

/// Whose world a pass over an entity world is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Whose {
    /// The running server's, on its game thread.
    Server,
    /// The client's own.
    Client,
}

/// A hosted server starts on this client, on the calling thread, which runs
/// it until it leaves the game's server list.
pub(crate) fn server_starts() {
    let me = current_thread();
    super::STATE.hosted.starts(me);
    crate::log!(INFO, "a hosted game starts on thread {me}");
}

/// The hosted server stepped its world on `thread`, as the post-step link
/// saw: should its start have been recorded on another thread, the worlds
/// are told by this one from now on.
pub(super) fn server_stepped(thread: u32) {
    if let Some(started) = super::STATE.hosted.stepped_on(thread) {
        crate::log!(
            WARN,
            "a hosted game started on thread {started} steps on thread {thread}; its worlds are told by the second"
        );
    }
}

/// The hosted server left the game's server list on this thread, after the
/// game freed it; its thread steps no world of it any more.
pub(crate) fn server_left() {
    super::STATE.hosted.left(current_thread());
}

/// Whose `entity_world` is, passed on this thread to the loader of `side`;
/// `None` for a world the loader cannot tell.
pub(crate) fn whose(side: Side, entity_world: usize) -> Option<Whose> {
    let me = current_thread();
    let state = &super::STATE;
    tell(Seen {
        side,
        world: entity_world,
        server: usize::try_from(super::server_world()).unwrap_or(0),
        on_game_thread: super::hosting() && me == state.game_thread.load(Ordering::Acquire),
        on_hosted_thread: state.hosted.runs_on(me),
    })
}

/// What the loader knows when an engine pass hands it a world.
#[derive(Debug, Clone, Copy)]
struct Seen {
    side: Side,
    world: usize,
    /// The running server's entity world, 0 for none seen.
    server: usize,
    /// On the live server's game thread.
    on_game_thread: bool,
    /// On the thread of a hosted server, from its start until it left.
    on_hosted_thread: bool,
}

fn tell(seen: Seen) -> Option<Whose> {
    if seen.world == 0 {
        None
    } else if seen.server != 0 && seen.world == seen.server {
        seen.on_game_thread.then_some(Whose::Server)
    } else if seen.side == Side::CLIENT && !seen.on_hosted_thread {
        Some(Whose::Client)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: usize = 0x1000; // bound, not a game offset: a test's own world address
    const CLIENT: usize = 0x2000; // bound, not a game offset: a test's own world address
    const HOSTED_THREAD: u32 = 7;
    const CLIENT_THREAD: u32 = 8;

    /// What a client's pass of `world` on `thread` sees while `hosted` holds
    /// its state and the post-step link saw `server` (0 for never).
    fn on_client(hosted: &Hosted, thread: u32, world: usize, server: usize) -> Seen {
        Seen {
            side: Side::CLIENT,
            world,
            server,
            on_game_thread: server != 0 && thread == HOSTED_THREAD,
            on_hosted_thread: hosted.runs_on(thread),
        }
    }

    #[test]
    fn a_hosted_server_s_thread_counts_from_its_start_until_it_left() {
        let hosted = Hosted::new();
        assert!(!hosted.runs_on(HOSTED_THREAD));
        assert!(!hosted.runs_on(0));
        hosted.starts(HOSTED_THREAD);
        assert!(hosted.runs_on(HOSTED_THREAD));
        assert!(!hosted.runs_on(CLIENT_THREAD));
        hosted.left(HOSTED_THREAD);
        assert!(!hosted.runs_on(HOSTED_THREAD));
    }

    #[test]
    fn an_old_server_leaving_late_keeps_the_newer_server_s_thread() {
        let hosted = Hosted::new();
        hosted.starts(CLIENT_THREAD);
        hosted.starts(HOSTED_THREAD);
        hosted.left(CLIENT_THREAD);
        assert!(hosted.runs_on(HOSTED_THREAD));
        assert_eq!(tell(on_client(&hosted, HOSTED_THREAD, SERVER, 0)), None);
    }

    #[test]
    fn a_hosted_server_seen_stepping_on_another_thread_is_told_by_that_thread() {
        let hosted = Hosted::new();
        assert_eq!(hosted.stepped_on(HOSTED_THREAD), None, "no start recorded");
        assert!(!hosted.runs_on(HOSTED_THREAD));
        hosted.starts(CLIENT_THREAD);
        assert_eq!(hosted.stepped_on(HOSTED_THREAD), Some(CLIENT_THREAD));
        assert!(hosted.runs_on(HOSTED_THREAD));
        assert_eq!(
            tell(on_client(&hosted, CLIENT_THREAD, CLIENT, 0)),
            Some(Whose::Client)
        );
        assert_eq!(hosted.stepped_on(HOSTED_THREAD), None, "already there");
    }

    #[test]
    fn a_hosted_server_s_world_unseen_by_the_post_step_link_is_nobody_s_and_the_client_s_its_own() {
        let hosted = Hosted::new();
        hosted.starts(HOSTED_THREAD);
        assert_eq!(tell(on_client(&hosted, HOSTED_THREAD, SERVER, 0)), None);
        assert_eq!(
            tell(on_client(&hosted, CLIENT_THREAD, CLIENT, 0)),
            Some(Whose::Client)
        );
    }

    #[test]
    fn with_the_post_step_link_never_seeing_the_server_the_client_s_world_stays_its_own() {
        let hosted = Hosted::new();
        hosted.starts(HOSTED_THREAD);
        for _step in 0..3 {
            assert_eq!(tell(on_client(&hosted, HOSTED_THREAD, SERVER, 0)), None);
            assert_eq!(
                tell(on_client(&hosted, CLIENT_THREAD, CLIENT, 0)),
                Some(Whose::Client)
            );
        }
    }

    #[test]
    fn an_abandoned_start_leaves_the_client_s_world_its_own_once_the_server_left() {
        let hosted = Hosted::new();
        hosted.starts(HOSTED_THREAD);
        hosted.left(HOSTED_THREAD);
        assert_eq!(
            tell(on_client(&hosted, CLIENT_THREAD, CLIENT, 0)),
            Some(Whose::Client)
        );
        hosted.starts(HOSTED_THREAD);
        assert_eq!(tell(on_client(&hosted, HOSTED_THREAD, SERVER, 0)), None);
    }

    #[test]
    fn another_world_the_hosted_server_s_thread_moves_is_nobody_s() {
        let hosted = Hosted::new();
        hosted.starts(HOSTED_THREAD);
        assert_eq!(
            tell(on_client(&hosted, HOSTED_THREAD, CLIENT, SERVER)),
            None
        );
    }

    #[test]
    fn each_world_is_told_once_the_hosted_server_s_is_seen() {
        let hosted = Hosted::new();
        hosted.starts(HOSTED_THREAD);
        assert_eq!(
            tell(on_client(&hosted, HOSTED_THREAD, SERVER, SERVER)),
            Some(Whose::Server)
        );
        assert_eq!(
            tell(on_client(&hosted, CLIENT_THREAD, CLIENT, SERVER)),
            Some(Whose::Client)
        );
        assert_eq!(
            tell(on_client(&hosted, CLIENT_THREAD, SERVER, SERVER)),
            None
        );
    }

    #[test]
    fn a_client_with_no_hosted_server_has_only_its_own_world() {
        let hosted = Hosted::new();
        assert_eq!(
            tell(on_client(&hosted, CLIENT_THREAD, CLIENT, 0)),
            Some(Whose::Client)
        );
        assert_eq!(tell(on_client(&hosted, CLIENT_THREAD, 0, 0)), None);
    }

    #[test]
    fn a_dedicated_server_has_only_its_own_world_on_its_game_thread() {
        let on_server = |world, server, on_game_thread| Seen {
            side: Side::SERVER,
            world,
            server,
            on_game_thread,
            on_hosted_thread: false,
        };
        assert_eq!(tell(on_server(SERVER, SERVER, true)), Some(Whose::Server));
        assert_eq!(tell(on_server(SERVER, 0, true)), None);
        assert_eq!(tell(on_server(CLIENT, SERVER, true)), None);
        assert_eq!(tell(on_server(SERVER, SERVER, false)), None);
    }
}
