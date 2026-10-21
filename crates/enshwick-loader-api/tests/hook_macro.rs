//! `#[hook]` and `detour!` on the shapes mods write: a target type by an alias, a raw
//! body name, `Ok` and `Err` taken by a glob import; and a panicking body
//! that falls through to the rest of the chain.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::test::{FakeHost, address};
    use enshwick_loader_api::{Next, hook};

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
    fn r#move(next: Next<Upgrade>, level: u32) -> u32 {
        assert_ne!(level, 13, "unlucky");
        // SAFETY: the original takes the level.
        unsafe { next.get()(level) * 2 }
    }

    #[hook]
    fn count(next: Next<Count>, n: u32) -> u32 {
        next.get()(n) + 1
    }

    enshwick_loader_api::detour! {
        static PLAIN: extern "system" fn(n: u32) -> u32 = count;
    }

    unsafe extern "system" fn upgrade(level: u32) -> u32 {
        level + 1
    }

    extern "system" fn plain(n: u32) -> u32 {
        n
    }

    struct Hooks;

    impl enshwick_loader_api::Mod for Hooks {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "hooks", side: SERVER)
        }

        fn init(
            load: &mut enshwick_loader_api::Load<'_>,
        ) -> Result<Self, enshwick_loader_api::Off> {
            load.hook(&MOVE, load.symbol("upgrade")?)?;
            load.hook(&COUNT, load.symbol("count")?)?;
            load.hook(&PLAIN, load.symbol("plain")?)?;
            Result::Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Hooks);

    #[test]
    fn hooks_by_alias_link_run_and_fall_through_on_a_panic() {
        let loader = FakeHost::new()
            .symbol("upgrade", address(upgrade as Upgrade))
            .symbol("count", address(plain as Count))
            .symbol("plain", address(plain as Count))
            .install();
        // SAFETY: the export of `declare_mod!` above.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::OK);
        assert_eq!(MOVE.name(), "MOVE");

        let moved = loader.detour(&MOVE).expect("linked");
        // SAFETY: the detour takes a level.
        assert_eq!(unsafe { moved(4) }, 10);
        let counted = loader.detour(&COUNT).expect("linked");
        assert_eq!(counted(4), 5);
        let plained = loader.detour(&PLAIN).expect("linked");
        assert_eq!(plained(4), 5);

        // SAFETY: as above.
        assert_eq!(unsafe { moved(13) }, 14);
        assert_eq!(loader.faults(), ["panic in hook MOVE"]);
    }
}
