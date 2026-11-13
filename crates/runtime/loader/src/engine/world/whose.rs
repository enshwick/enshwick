//! Whose world an engine pass hands the loader: the running server's, or on
//! a client its own. A client runs both while it hosts a game, each on its
//! own thread, and learns the server's world only after the server's first
//! step, while the engine already moves that world's events as it loads it
//! (`ecs.event_merge` has callers besides `world.step`, client 26430625).
//! So from a hosted server's start until its world is seen, no world counts
//! as the client's own.

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::Side;

/// A hosted server started on this client and its world is not seen yet.
static STARTING: AtomicBool = AtomicBool::new(false);

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
    STARTING.store(true, Ordering::Release);
}

/// The hosted server's world is seen, or the server is gone.
pub(super) fn server_settled() {
    STARTING.store(false, Ordering::Release);
}

/// Whose `entity_world` is, passed on this thread to the loader of `side`;
/// `None` for a world the loader cannot tell.
pub(crate) fn whose(side: Side, entity_world: usize) -> Option<Whose> {
    // SAFETY: `GetCurrentThreadId` has no preconditions.
    let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    tell(Seen {
        side,
        world: entity_world,
        server: usize::try_from(super::server_world()).unwrap_or(0),
        on_game_thread: super::hosting() && me == super::GAME_THREAD.load(Ordering::Acquire),
        starting: STARTING.load(Ordering::Acquire),
    })
}

/// What the loader knows when an engine pass hands it a world.
#[derive(Debug, Clone, Copy)]
struct Seen {
    side: Side,
    world: usize,
    /// The running server's entity world, 0 for none seen.
    server: usize,
    on_game_thread: bool,
    starting: bool,
}

fn tell(seen: Seen) -> Option<Whose> {
    if seen.world == 0 {
        None
    } else if seen.server != 0 && seen.world == seen.server {
        seen.on_game_thread.then_some(Whose::Server)
    } else if seen.side == Side::CLIENT && !seen.starting {
        Some(Whose::Client)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: usize = 0x1000;
    const CLIENT: usize = 0x2000;

    fn seen(side: Side, world: usize, server: usize, starting: bool) -> Seen {
        Seen {
            side,
            world,
            server,
            on_game_thread: world == server,
            starting,
        }
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
