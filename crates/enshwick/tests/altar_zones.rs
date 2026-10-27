//! Altar zone sizes: every mod's filter joins one chain, and a mod may ask
//! for the zones to be sized again.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::altars::ZoneSizes;
    use enshwick::{Load, Mod, Off};

    static SIZES: Mutex<Option<ZoneSizes>> = Mutex::new(None);

    struct Wider;

    impl Mod for Wider {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.wider", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let sizes = load.on_altar_zone(|zone| {
                zone.grow += 2;
                zone.height = zone.height.max(80);
            })?;
            *SIZES.lock().unwrap() = Some(sizes);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Wider);

    #[test]
    fn a_filter_changes_the_zone_so_far_and_may_ask_for_a_new_sizing() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.altars_opened(), (true, true));
        assert_eq!(loader.altar_zone(enshwick::Entity(4096), 2, 60), (2, 80));
        assert_eq!(loader.altar_zone(enshwick::Entity(4097), 3, 120), (2, 120));
        let sizes = SIZES.lock().unwrap().unwrap();
        sizes.changed().unwrap();
        assert_eq!(loader.zone_resizes(), 1);
    }
}
