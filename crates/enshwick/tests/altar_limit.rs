//! The altar limit: every mod's filter joins one chain over the game's own
//! count, and the loader keeps the answer within the zones the game groups.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct MoreAltars;

    impl Mod for MoreAltars {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.more-altars", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_altar_limit(|limit| limit.allowed = limit.vanilla.saturating_mul(3))?;
            load.raise_base_limit()?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(MoreAltars);

    #[test]
    fn a_filter_answers_from_the_game_s_count_within_the_raised_limit() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.altar_limit(3, 6), 18);
        assert_eq!(loader.altar_limit(11, 16), 32);
        let limit = enshwick::altars::base_limit().unwrap();
        assert_eq!(
            (limit.zones, limit.bases, limit.vanilla_zones),
            (32, 16, 16)
        );
    }
}
