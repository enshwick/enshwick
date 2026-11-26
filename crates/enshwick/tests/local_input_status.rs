//! Cached sampler diagnostics are distinct from held gameplay controls.
#[cfg(test)]
mod tests {
    use enshwick::abi::{LocalInputInfo as Info, Side, Status};
    use enshwick::input::Input;
    use enshwick::{Load, Mod, Off, WorldError};
    use std::sync::Mutex;
    static INPUT: Mutex<Option<Input>> = Mutex::new(None);
    struct Watcher;
    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *INPUT.lock().unwrap() = Some(load.input()?);
            Ok(Self)
        }
    }
    enshwick::test::declare_mod!(
        Watcher,
        r#"id = "you.input-diagnostics"
side = "both""#
    );

    #[test]
    fn cached_status_does_not_request_a_sample_and_zero_held_mask_is_available() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .player_input()
            .install();
        // SAFETY: these are this file's generated exports.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let input = INPUT.lock().unwrap().unwrap();
        let never = input.local_status().unwrap();
        assert!(never.requested, "opening Input started the reads");
        assert!(!never.sample_present && !never.fresh);
        assert_eq!(never.age_ms, None);
        assert_eq!(input.local_status().unwrap(), never);
        loader.hold_locally(Some(0));
        loader.set_local_input_status(enshwick::abi::headed!(Info {
            flags: Info::REQUESTED
                | Info::LAYOUT_AVAILABLE
                | Info::LINK_AVAILABLE
                | Info::READ_ENABLED
                | Info::SAMPLE_PRESENT
                | Info::FRESH,
            reserved: 0,
            age_ms: 15,
        }));
        let live = input.local_status().unwrap();
        assert!(
            live.requested
                && live.layout_available
                && live.link_available
                && live.read_enabled
                && live.sample_present
                && live.fresh
        );
        assert_eq!(live.age_ms, Some(15));
        assert!(
            input.local_held().is_none(),
            "no interface step ran since opening Input started the reads"
        );
        let _step = loader.draw_frame();
        assert_eq!(input.local_held().unwrap().bits(), 0);
        loader.set_local_input_status(enshwick::abi::headed!(Info {
            flags: Info::SAMPLE_PRESENT,
            reserved: 0,
            age_ms: 1001,
        }));
        let stale = input.local_status().unwrap();
        assert!(
            stale.sample_present
                && !stale.fresh
                && !stale.layout_available
                && !stale.link_available
                && !stale.read_enabled
        );
        assert_eq!(stale.age_ms, Some(1001));
    }

    #[test]
    fn sampler_status_is_client_only() {
        let loader = enshwick::test::FakeHost::new().player_input().install();
        // SAFETY: these are this file's generated exports.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(matches!(
            INPUT.lock().unwrap().unwrap().local_status(),
            Err(WorldError::Unavailable)
        ));
    }

    #[test]
    fn cached_status_keeps_extreme_ages_and_ignores_unknown_flags() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .player_input()
            .install();
        // SAFETY: these are this file's generated exports.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let input = INPUT.lock().unwrap().unwrap();
        // The loader decides freshness and its own tests hold the 1000 ms
        // boundary; the SDK only copies the flag, so no age here implies it.
        for age_ms in [0, u64::MAX - 1, u64::MAX] {
            loader.set_local_input_status(enshwick::abi::headed!(Info {
                flags: Info::SAMPLE_PRESENT | (1 << 31),
                reserved: 0,
                age_ms,
            }));
            let observed = input.local_status().unwrap();
            assert_eq!(observed.age_ms, (age_ms != u64::MAX).then_some(age_ms));
            assert!(observed.requested && observed.sample_present);
            assert!(!observed.layout_available && !observed.link_available);
            assert!(!observed.read_enabled && !observed.fresh);
            assert_eq!(input.local_status().unwrap(), observed);
        }
    }
}
