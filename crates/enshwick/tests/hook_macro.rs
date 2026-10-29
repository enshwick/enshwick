//! `#[hook]` and `detour!` on the shapes mods write: a target type by an alias, a raw
//! body name, `Ok` and `Err` taken by a glob import; and a panicking body
//! that falls through to the rest of the chain, and a body that panics after
//! calling `next`, which keeps what the chain gave without running it again.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::test::{FakeHost, address};
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::{Next, hook};

    #[expect(dead_code, reason = "only its variant names are imported")]
    enum Verdict {
        Ok,
        Err,
    }
    #[expect(unused_imports, reason = "shadows the prelude's `Ok` and `Err`")]
    use Verdict::*;

    type Upgrade = unsafe extern "system" fn(u32) -> u32;
    type Count = extern "system" fn(u32) -> u32;

    /// Doubles the upgrade.
    #[hook(priority = 3)]
    fn r#move(next: Next<'_, Upgrade>, level: u32) -> u32 {
        assert_ne!(level, 13, "unlucky");
        // SAFETY: the chain behind `MOVE` ends in `upgrade`, which takes any level.
        unsafe { next.call(level) * 2 }
    }

    #[hook]
    fn count(next: Next<'_, Count>, n: u32) -> u32 {
        next.call(n) + 1
    }

    enshwick::detour! {
        static PLAIN: extern "system" fn(n: u32) -> u32 = count;
    }

    static TALLIED: AtomicU32 = AtomicU32::new(0);

    /// A body with the name the macros once gave their own entry point.
    #[hook]
    fn entry(next: Next<'_, Count>, n: u32) -> u32 {
        next.call(n)
    }

    enshwick::detour! {
        static ENTRY_TOO: extern "system" fn(n: u32) -> u32 = entry;
    }

    struct Unwritten;

    impl enshwick::Mod for Unwritten {
        fn manifest() -> &'static enshwick::abi::Manifest {
            panic!("no manifest")
        }

        fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            Result::Ok(Self)
        }
    }

    /// Panics after the chain ran: the original must not run again.
    #[hook]
    fn tally(next: Next<'_, Count>, n: u32) -> u32 {
        let out = next.call(n);
        assert_ne!(out, 13, "unlucky");
        out
    }

    enshwick::detour! {
        static TALLY_TOO: unsafe extern "system" fn(n: u32) -> u32 = tally_too;
    }

    fn tally_too(next: Next<'_, Upgrade>, n: u32) -> u32 {
        // SAFETY: the chain behind `TALLY_TOO` ends in `counted_once`, which
        // takes any number.
        let out = unsafe { next.call(n) };
        assert_ne!(out, 14, "unlucky");
        out
    }

    extern "system" fn tallied(n: u32) -> u32 {
        TALLIED.fetch_add(1, Ordering::Relaxed);
        n
    }

    static COUNTED_ONCE: AtomicU32 = AtomicU32::new(0);

    unsafe extern "system" fn counted_once(n: u32) -> u32 {
        COUNTED_ONCE.fetch_add(1, Ordering::Relaxed);
        n + 1
    }

    enshwick::detour! {
        /// Panics before the chain ran: the original runs once, from the entry.
        static EARLY: extern "system" fn(n: u32) -> u32 = early;
    }

    fn early(next: Next<'_, Count>, n: u32) -> u32 {
        assert_ne!(n, 13, "unlucky");
        next.call(n) + 100
    }

    static EARLY_RAN: AtomicU32 = AtomicU32::new(0);

    extern "system" fn early_ran(n: u32) -> u32 {
        EARLY_RAN.fetch_add(1, Ordering::Relaxed);
        n
    }

    unsafe extern "system" fn upgrade(level: u32) -> u32 {
        level + 1
    }

    extern "system" fn plain(n: u32) -> u32 {
        n
    }

    struct Hooks;

    impl enshwick::Mod for Hooks {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.hooks", side: SERVER)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            // SAFETY: the fake's symbol is a function of the hook's type.
            unsafe { load.hook(&MOVE, load.symbol("upgrade")?) }?;
            // SAFETY: the fake's symbol is a function of the hook's type.
            unsafe { load.hook(&COUNT, load.symbol("count")?) }?;
            // SAFETY: the fake's symbol is a function of the hook's type.
            unsafe { load.hook(&PLAIN, load.symbol("plain")?) }?;
            // SAFETY: the fake's symbol is a function of the hook's type.
            unsafe { load.hook(&TALLY, load.symbol("tallied")?) }?;
            // SAFETY: the fake's symbol is a function of the hook's type.
            unsafe { load.hook(&TALLY_TOO, load.symbol("counted_once")?) }?;
            // SAFETY: the fake's symbol is a function of the hook's type.
            unsafe { load.hook(&EARLY, load.symbol("early_ran")?) }?;
            Result::Ok(Self)
        }
    }

    enshwick::declare_mod!(Hooks);

    #[test]
    fn hooks_run_the_chain_at_most_once_when_their_body_panics() {
        let loader = FakeHost::new()
            .symbol("upgrade", address(upgrade as Upgrade))
            .symbol("count", address(plain as Count))
            .symbol("plain", address(plain as Count))
            .symbol("tallied", address(tallied as Count))
            .symbol("counted_once", address(counted_once as Upgrade))
            .symbol("early_ran", address(early_ran as Count))
            .install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Hooks)`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::OK);
        assert_eq!(MOVE.name(), "MOVE");

        let moved = loader.detour(&MOVE).expect("linked");
        // SAFETY: the chain behind `MOVE` ends in `upgrade`, which takes any level.
        assert_eq!(unsafe { moved(4) }, 10);
        let counted = loader.detour(&COUNT).expect("linked");
        assert_eq!(counted(4), 5);
        let plained = loader.detour(&PLAIN).expect("linked");
        assert_eq!(plained(4), 5);

        // SAFETY: the chain behind `MOVE` ends in `upgrade`, which takes any level.
        assert_eq!(unsafe { moved(13) }, 14);
        assert_eq!(loader.faults(), ["panic in hook MOVE"]);

        let tally = loader.detour(&TALLY).expect("linked");
        assert_eq!(tally(13), 13, "the value the chain gave");
        assert_eq!(TALLIED.load(Ordering::Relaxed), 1, "the original ran once");

        let tally_too = loader.detour(&TALLY_TOO).expect("linked");
        // SAFETY: the chain behind `TALLY_TOO` ends in `counted_once`, which
        // takes any number.
        assert_eq!(unsafe { tally_too(13) }, 14, "the value the chain gave");
        assert_eq!(
            COUNTED_ONCE.load(Ordering::Relaxed),
            1,
            "the original ran once"
        );
        assert_eq!(
            loader.faults(),
            [
                "panic in hook MOVE",
                "panic in hook TALLY",
                "panic in hook TALLY_TOO"
            ]
        );

        let early = loader.detour(&EARLY).expect("linked");
        assert_eq!(early(13), 13, "the original's own answer");
        assert_eq!(
            EARLY_RAN.load(Ordering::Relaxed),
            1,
            "the original ran once"
        );
        assert_eq!(
            loader.faults().last().map(String::as_str),
            Some("panic in hook EARLY")
        );
    }

    #[test]
    fn a_body_named_entry_compiles() {
        assert_eq!(ENTRY.name(), "ENTRY");
        assert_eq!(ENTRY_TOO.name(), "ENTRY_TOO");
    }

    #[test]
    fn a_panicking_manifest_reads_as_a_refusal() {
        assert!(enshwick::__private::query::<Unwritten>().is_null());
    }
}
