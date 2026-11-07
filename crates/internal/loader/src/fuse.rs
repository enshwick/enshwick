//! The switch of a feature the loader runs from game threads: a panic in the
//! loader's own work turns the feature off, logged once, and its detours
//! hand every later call to the game unchanged.

use std::sync::atomic::{AtomicBool, Ordering};

/// A feature's switch, read by every detour of the feature.
#[derive(Debug)]
pub struct Fuse {
    feature: &'static str,
    off: AtomicBool,
    /// Whether a live check of [`Fuse::verify`] held this session.
    checked: AtomicBool,
}

impl Fuse {
    /// A switch that is on, for the feature the log names `feature`.
    #[must_use]
    pub const fn new(feature: &'static str) -> Self {
        Self {
            feature,
            off: AtomicBool::new(false),
            checked: AtomicBool::new(false),
        }
    }

    /// Whether the feature may write game memory: runs `check`, which reads
    /// the offsets the feature found on a live object, until one holds, and
    /// turns the feature off for the session when one fails, with one
    /// warning naming the game `build` and what did not hold. A check that
    /// held costs one load after it.
    #[inline]
    pub fn verify<E: core::fmt::Display>(
        &self,
        build: &str,
        check: impl FnOnce() -> Result<(), E>,
    ) -> bool {
        if self.checked.load(Ordering::Acquire) {
            return !self.is_off();
        }
        self.verify_first(build, check)
    }

    #[cold]
    #[inline(never)]
    fn verify_first<E: core::fmt::Display>(
        &self,
        build: &str,
        check: impl FnOnce() -> Result<(), E>,
    ) -> bool {
        if self.is_off() {
            return false;
        }
        match check() {
            Ok(()) => {
                self.checked.store(true, Ordering::Release);
                crate::log!(
                    INFO,
                    "{}: its live object holds what {build} found",
                    self.feature
                );
                !self.is_off()
            }
            Err(why) => {
                if !self.off.swap(true, Ordering::Relaxed) {
                    crate::log!(WARN, "{} off on {build}: {why}", self.feature);
                    crate::diag::part_off(self.feature, &why.to_string());
                }
                false
            }
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
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
            Ok(done) => Some(done),
            Err(payload) => {
                self.panicked(site, payload.as_ref());
                None
            }
        }
    }

    /// Turns the feature off after the panic at `site` that left `payload`.
    #[cold]
    #[inline(never)]
    fn panicked(&self, site: &str, payload: &(dyn core::any::Any + Send)) {
        self.turn_off(&format!(
            "the loader panicked in {site}: {}",
            crate::early::panic_text(payload)
        ));
    }

    /// Turns the feature off after a panic at `site`, logged the first time.
    #[cold]
    pub fn blow(&self, site: &str) {
        self.turn_off(&format!("the loader panicked in {site}"));
    }

    /// Turns the feature off for `reason`, logged and kept for the status
    /// summary and the Mods screen the first time.
    #[cold]
    fn turn_off(&self, reason: &str) {
        if !self.off.swap(true, Ordering::Relaxed) {
            crate::log!(
                ERROR,
                "{} off: {reason}; the game runs it vanilla",
                self.feature
            );
            crate::diag::part_off(self.feature, reason);
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

    #[test]
    fn a_blown_fuse_reports_its_feature_off_with_the_panic_message() {
        let fuse = Fuse::new("lantern glow");
        let _caught = fuse.run("the glow pass", || -> u32 { panic!("no oil\nleft: 0") });
        let off = crate::diag::parts_off();
        let glow = off.iter().find(|p| p.part == "lantern glow");
        assert_eq!(
            glow.map(|p| p.reason.as_str()),
            Some("the loader panicked in the glow pass: no oil")
        );
    }

    #[test]
    fn a_live_check_that_holds_runs_once_and_lets_writes_through() {
        let fuse = Fuse::new("held check");
        let runs = core::cell::Cell::new(0);
        let check = || {
            runs.set(runs.get() + 1);
            Ok::<(), &str>(())
        };
        assert!(fuse.verify("build 1", check));
        assert!(fuse.verify("build 1", check));
        assert_eq!(runs.get(), 1);
        let _caught = fuse.run("a later call", || -> u32 { panic!("broken") });
        assert!(
            !fuse.verify("build 1", check),
            "a blown fuse refuses writes"
        );
    }

    #[test]
    fn a_failed_live_check_turns_the_feature_off_for_the_session_with_its_reason() {
        let fuse = Fuse::new("failed check");
        assert!(!fuse.verify("build 1", || Err("knowledge.state gives a null pointer")));
        assert!(fuse.is_off());
        assert!(!fuse.verify("build 1", || Ok::<(), &str>(())), "stays off");
        assert_eq!(fuse.run("a later call", || 1), None);
        let off = crate::diag::parts_off();
        let part = off.iter().find(|p| p.part == "failed check");
        assert_eq!(
            part.map(|p| p.reason.as_str()),
            Some("knowledge.state gives a null pointer")
        );
    }
}
