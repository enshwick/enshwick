//! A mod learns which damage causes its filter does not get on the build.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::damage::{DamageCause, causes_off};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    struct Filtering;

    impl Mod for Filtering {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            if causes_off().is_ok() {
                return Err(Off::new("causes answered before the parts linked"));
            }
            load.on_damage(|damage| damage.scale(0.5))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Filtering,
        r#"
        id = "you.filtering"
        side = "server"
        "#
    );

    #[test]
    fn a_build_without_the_shroud_sites_answers_only_the_shroud_causes_off() {
        let loader = FakeHost::new()
            .without_damage_sites(&[DamageCause::Fog, DamageCause::DeadlyFog])
            .install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Filtering)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let mut off = causes_off().unwrap();
        off.sort_by_key(|cause| cause.to_abi());
        assert_eq!(off, [DamageCause::Fog, DamageCause::DeadlyFog]);
    }

    #[test]
    fn a_filter_on_a_build_with_every_site_gets_every_cause() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Filtering)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        assert_eq!(causes_off().unwrap(), []);
    }
}
