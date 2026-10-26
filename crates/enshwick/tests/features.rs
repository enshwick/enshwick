//! A mod's optional features: one that turns off on this game build leaves
//! the mod and its other features running, and the loader learns which and
//! why.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct Lanterns;

    impl Mod for Lanterns {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.lanterns", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let glow = load.feature("glow", |load| load.symbol("lantern.glow"));
            assert_eq!(glow, None);
            let light = load.feature("light", |load| load.symbol("lantern.light"));
            assert_eq!(light, Some(0x1_4000_2000));
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Lanterns);

    #[test]
    fn a_feature_that_turns_off_leaves_the_mod_running_and_says_why() {
        let loader = enshwick::test::FakeHost::new()
            .symbol("lantern.light", 0x1_4000_2000)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            loader.features(),
            [
                (
                    "glow".to_owned(),
                    Some("symbol lantern.glow: missing on this game build".to_owned())
                ),
                ("light".to_owned(), None),
            ]
        );
    }
}
