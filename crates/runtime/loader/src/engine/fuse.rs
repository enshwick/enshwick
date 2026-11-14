//! A feature the loader runs from game threads, and the one way it turns off.
//!
//! A panic in its own work, a failed live check or a link the build cannot
//! make turns it off once: logged with its code, listed for the status
//! summary and the Mods screen, and wired past in every chain it linked, so
//! the game runs that part vanilla.

use std::sync::atomic::Ordering;

use enshwick_core::abi::Level;

use crate::engine::errors::{self, Code};
#[cfg(windows)]
use crate::engine::registry::Registry;

/// The features the loader's links and reports name, each once, so all the
/// links of a feature turn off with it. Never sealed: a report may name a
/// feature after the hooks arm.
#[cfg(windows)]
static NAMED: Registry<&'static Fuse> = Registry::new();

/// Why a feature is off, as the log, the status summary and the Mods screen
/// say it; every way a feature turns off builds its text here.
#[derive(Debug, Clone)]
pub(crate) struct Off {
    /// The log line, before its code.
    line: String,
    /// The reason the status summary and the Mods screen list.
    entry: String,
    level: Level,
    /// The error code the line ends with; none when the cause is neither
    /// the build nor a fault, such as two mods asking for one name.
    code: Option<Code>,
}

impl Off {
    /// `feature` is off on the build named `build`, whose entry `id` failed
    /// for `reason`.
    #[cfg(windows)]
    pub(crate) fn build(feature: &str, build: &str, id: &str, reason: &str) -> Self {
        Self {
            line: crate::engine::builds::off_line(feature, build, id, reason),
            entry: format!("{id} {reason}"),
            level: Level::WARN,
            code: Some(errors::FEATURES_OFF),
        }
    }

    /// The live object of `feature` does not hold what the build named
    /// `build` found: `why`.
    #[cfg(windows)]
    fn live(feature: &str, build: &str, why: &str) -> Self {
        Self {
            line: format!("{feature} off on {build}: {why}"),
            entry: why.to_owned(),
            level: Level::WARN,
            code: Some(errors::FEATURES_OFF),
        }
    }

    /// The loader's own code of `feature` failed while the game ran.
    pub(crate) fn fault(feature: &str, reason: &str) -> Self {
        Self {
            line: format!("{feature} off: {reason}; the game runs it vanilla"),
            entry: reason.to_owned(),
            level: Level::ERROR,
            code: Some(errors::FEATURE_FAULT),
        }
    }

    /// A log `line` and a status `entry` a caller wrote for a build that
    /// lacks what the feature needs.
    #[cfg(windows)]
    pub(crate) fn reported(line: String, entry: String) -> Self {
        Self {
            line,
            entry,
            level: Level::WARN,
            code: None,
        }
    }
}

/// Whether a fuse is off: one atomic for the process.
#[cfg(not(test))]
type Switch = std::sync::atomic::AtomicBool;

/// Whether a fuse is off, kept per thread in the loader's own tests, so a
/// test that blows a static fuse leaves every other test's alone.
#[cfg(test)]
type Switch = test_switch::Switch;

/// A feature's switch, read by every detour of the feature.
#[derive(Debug)]
pub struct Fuse {
    feature: &'static str,
    off: Switch,
    /// Whether a live check of `Fuse::verify` held in this process, kept per
    /// test as `off` is in the loader's own tests.
    #[cfg_attr(
        not(windows),
        expect(dead_code, reason = "live checks read game memory on Windows only")
    )]
    checked: Switch,
}

impl Fuse {
    /// A switch that is on, for the feature the log names `feature`.
    #[must_use]
    pub const fn new(feature: &'static str) -> Self {
        Self {
            feature,
            off: Switch::new(false),
            checked: Switch::new(false),
        }
    }

    /// Whether the feature may write game memory: runs `check`, which reads
    /// the offsets the feature found on a live object, until one holds, and
    /// turns the feature off for the process when one fails, with one
    /// warning naming the game `build` and what did not hold. A null pointer
    /// on the way means the object is not made yet: the write is refused
    /// and the next call checks again. A check that held costs one load
    /// after it.
    #[cfg(windows)]
    #[inline]
    pub(crate) fn verify(
        &self,
        build: &str,
        check: impl FnOnce() -> Result<(), crate::engine::live::Broken>,
    ) -> bool {
        if self.checked.load(Ordering::Acquire) {
            return !self.is_off();
        }
        self.verify_first(build, check)
    }

    #[cfg(windows)]
    #[cold]
    #[inline(never)]
    fn verify_first(
        &self,
        build: &str,
        check: impl FnOnce() -> Result<(), crate::engine::live::Broken>,
    ) -> bool {
        if self.is_off() {
            return false;
        }
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)) {
            Ok(Ok(())) => {
                let _was = self.checked.swap(true, Ordering::Release);
                crate::log!(
                    INFO,
                    "{}: its live object holds what {build} found",
                    self.feature
                );
                !self.is_off()
            }
            // shortcut: a wrong offset that reads a null keeps the part
            // refusing without a message; name it once a build shows one.
            Ok(Err(why)) if why.is_null() => false,
            Ok(Err(why)) => {
                self.turn_off(&Off::live(self.feature, build, &why.to_string()));
                false
            }
            Err(payload) => {
                self.panicked("its live check", payload.as_ref());
                false
            }
        }
    }

    /// The status a write [`Fuse::verify`] refused answers: unavailable once
    /// the feature is off, missing while its live object is not made yet.
    #[cold]
    pub fn refused(&self) -> enshwick_core::abi::Status {
        if self.is_off() {
            enshwick_core::abi::Status::UNAVAILABLE
        } else {
            enshwick_core::abi::Status::MISSING
        }
    }

    /// The feature the log names.
    #[must_use]
    pub const fn feature(&self) -> &'static str {
        self.feature
    }

    /// Whether a panic or a failed live check turned the feature off.
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

    /// Runs `work` on each of `items` whose fuse is on, in order, as `site`,
    /// under one unwind guard for the whole walk: a panic turns off the fuse
    /// of the item it came from, and the walk goes on after that item. A
    /// guard per item costs about 1.6 ns each around a call through a
    /// pointer (`benches/step_parts.rs`).
    #[inline]
    pub fn run_each<'fuse, T>(
        items: &[T],
        site: &str,
        fuse: impl Fn(&T) -> &'fuse Fuse,
        mut work: impl FnMut(&T),
    ) {
        let mut from = 0;
        while from < items.len() {
            let running = core::cell::Cell::new(from);
            let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for (at, item) in items.iter().enumerate().skip(from) {
                    if !fuse(item).is_off() {
                        running.set(at);
                        work(item);
                    }
                }
            }));
            let Err(payload) = walked else {
                return;
            };
            let at = running.get();
            if let Some(item) = items.get(at) {
                fuse(item).panicked(site, payload.as_ref());
            }
            from = at + 1;
        }
    }

    /// Runs `work` on each of `items` whose fuse is on, in order, under one
    /// unwind guard, until one panics: that item's fuse turns off, naming
    /// the item's `site`, and the walk ends there for this call. For work
    /// whose later items build on state an item that panicked part way may
    /// have left broken, such as the game's window stack; [`Fuse::run_each`]
    /// goes on instead.
    #[inline]
    pub fn run_in_turn<'fuse, T>(
        items: &[T],
        site: impl Fn(&T) -> &str,
        fuse: impl Fn(&T) -> &'fuse Fuse,
        mut work: impl FnMut(&T),
    ) {
        let running = core::cell::Cell::new(0);
        let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for (at, item) in items.iter().enumerate() {
                if !fuse(item).is_off() {
                    running.set(at);
                    work(item);
                }
            }
        }));
        if let (Err(payload), Some(item)) = (walked, items.get(running.get())) {
            fuse(item).panicked(site(item), payload.as_ref());
        }
    }

    /// Turns the feature off after the panic at `site` that left `payload`.
    #[cold]
    #[inline(never)]
    fn panicked(&self, site: &str, payload: &(dyn core::any::Any + Send)) {
        let reason = format!(
            "the loader panicked in {site}: {}",
            crate::engine::early::panic_text(payload)
        );
        self.turn_off(&Off::fault(self.feature, &reason));
    }

    /// Turns the feature off after a panic at `site`, logged the first time.
    #[cold]
    pub fn blow(&self, site: &str) {
        let reason = format!("the loader panicked in {site}");
        self.turn_off(&Off::fault(self.feature, &reason));
    }

    /// The feature named `feature`, one for the name, made the first time
    /// a link or a report names it. A cold lookup under a lock: hot paths
    /// hold their feature.
    #[cfg(windows)]
    pub(crate) fn named(feature: &str) -> &'static Self {
        // Kept for the process, as every link that names it is.
        let leaked = || -> &'static Self {
            Box::leak(Box::new(Self::new(Box::leak(
                feature.to_owned().into_boxed_str(),
            ))))
        };
        NAMED
            .edit(|named| {
                if let Some(found) = named.iter().copied().find(|f| f.feature == feature) {
                    return found;
                }
                let new = leaked();
                named.push(new);
                new
            })
            // Nothing seals the named features; a sealed list would only
            // cost the twin's dedup.
            .unwrap_or_else(|_sealed| leaked())
    }

    /// Turns the feature off for `off`. The first time, it logs the line
    /// with its code, lists the feature for the status summary and the Mods
    /// screen, and wires every chain past the links that carry this fuse;
    /// another fuse of the same name stays as it is, since a name may be
    /// shared by parts that must keep running. Never call it with the chains
    /// held: it takes their lock.
    #[cold]
    #[inline(never)]
    pub(crate) fn turn_off(&self, off: &Off) {
        if self.off.swap(true, Ordering::AcqRel) {
            return;
        }
        let source = crate::engine::settings::LOADER_ID;
        if crate::engine::log::enabled(off.level, source) {
            match off.code {
                Some(code) => {
                    crate::engine::log::write(off.level, source, &format!("{} ({code})", off.line));
                }
                None => crate::engine::log::write(off.level, source, &off.line),
            }
        }
        crate::engine::diag::part_off(self.feature, &off.entry);
        #[cfg(windows)]
        crate::engine::loader::rewire();
    }
}

/// The fuse switch of the loader's own tests, kept in the scope of the test
/// that reads it ([`crate::engine::per_test`]): a test that blows a
/// process-wide fuse, through the code it tests, never turns that feature
/// off for another test in the process.
#[cfg(test)]
mod test_switch {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use super::Fuse;
    use crate::engine::per_test;

    impl Fuse {
        /// A fuse on the loader's own storage, one switch for the process,
        /// for a test of what a fuse does across threads; every other fuse
        /// of a test is kept per test.
        pub(crate) const fn process_wide(feature: &'static str) -> Self {
            Self {
                feature,
                off: Switch::process_wide(),
                checked: Switch::process_wide(),
            }
        }
    }

    /// The last identity handed to a switch; 0 is none yet.
    static LAST: AtomicU64 = AtomicU64::new(0);

    /// A switch named by an identity it takes when first read, which moves
    /// with the fuse, unlike its address, which a later fuse may reuse.
    /// One made by [`Switch::process_wide`] keeps the loader's own storage
    /// instead, so a test proves what a fuse does across threads in the game.
    #[derive(Debug)]
    pub(super) struct Switch {
        id: AtomicU64,
        process: Option<AtomicBool>,
    }

    impl Switch {
        pub(super) const fn new(off: bool) -> Self {
            assert!(!off, "a fuse starts on");
            Self {
                id: AtomicU64::new(0),
                process: None,
            }
        }

        /// A switch on the storage the loader runs with: one atomic for the
        /// process.
        pub(super) const fn process_wide() -> Self {
            Self {
                id: AtomicU64::new(0),
                process: Some(AtomicBool::new(false)),
            }
        }

        fn id(&self) -> u64 {
            let id = self.id.load(Ordering::Acquire);
            if id != 0 {
                return id;
            }
            let new = LAST.fetch_add(1, Ordering::Relaxed) + 1;
            match self
                .id
                .compare_exchange(0, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => new,
                Err(taken) => taken,
            }
        }

        pub(super) fn load(&self, order: Ordering) -> bool {
            if let Some(process) = &self.process {
                return process.load(order);
            }
            let id = self.id();
            per_test::with(|turned| turned.switches.contains(&id))
        }

        pub(super) fn swap(&self, off: bool, order: Ordering) -> bool {
            if let Some(process) = &self.process {
                return process.swap(off, order);
            }
            let id = self.id();
            per_test::with(|turned| {
                if off {
                    !turned.switches.insert(id)
                } else {
                    turned.switches.remove(&id)
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of the process's fuses, which the next two tests each blow: in
    /// one process, in either order, each finds it on at its start.
    static SHARED: Fuse = Fuse::new("shared lamp");

    fn blows_the_shared_fuse() {
        assert!(!SHARED.is_off(), "another test's blow reached this one");
        let _caught = SHARED.run("a test", || -> u32 { panic!("broken") });
        assert!(SHARED.is_off());
    }

    #[test]
    fn a_test_that_blows_a_static_fuse_leaves_it_on_for_the_next_test() {
        blows_the_shared_fuse();
    }

    #[test]
    fn another_test_finds_the_static_fuse_on_after_one_blew_it() {
        blows_the_shared_fuse();
    }

    #[test]
    fn a_fuse_blown_on_a_test_thread_stays_on_for_other_threads() {
        static OTHER: Fuse = Fuse::new("other thread lamp");
        OTHER.blow("a test");
        assert!(OTHER.is_off());
        let seen = std::thread::spawn(|| OTHER.is_off()).join();
        assert_eq!(seen.ok(), Some(false));
    }

    #[test]
    fn a_fuse_on_the_loader_s_storage_blown_on_one_thread_is_off_on_every_thread() {
        static LAMP: Fuse = Fuse::process_wide("process lamp");
        assert!(!LAMP.is_off());
        let blown =
            std::thread::spawn(|| LAMP.run("another thread", || -> u32 { panic!("broken") }))
                .join();
        assert_eq!(blown.ok(), Some(None));
        assert!(
            LAMP.is_off(),
            "a blow on one thread turns the feature off for all"
        );
        assert_eq!(LAMP.run("this thread", || 1), None);
        let seen = std::thread::spawn(|| LAMP.is_off()).join();
        assert_eq!(seen.ok(), Some(true));
    }

    #[test]
    fn a_moved_fuse_keeps_its_state_and_a_new_one_starts_on() {
        let fuse = Fuse::new("moved lamp");
        fuse.blow("a test");
        let moved = [fuse];
        assert!(moved[0].is_off(), "the state moves with the fuse");
        for _ in 0..2 {
            let fresh = Fuse::new("moved lamp");
            assert!(!fresh.is_off(), "a fuse made where another was starts on");
            fresh.blow("a test");
        }
    }

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
    fn a_walk_turns_off_only_the_item_that_panicked_and_goes_on_after_it() {
        let fuses = [
            Fuse::new("walk first"),
            Fuse::new("walk second"),
            Fuse::new("walk third"),
            Fuse::new("walk fourth"),
        ];
        let mut ran = Vec::new();
        let walk = |ran: &mut Vec<usize>| {
            Fuse::run_each(
                &[0, 1, 2, 3],
                "a walk",
                |&i| &fuses[i],
                |&i| {
                    assert!(i != 1 && i != 2, "broken {i}");
                    ran.push(i);
                },
            );
        };
        walk(&mut ran);
        assert_eq!(ran, [0, 3]);
        let off: Vec<_> = fuses.iter().map(Fuse::is_off).collect();
        assert_eq!(off, [false, true, true, false]);
        walk(&mut ran);
        assert_eq!(ran, [0, 3, 0, 3], "a blown item is skipped from then on");
    }

    #[test]
    fn a_walk_in_turn_stops_at_the_item_that_panicked_and_skips_it_from_then_on() {
        let fuses = [
            Fuse::new("turn first"),
            Fuse::new("turn second"),
            Fuse::new("turn third"),
        ];
        let sites = ["the first draw", "the second draw", "the third draw"];
        let mut ran = Vec::new();
        let walk = |ran: &mut Vec<usize>| {
            Fuse::run_in_turn(
                &[0, 1, 2],
                |&i| sites[i],
                |&i| &fuses[i],
                |&i| {
                    ran.push(i);
                    assert!(i != 1, "broken {i}");
                },
            );
        };
        walk(&mut ran);
        assert_eq!(ran, [0, 1], "the walk ends at the panic");
        let off: Vec<_> = fuses.iter().map(Fuse::is_off).collect();
        assert_eq!(off, [false, true, false]);
        let listed = crate::engine::diag::parts_off();
        let second = listed.iter().find(|p| p.part == "turn second");
        assert_eq!(
            second.map(|p| p.reason.as_str()),
            Some("the loader panicked in the second draw: broken 1"),
            "the fault names the site of the item that panicked"
        );
        walk(&mut ran);
        assert_eq!(ran, [0, 1, 0, 2], "the next walk passes the blown item");
    }

    #[test]
    fn a_feature_reports_off_once_and_another_fuse_of_its_name_stays_on() {
        static LAMPS: Fuse = Fuse::new("twin lamps");
        let _turn = crate::engine::log::capture_turn();
        LAMPS.turn_off(&Off::fault("twin lamps", "the first site"));
        LAMPS.blow("a later site");
        #[cfg(windows)]
        assert!(!Fuse::named("twin lamps").is_off());
        let reported = crate::engine::log::captured()
            .iter()
            .filter(|(_, _, line)| line.starts_with("twin lamps off"))
            .count();
        assert_eq!(reported, 1);
    }

    #[test]
    fn a_blown_fuse_reports_its_feature_off_with_the_panic_message() {
        let fuse = Fuse::new("lantern glow");
        let _caught = fuse.run("the glow pass", || -> u32 { panic!("no oil\nleft: 0") });
        let off = crate::engine::diag::parts_off();
        let glow = off.iter().find(|p| p.part == "lantern glow");
        assert_eq!(
            glow.map(|p| p.reason.as_str()),
            Some("the loader panicked in the glow pass: no oil")
        );
    }

    #[cfg(windows)]
    mod live_checks {
        use super::*;

        fn failed(wanted: &'static str) -> crate::engine::live::Broken {
            crate::engine::live::holds("knowledge.state", false, wanted).unwrap_err()
        }

        fn null() -> crate::engine::live::Broken {
            let zero = 0_usize;
            crate::engine::live::pointer("knowledge.object", (&raw const zero).addr(), 0)
                .unwrap_err()
        }

        #[test]
        fn a_live_check_that_holds_runs_once_and_lets_writes_through() {
            let fuse = Fuse::new("held check");
            let runs = core::cell::Cell::new(0);
            let check = || {
                runs.set(runs.get() + 1);
                Ok(())
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
            assert!(!fuse.verify("build 1", || Err(failed("a flag, 0 or 1"))));
            assert!(fuse.is_off());
            assert!(!fuse.verify("build 1", || Ok(())), "stays off");
            assert_eq!(fuse.run("a later call", || 1), None);
            let off = crate::engine::diag::parts_off();
            let part = off.iter().find(|p| p.part == "failed check");
            assert_eq!(
                part.map(|p| p.reason.as_str()),
                Some("knowledge.state does not give a flag, 0 or 1 on the live object")
            );
        }

        #[test]
        fn a_null_on_the_way_refuses_the_write_and_checks_again_next_call() {
            let fuse = Fuse::new("unmade object");
            assert!(!fuse.verify("build 1", || Err(null())));
            assert!(!fuse.is_off(), "an object not made yet is no broken layout");
            assert!(fuse.verify("build 1", || Ok(())));
        }

        #[test]
        fn a_live_check_that_panics_turns_only_its_feature_off() {
            let fuse = Fuse::new("panicking check");
            assert!(!fuse.verify("build 1", || panic!("bad read")));
            assert!(fuse.is_off());
        }

        /// A static fuse two tests verify, one with a check that holds and
        /// one with a check that fails.
        static CHECKED: Fuse = Fuse::new("checked lamp");

        #[test]
        fn a_check_that_held_in_one_test_leaves_the_next_test_s_check_to_run() {
            assert!(CHECKED.verify("build 1", || Ok(())));
        }

        #[test]
        fn a_failing_check_in_another_test_still_turns_the_static_fuse_off() {
            assert!(!CHECKED.verify("build 1", || Err(failed("a flag, 0 or 1"))));
            assert!(CHECKED.is_off());
        }
    }
}
