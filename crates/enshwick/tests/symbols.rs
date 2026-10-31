//! A pinned symbol that names no address turns the mod off.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct Pinned;

    impl Mod for Pinned {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            assert_eq!(load.symbol("game.present")?, 0x1_4000_1000);
            let null = load.symbol("game.null").unwrap_err();
            assert_eq!(
                null.to_string(),
                "symbol game.null: null on this game build"
            );
            Err(null)
        }
    }

    enshwick::test::declare_mod!(
        Pinned,
        r#"
        id = "you.pinned"
        side = "server"
        "#
    );

    #[test]
    fn a_null_symbol_is_refused() {
        let loader = enshwick::test::FakeHost::new()
            .symbol("game.present", 0x1_4000_1000)
            .symbol("game.null", 0)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Pinned)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::REFUSED);
    }
}
