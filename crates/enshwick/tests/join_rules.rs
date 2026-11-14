//! Join rules: a mod's say over an account joining, before its player
//! enters the world.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off, PlayerKey};

    const ADA: PlayerKey = PlayerKey::new(0x5c86_aa43_5230_614e);
    const BOB: PlayerKey = PlayerKey::new(0x0110_0001_0000_0042);
    const PANICS: PlayerKey = PlayerKey::new(0xdead);

    struct AllowList;

    impl Mod for AllowList {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_join(|join| {
                assert_ne!(join.player, Some(PANICS), "a rule that panics");
                join.player == Some(ADA)
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        AllowList,
        r#"
        id = "you.allow-list"
        side = "server"
        "#
    );

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(AllowList)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        loader
    }

    #[test]
    fn an_account_on_the_list_joins_and_any_other_is_refused() {
        let loader = started();
        assert!(loader.join(Some(ADA)));
        assert!(!loader.join(Some(BOB)));
        assert!(!loader.join(None), "an account the game gives no key");
    }

    #[test]
    fn a_rule_that_panics_lets_the_account_in_and_faults_its_mod() {
        let loader = started();
        assert!(loader.join(Some(PANICS)));
        assert!(
            loader
                .faults()
                .iter()
                .any(|f| f.contains("panic in a join rule")),
            "{:?}",
            loader.faults()
        );
    }
}
