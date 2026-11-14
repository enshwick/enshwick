//! Whose world an engine pass hands the loader: the running server's, or on
//! a client its own. A client runs both while it hosts a game, each on its
//! own thread, and learns the server's world only after the server's first
//! step, while the engine already moves that world's events as it loads it
//! (`ecs.event_merge` has callers besides `world.step`, client 26430625).
//! So from a hosted server's start until its world is seen, no world counts
//! as the client's own; from then until its teardown, which follows the
//! final save after the hosted game ends, no world its thread moves does.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use enshwick_core::abi::Side;

/// The hosted server a client runs, as far as its engine passes need it.
#[derive(Debug)]
pub(super) struct Hosted {
    /// A hosted server started on this client and its world is not seen yet.
    starting: AtomicBool,
    /// The hosted server's game thread, from its world's first sight until
    /// its teardown; 0 for none.
    thread: AtomicU32,
}

impl Hosted {
    pub(super) const fn new() -> Self {
        Self {
            starting: AtomicBool::new(false),
            thread: AtomicU32::new(0),
        }
    }

    /// The server's world is seen, stepped on `thread`.
    pub(super) fn server_seen(&self, thread: u32) {
        self.thread.store(thread, Ordering::Release);
        self.starting.store(false, Ordering::Release);
    }

    /// The server is torn down.
    pub(super) fn server_gone(&self) {
        self.thread.store(0, Ordering::Release);
        self.starting.store(false, Ordering::Release);
    }

    /// A hosted server starts; its world is not seen yet.
    fn starts(&self) {
        self.starting.store(true, Ordering::Release);
    }

    /// Whether a hosted server started and its world is not seen yet.
    fn starting(&self) -> bool {
        self.starting.load(Ordering::Acquire)
    }

    /// Whether `thread` steps a server seen and not yet torn down.
    fn steps_on(&self, thread: u32) -> bool {
        self.thread.load(Ordering::Acquire) == thread
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

/// A hosted server starts on this client; its world is not seen yet.
pub(crate) fn server_starts() {
    super::STATE.hosted.starts();
}

/// Whose `entity_world` is, passed on this thread to the loader of `side`;
/// `None` for a world the loader cannot tell.
pub(crate) fn whose(side: Side, entity_world: usize) -> Option<Whose> {
    // SAFETY: `GetCurrentThreadId` has no preconditions.
    let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    let state = &super::STATE;
    tell(Seen {
        side,
        world: entity_world,
        server: usize::try_from(super::server_world()).unwrap_or(0),
        on_game_thread: super::hosting() && me == state.game_thread.load(Ordering::Acquire),
        on_hosted_thread: state.hosted.steps_on(me),
        starting: state.hosted.starting(),
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
    /// On the thread of a server seen and not yet torn down, live or
    /// stopping.
    on_hosted_thread: bool,
    starting: bool,
}

fn tell(seen: Seen) -> Option<Whose> {
    if seen.world == 0 {
        None
    } else if seen.server != 0 && seen.world == seen.server {
        seen.on_game_thread.then_some(Whose::Server)
    } else if seen.side == Side::CLIENT && !seen.starting && !seen.on_hosted_thread {
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

    #[test]
    fn a_hosted_server_is_starting_until_seen_and_its_thread_counts_until_gone() {
        let hosted = Hosted::new();
        hosted.starts();
        assert!(hosted.starting());
        hosted.server_seen(7);
        assert!(!hosted.starting());
        assert!(hosted.steps_on(7));
        assert!(!hosted.steps_on(8));
        hosted.starts();
        hosted.server_gone();
        assert!(!hosted.starting());
        assert!(!hosted.steps_on(7));
    }

    fn seen(side: Side, world: usize, server: usize, starting: bool) -> Seen {
        Seen {
            side,
            world,
            server,
            on_game_thread: world == server,
            on_hosted_thread: world == server,
            starting,
        }
    }

    #[test]
    fn a_hosted_server_s_world_after_its_game_ended_is_nobody_s_until_its_teardown() {
        let final_save = Seen {
            on_hosted_thread: true,
            ..seen(Side::CLIENT, SERVER, 0, false)
        };
        assert_eq!(tell(final_save), None);
        assert_eq!(
            tell(seen(Side::CLIENT, CLIENT, 0, false)),
            Some(Whose::Client)
        );
    }

    #[test]
    fn another_world_the_hosted_server_s_thread_moves_is_nobody_s() {
        let other = Seen {
            on_game_thread: true,
            on_hosted_thread: true,
            ..seen(Side::CLIENT, CLIENT, SERVER, false)
        };
        assert_eq!(tell(other), None);
    }

    #[test]
    fn a_hosted_server_s_world_before_its_first_step_is_nobody_s() {
        assert_eq!(tell(seen(Side::CLIENT, SERVER, 0, true)), None);
        assert_eq!(tell(seen(Side::CLIENT, CLIENT, 0, true)), None);
    }

    #[test]
    fn each_world_is_told_once_the_hosted_server_s_is_seen() {
        assert_eq!(
            tell(seen(Side::CLIENT, SERVER, SERVER, false)),
            Some(Whose::Server)
        );
        assert_eq!(
            tell(seen(Side::CLIENT, CLIENT, SERVER, false)),
            Some(Whose::Client)
        );
        assert_eq!(
            tell(seen(Side::CLIENT, CLIENT, 0, false)),
            Some(Whose::Client)
        );
    }

    #[test]
    fn a_dedicated_server_has_only_its_own_world_on_its_game_thread() {
        assert_eq!(
            tell(seen(Side::SERVER, SERVER, SERVER, false)),
            Some(Whose::Server)
        );
        assert_eq!(tell(seen(Side::SERVER, SERVER, 0, false)), None);
        assert_eq!(tell(seen(Side::SERVER, CLIENT, SERVER, false)), None);
        let off_thread = Seen {
            on_game_thread: false,
            ..seen(Side::SERVER, SERVER, SERVER, false)
        };
        assert_eq!(tell(off_thread), None);
        assert_eq!(tell(seen(Side::CLIENT, 0, 0, false)), None);
    }
}
