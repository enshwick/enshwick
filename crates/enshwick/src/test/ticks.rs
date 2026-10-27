//! The fake loader's steps: every-step and periodic tick handlers, delivered
//! as the loader delivers them.

use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{Event, EventFn, ModHandle, Status, TickData, TickHandle};

use super::Installed;
use super::table::{put, with};

/// A periodic tick handler the mod subscribed.
struct FakeTick {
    owner: ModHandle,
    every: u64,
    callback: EventFn,
    user: usize,
    /// The step and time of its last call; `None` before the first.
    last: Option<(u64, u64)>,
}

/// The fake's server steps and the mod's periodic tick handlers.
#[derive(Default)]
pub(crate) struct FakeTicks {
    /// The last step [`Installed::tick`] ran, 0 before the first.
    step: u64,
    /// The time of that step, in nanoseconds from the first.
    nanos: u64,
    ticks: Vec<FakeTick>,
}

pub(crate) extern "system" fn subscribe_tick(
    me: ModHandle,
    every_nanos: u64,
    callback: EventFn,
    user: *mut c_void,
    out: *mut TickHandle,
) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.base_limit.armed {
            return Status::TOO_LATE;
        }
        let Ok(handle) = u32::try_from(s.ticks.ticks.len()) else {
            return Status::REFUSED;
        };
        s.ticks.ticks.push(FakeTick {
            owner: me,
            every: every_nanos,
            callback,
            user: user.expose_provenance(),
            last: None,
        });
        put(out, TickHandle(handle))
    })
}

pub(crate) extern "system" fn set_tick_every(
    me: ModHandle,
    handle: TickHandle,
    every_nanos: u64,
) -> Status {
    with(Status::MISSING, |s| {
        let Some(tick) = s.ticks.ticks.get_mut(handle.0 as usize) else {
            return Status::MISSING;
        };
        if tick.owner != me {
            return Status::REFUSED;
        }
        tick.every = every_nanos;
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
    /// does: every handler of [`Load::on_every_step`](crate::Load::on_every_step)
    /// runs, and every periodic handler whose period has passed since its
    /// last call, with the steps and time since then. Returns how many
    /// handlers ran.
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
        let calls: Vec<(EventFn, usize, TickData)> = with(Vec::new(), |s| {
            let ticks = &mut s.ticks;
            ticks.step += 1;
            ticks.nanos = ticks.nanos.saturating_add(elapsed);
            let (step, now) = (ticks.step, ticks.nanos);
            let tick = TickData {
                elapsed_nanos: elapsed,
                step_nanos: elapsed,
                ..TickData::new(step)
            };
            let every_step = s
                .subscriptions
                .iter()
                .filter(|sub| sub.0 == Event::TICKED)
                .map(|sub| (sub.1, sub.2, tick));
            let periodic = ticks
                .ticks
                .iter_mut()
                .filter(|t| due(now, t.last, t.every))
                .map(|t| {
                    let (steps, nanos) = t
                        .last
                        .map_or((1, elapsed), |(at_step, at)| (step - at_step, now - at));
                    t.last = Some((step, now));
                    let data = TickData {
                        elapsed_steps: steps,
                        elapsed_nanos: nanos,
                        ..tick
                    };
                    (t.callback, t.user, data)
                });
            every_step.chain(periodic).collect()
        });
        for (callback, user, data) in &calls {
            // SAFETY: `data` lives for the call and `user` is the pointer the
            // mod subscribed `callback` with.
            unsafe {
                callback(
                    Event::TICKED,
                    (&raw const *data).cast(),
                    core::ptr::with_exposed_provenance_mut(*user),
                );
            };
        }
        calls.len()
    }
}

impl FakeTicks {
    /// The mod's periodic tick handlers and their pointers, for a raw raise
    /// of [`Event::TICKED`], which reaches every tick handler.
    pub(crate) fn handlers(&self) -> impl Iterator<Item = (EventFn, usize)> + '_ {
        self.ticks.iter().map(|t| (t.callback, t.user))
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
