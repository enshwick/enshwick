//! The switch of a feature the loader runs from game threads: a panic in the
//! loader's own work turns the feature off, logged once, and its detours
//! hand every later call to the game unchanged.

use std::sync::atomic::{AtomicBool, Ordering};

/// A feature's switch, read by every detour of the feature.
#[derive(Debug)]
pub struct Fuse {
    feature: &'static str,
    off: AtomicBool,
}

impl Fuse {
    /// A switch that is on, for the feature the log names `feature`.
    #[must_use]
    pub const fn new(feature: &'static str) -> Self {
        Self {
            feature,
            off: AtomicBool::new(false),
        }
    }

    /// Whether a panic turned the feature off.
    #[inline]
    pub fn is_off(&self) -> bool {
        self.off.load(Ordering::Relaxed)
    }

    /// Runs `work`, the loader's part of `site`: `None` while the feature is
    /// off or when `work` panicked, which turns it off.
    #[inline]
    pub fn run<R>(&self, site: &str, work: impl FnOnce() -> R) -> Option<R> {
        if self.is_off() {
            return None;
        }
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).ok();
        if done.is_none() {
            self.blow(site);
        }
        done
    }

    /// Turns the feature off after a panic at `site`, logged the first time.
    #[cold]
    pub fn blow(&self, site: &str) {
        if !self.off.swap(true, Ordering::Relaxed) {
            crate::log!(
                ERROR,
                "{} off: the loader panicked in {site}; the game runs it vanilla",
                self.feature
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_turns_the_feature_off_and_later_work_never_runs() {
        let fuse = Fuse::new("test feature");
        assert_eq!(fuse.run("the first call", || 1), Some(1));
        assert!(!fuse.is_off());
        let caught = fuse.run("the second call", || -> u32 { panic!("broken") });
        assert_eq!(caught, None);
        assert!(fuse.is_off());
        let mut ran = false;
        assert_eq!(fuse.run("the third call", || ran = true), None);
        assert!(!ran);
    }
}
