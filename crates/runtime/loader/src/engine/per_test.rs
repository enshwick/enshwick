//! What one of the loader's own tests turned off, kept apart from every
//! other test in the process: the fuses it blew or checked, the parts it
//! listed off, the mods it turned off and the log lines it captures. libtest runs every test on a thread
//! of its own, even with one test thread, so a test's scope is its thread's,
//! and work it hands another
//! thread through [`carried`] joins it. Outside the loader's tests the
//! loader keeps one process-wide state and [`carried`] is the work itself.

#[cfg(test)]
pub(crate) use scope::{with, with_lines};

/// `work`, to run on another thread as part of the calling test: in the
/// loader's tests that thread sees and turns off what the test does.
#[cfg(not(test))]
#[inline]
pub(crate) fn carried<R>(work: impl FnOnce() -> R + Send) -> impl FnOnce() -> R + Send {
    work
}

/// `work`, to run on another thread as part of the calling test: that
/// thread sees and turns off what the test does.
#[cfg(test)]
pub(crate) fn carried<R>(work: impl FnOnce() -> R + Send) -> impl FnOnce() -> R + Send {
    let scope = scope::current();
    move || {
        scope::adopt(scope);
        work()
    }
}

#[cfg(test)]
mod scope {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex, PoisonError};

    use enshwick_base::Relock;
    use enshwick_core::abi::Level;
    use rustc_hash::FxHashSet;

    use crate::engine::diag::{PartOff, TurnedOff};

    /// A captured log line: level, source and message.
    pub(crate) type CapturedLine = (Level, String, String);

    /// What one test turned off or checked.
    #[derive(Debug, Default)]
    pub(crate) struct Turned {
        /// The identities of the fuse switches it set: the fuses it blew and
        /// the fuses whose live check held.
        pub(crate) switches: FxHashSet<u64>,
        /// The parts it listed off, in order.
        pub(crate) parts: Vec<PartOff>,
        /// The mods it turned off, oldest first.
        pub(crate) mods: Vec<TurnedOff>,
    }

    /// One test's state.
    #[derive(Debug, Default)]
    pub(super) struct Scoped {
        turned: Mutex<Turned>,
        /// The log lines it captures, while it captures. A lock of its own,
        /// off the poison policy: recovering a lock logs, and a logged line
        /// comes here.
        lines: Mutex<Option<Vec<CapturedLine>>>,
    }

    type Scope = Arc<Scoped>;

    std::thread_local! {
        /// The scope of the test this thread runs for, made at first use.
        static CURRENT: RefCell<Option<Scope>> = const { RefCell::new(None) };
    }

    /// The calling thread's scope.
    pub(super) fn current() -> Scope {
        CURRENT.with_borrow_mut(|current| Arc::clone(current.get_or_insert_default()))
    }

    /// Makes `scope` the calling thread's.
    pub(super) fn adopt(scope: Scope) {
        CURRENT.set(Some(scope));
    }

    /// Runs `work` on what the calling thread's test turned off.
    pub(crate) fn with<R>(work: impl FnOnce(&mut Turned) -> R) -> R {
        work(&mut current().turned.relock())
    }

    /// Runs `work` on the log lines the calling thread's test captures,
    /// `None` while it does not.
    pub(crate) fn with_lines<R>(work: impl FnOnce(&mut Option<Vec<CapturedLine>>) -> R) -> R {
        // Off the poison policy, as `Scoped::lines` says.
        // ast-grep-ignore: lock-through-relock
        work(
            &mut current()
                .lines
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fuse::Fuse;

    #[test]
    fn work_carried_to_another_thread_turns_off_what_its_test_sees() {
        static LAMP: Fuse = Fuse::new("carried lamp");
        let blown = std::thread::spawn(carried(|| LAMP.blow("a carried read"))).join();
        assert!(blown.is_ok());
        assert!(LAMP.is_off());
        let listed = crate::engine::diag::parts_off();
        assert!(
            listed.iter().any(|p| p.part == "carried lamp"),
            "{listed:?}"
        );
    }

    #[test]
    fn a_thread_started_without_carrying_has_a_scope_of_its_own() {
        static LAMP: Fuse = Fuse::new("uncarried lamp");
        let blown = std::thread::spawn(|| {
            LAMP.blow("a thread of its own");
            LAMP.is_off()
        })
        .join();
        assert_eq!(blown.ok(), Some(true));
        assert!(!LAMP.is_off());
    }
}
