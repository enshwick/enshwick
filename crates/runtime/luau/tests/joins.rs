//! `enshwick.world.on_join`: a script's join rule refuses an account as a
//! Rust mod's does.

#[cfg(test)]
mod tests {
    use enshwick::PlayerKey;
    use enshwick::test::FakeHost;

    use crate::harness::run;

    #[test]
    fn a_script_s_join_rule_lets_in_only_the_accounts_it_names() {
        let loader = run(
            FakeHost::new().world(),
            r#"
            enshwick.world.on_join(function(join)
                return join.player == "7"
            end)
            "#,
        );
        assert!(loader.join(Some(PlayerKey::new(7))));
        assert!(!loader.join(Some(PlayerKey::new(8))));
        assert!(!loader.join(None), "nil for an account without a key");
    }
}
