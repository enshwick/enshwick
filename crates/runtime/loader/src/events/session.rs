//! The server session the post-step link follows: which server steps on
//! which thread, and which hosted game that is.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use enshwick_base::Relock;

/// The server session the post-step link follows: the server and its thread.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Session {
    server: usize,
    thread: u32,
    /// Set when the session stopped; its last steps no longer count.
    ended: bool,
    /// The hosted games that ended, which a new session keeps.
    games_ended: u64,
}

/// What a step means for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The session seen so far goes on.
    Same,
    /// Another server or thread: a new session.
    New,
    /// A step of a session that already stopped.
    Ended,
}

impl Session {
    const fn new() -> Self {
        Self {
            server: 0,
            thread: 0,
            ended: false,
            games_ended: 0,
        }
    }

    /// Follows a step of `server` on `thread`.
    pub(crate) fn step(&mut self, server: usize, thread: u32) -> Step {
        if (server, thread) == (self.server, self.thread) {
            return if self.ended { Step::Ended } else { Step::Same };
        }
        *self = Self {
            server,
            thread,
            ended: false,
            games_ended: self.games_ended,
        };
        Step::New
    }

    /// Marks the session stopped: the hosted game ended.
    pub(crate) fn stop(&mut self) {
        if !self.ended {
            self.games_ended += 1;
        }
        self.ended = true;
    }

    /// A hosted game starts: its first step is a new session even on the
    /// server and thread of the one that stopped.
    pub(crate) fn begin(&mut self) {
        *self = Self {
            games_ended: self.games_ended,
            ..Self::default()
        };
    }

    /// The game the server steps, from 1, as
    /// [`TickData::game`](enshwick_core::abi::TickData::game) counts it.
    pub(crate) const fn game(&self) -> u64 {
        self.games_ended + 1
    }

    /// Follows a step as [`Session::step`] does; with the game it is in.
    pub(crate) fn step_in_game(&mut self, server: usize, thread: u32) -> (Step, u64) {
        (self.step(server, thread), self.game())
    }
}

/// The session the post-step link follows, with the copy each step reads
/// first: a step of the running session seen last takes no lock.
pub(super) struct Followed {
    session: Mutex<Session>,
    /// The running session's server, 0 while none runs.
    server: AtomicUsize,
    thread: AtomicU32,
    game: AtomicU64,
}

impl Followed {
    const fn new() -> Self {
        Self {
            session: Mutex::new(Session::new()),
            server: AtomicUsize::new(0),
            thread: AtomicU32::new(0),
            game: AtomicU64::new(Session::new().game()),
        }
    }

    /// What a step of `server` on `thread` means, with its game.
    #[inline]
    pub(super) fn step(&self, server: usize, thread: u32) -> (Step, u64) {
        if server != 0
            && self.server.load(Ordering::Acquire) == server
            && self.thread.load(Ordering::Acquire) == thread
        {
            return (Step::Same, self.game.load(Ordering::Acquire));
        }
        self.change(|session| session.step_in_game(server, thread))
    }

    /// Runs `change` on the session under its lock and copies out what a
    /// step reads first.
    #[cold]
    pub(super) fn change<R>(&self, change: impl FnOnce(&mut Session) -> R) -> R {
        let mut session = self.session.relock();
        let changed = change(&mut session);
        let running = if session.ended { 0 } else { session.server };
        self.server.store(running, Ordering::Release);
        self.thread.store(session.thread, Ordering::Release);
        self.game.store(session.game(), Ordering::Release);
        changed
    }
}

/// The session of this process's server.
pub(super) static SESSION: Followed = Followed::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_follows_its_server_and_thread() {
        let mut session = Session::default();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.step(0x1000, 7), Step::Same);
        session.stop();
        assert_eq!(session.step(0x1000, 7), Step::Ended);
        session.begin();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.step(0x1000, 7), Step::Same);
        // The next hosted game: a new server on a new thread.
        assert_eq!(session.step(0x2000, 9), Step::New);
        assert_eq!(session.step(0x2000, 9), Step::Same);
        // The same address again, on another thread, is another game.
        session.stop();
        assert_eq!(session.step(0x2000, 11), Step::New);
    }

    #[test]
    fn each_hosted_game_that_ends_makes_the_next_steps_another_game() {
        let mut session = Session::default();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.game(), 1);
        session.begin();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.game(), 1, "the game's own start is the same game");
        session.stop();
        session.stop();
        assert_eq!(session.game(), 2, "one game ended, once");
        session.begin();
        assert_eq!(session.step(0x2000, 9), Step::New);
        assert_eq!(session.game(), 2);
    }

    #[test]
    fn a_followed_session_answers_as_its_locked_one_does_through_every_change() {
        let followed = Followed::new();
        let mut session = Session::default();
        let steps: [(usize, u32); 6] = [
            (0x1000, 7),
            (0x1000, 7),
            (0, 7),
            (0x2000, 7),
            (0x2000, 9),
            (0x2000, 9),
        ];
        for (at, &(server, thread)) in steps.iter().enumerate() {
            if at == 3 {
                followed.change(Session::stop);
                session.stop();
                assert_eq!(
                    followed.step(0x1000, 7),
                    session.step_in_game(0x1000, 7),
                    "a stopped session's steps take the lock and end"
                );
                followed.change(Session::begin);
                session.begin();
            }
            assert_eq!(
                followed.step(server, thread),
                session.step_in_game(server, thread),
                "step {at}"
            );
        }
        assert_eq!(followed.game.load(Ordering::Acquire), 2);
    }
}
