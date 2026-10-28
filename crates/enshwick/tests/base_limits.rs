//! Base limits a mod asks for: the largest request holds, and a request past
//! the most the loader takes is refused; an unknown build keeps the game's.

#[cfg(test)]
#[cfg(feature = "unstable")]
mod tests {
    use enshwick::abi::Status;
    use enshwick::altars::Grouping;
    use enshwick::{Load, Mod, Off};

    struct ManyAltars;

    impl Mod for ManyAltars {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.many-altars", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let Err(refused) = load.request_base_limits(256, 16) else {
                return Err(Off::new("256 altar zones taken"));
            };
            assert!(
                refused.to_string().contains("the most is 255 in 179"),
                "{refused}"
            );
            assert!(load.request_base_limits(255, 180).is_err());
            assert!(load.request_base_limits(128, 40)?);
            assert!(
                load.raise_base_limit()?,
                "a smaller request leaves the larger"
            );
            load.on_altar_limit(|limit| limit.allowed = 200)?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(ManyAltars);

    #[test]
    fn the_largest_request_holds_and_one_past_the_most_is_refused() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let limit = enshwick::altars::base_limit().unwrap();
        assert_eq!((limit.zones, limit.bases), (128, 40));
        assert_eq!(loader.requested_base_limits(), Some((128, 40)));
        assert_eq!((limit.vanilla_zones, limit.vanilla_bases), (16, 16));
        assert_eq!((limit.max_zones, limit.max_bases), (255, 179));
        assert_eq!(loader.altar_limit(11, 16), 128, "the zones cap the chain");
        assert!(!limit.past_vanilla(Grouping {
            zones: 16,
            bases: 16
        }));
        assert!(limit.past_vanilla(Grouping {
            zones: 17,
            bases: 4
        }));
        assert!(limit.past_vanilla(Grouping {
            zones: 16,
            bases: 17
        }));
    }
}

#[cfg(test)]
#[cfg(feature = "unstable")]
mod unknown_build {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct ManyAltars;

    impl Mod for ManyAltars {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.many-altars", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            assert!(!load.request_base_limits(128, 40)?);
            assert!(!load.raise_base_limit()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(ManyAltars);

    #[test]
    fn an_unknown_build_keeps_the_game_s_limits() {
        let loader = enshwick::test::FakeHost::new()
            .without_base_limit()
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.requested_base_limits(), None);
        let limit = enshwick::altars::base_limit().unwrap();
        assert_eq!((limit.zones, limit.bases), (16, 16));
    }
}

#[cfg(test)]
#[cfg(feature = "unstable")]
mod zones_only {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct ManyBases;

    impl Mod for ManyBases {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.many-bases", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            assert!(load.request_base_limits(64, 179)?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(ManyBases);

    #[test]
    fn a_build_without_bases_past_16_raises_the_zones_and_keeps_16_bases() {
        let loader = enshwick::test::FakeHost::new()
            .without_bases_past_16()
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.requested_base_limits(), Some((64, 179)));
        let limit = enshwick::altars::base_limit().unwrap();
        assert_eq!((limit.zones, limit.bases), (64, 16));
        assert_eq!((limit.max_zones, limit.max_bases), (255, 16));
    }
}
