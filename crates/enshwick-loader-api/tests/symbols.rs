//! A pinned symbol that names no address turns the mod off.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::{Load, Mod, Off};

    struct Pinned;

    impl Mod for Pinned {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "pinned", side: SERVER)
        }

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

    enshwick_loader_api::declare_mod!(Pinned);

    #[test]
    fn a_null_symbol_is_refused() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .symbol("game.present", 0x1_4000_1000)
            .symbol("game.null", 0)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::REFUSED);
    }
}
