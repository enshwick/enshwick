//! The fake loader's steps: every-step and periodic tick handlers, delivered
//! as the loader delivers them.

use core::ffi::c_void;
use core::time::Duration;

use enshwick_core::abi::{Event, EventFn, ModHandle, Status, TickData, TickHandle};

use crate::test::Installed;
use crate::test::table::{Registered, put, with};

/// A periodic tick handler the mod subscribed.
struct FakeTick {
    every: u64,
    handler: Registered<EventFn>,
    /// The step and time of its last call; `None` before the first.
    last: Option<(u64, u64)>,
    /// The game of its last call, 0 before the first: as the loader's, a
    /// handler runs on its first step in a game whatever its period.
    game: u64,
}

/// The fake's server steps and the mod's periodic tick handlers.
#[derive(Default)]
pub(crate) struct FakeTicks {
    /// The last step [`Installed::tick`] ran, 0 before the first.
    pub(crate) step: u64,
    /// The time of that step, in nanoseconds from the first.
    nanos: u64,
    /// The hosted games that ended: the steps run in game one more.
    games_ended: u64,
    ticks: Vec<FakeTick>,
    /// The periodic tick subscriptions asked for, refused ones included.
    asked: usize,
}

pub(crate) extern "system" fn subscribe_tick(
    me: ModHandle,
    every_ns: u64,
    callback: Option<EventFn>,
    user: *mut c_void,
    out: *mut TickHandle,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.registration == crate::test::table::Registration::Closed {
            return Status::TOO_LATE;
        }
        let asked = s.ticks.asked;
        s.ticks.asked += 1;
        if s.host.refused_tick == Some(asked) {
            return Status::REFUSED;
        }
        let Ok(handle) = u32::try_from(s.ticks.ticks.len()) else {
            return Status::REFUSED;
        };
        s.ticks.ticks.push(FakeTick {
            every: every_ns,
            handler: Registered::new(callback, user, me),
            last: None,
            game: 0,
        });
        put(out, TickHandle(handle))
    })
}

pub(crate) extern "system" fn set_tick_every(
    me: ModHandle,
    handle: TickHandle,
    every_ns: u64,
) -> Status {
    with(Status::MISSING, |s| {
        let Some(tick) = s.ticks.ticks.get_mut(handle.0 as usize) else {
            return Status::MISSING;
        };
        if tick.handler.owner() != Some(me) {
            return Status::REFUSED;
        }
        tick.every = every_ns;
        Status::OK
    })
}

/// Whether a handler last called at `last` is due at `now` on a period of
/// `every` nanoseconds; the loader's rule.
fn due(now: u64, last: Option<(u64, u64)>, every: u64) -> bool {
    last.is_none_or(|(_, at)| now >= at.saturating_add(every))
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Runs one server step `elapsed` after the one before, as the loader
    /// does: every handler of [`Load::on_every_step`](enshwick_core::Load::on_every_step)
    /// runs, and every periodic handler whose period has passed since its
    /// last call or that has not run in this game yet, with the steps and
    /// time since then. Returns how many handlers ran.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.tick(Duration::from_millis(50)), 0);
    /// ```
    #[must_use = "the count says whether the mod handles the step"]
    pub fn tick(&self, elapsed: Duration) -> usize {
        let elapsed = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        let calls: Vec<(Registered<EventFn>, TickData)> = with(Vec::new(), |s| {
            let ticks = &mut s.ticks;
            ticks.step += 1;
            ticks.nanos = ticks.nanos.saturating_add(elapsed);
            let (step, now) = (ticks.step, ticks.nanos);
            let tick = TickData {
                elapsed_ns: elapsed,
                step_ns: elapsed,
                game: ticks.games_ended + 1,
                ..TickData::new(step)
            };
            let every_step = s
                .subscriptions
                .iter()
                .filter(|sub| sub.0 == Event::TICKED)
                .map(|sub| (sub.1, tick));
            let periodic = ticks
                .ticks
                .iter_mut()
                .filter(|t| due(now, t.last, t.every) || t.game != tick.game)
                .map(|t| {
                    t.game = tick.game;
                    let (steps, nanos) = t
                        .last
                        .map_or((1, elapsed), |(at_step, at)| (step - at_step, now - at));
                    t.last = Some((step, now));
                    let data = TickData {
                        elapsed_steps: steps,
                        elapsed_ns: nanos,
                        ..tick
                    };
                    (t.handler, data)
                });
            every_step.chain(periodic).collect()
        });
        for (handler, data) in &calls {
            // SAFETY: `data` lives for the call and `user` is the pointer the
            // mod subscribed `callback` with.
            handler.run(|callback, user| unsafe {
                callback(Event::TICKED, (&raw const *data).cast(), user);
            });
        }
        calls.len()
    }

    /// Ends the game a client hosts, as the loader does when the player
    /// leaves it: the next [`tick`](Self::tick) is the first step of the next
    /// game, with [`TickData::game`] one more, and no mod holds saves.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.end_game();
    /// assert_eq!(loader.tick(Duration::from_millis(50)), 0);
    /// ```
    pub fn end_game(&self) {
        with((), |s| {
            s.ticks.games_ended += 1;
            s.save.held = false;
            s.save.shut = false;
        });
    }
}

impl FakeTicks {
    /// The mod's periodic tick handlers and their pointers, for a raw raise
    /// of [`Event::TICKED`], which reaches every tick handler.
    pub(crate) fn handlers(&self) -> impl Iterator<Item = Registered<EventFn>> + '_ {
        self.ticks.iter().map(|t| t.handler)
    }

    /// The fake server's time: of the last step, in nanoseconds from the first.
    pub(crate) const fn nanos(&self) -> u64 {
        self.nanos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_call_is_due_at_once_and_later_ones_a_period_after_the_last() {
        assert!(due(0, None, 100));
        assert!(!due(150, Some((1, 100)), 100));
        assert!(due(200, Some((1, 100)), 100));
        assert!(due(100, Some((1, 100)), 0));
        assert!(!due(u64::MAX - 1, Some((1, u64::MAX - 1)), 5));
    }
}
