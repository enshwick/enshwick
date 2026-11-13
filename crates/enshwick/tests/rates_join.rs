//! A mod or pack of side server cannot declare a rate clients count too; a
//! pack of side both may.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::rates::Rate;
    use enshwick::test::FakeHost;

    struct Alone;

    impl enshwick::Mod for Alone {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let roofs = Rate::RecipeOutput(enshwick::content::recipes::RecipeKind::Roof);
            if load.rate(roofs).is_ok() {
                return Err(enshwick::Off::new("a recipe output rate took without join"));
            }
            load.rate(Rate::FactoryProductionSpeed)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Alone,
        r#"
        id = "you.alone"
        side = "server"
        "#
    );

    #[test]
    fn a_rate_clients_count_needs_a_mod_that_joins() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Alone)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::REFUSED);
        assert_eq!(loader.rate(Rate::FactoryProductionSpeed), None);
    }

    #[test]
    fn a_pack_of_side_both_declares_a_rate_clients_count_and_one_of_side_server_does_not() {
        use enshwick::abi::{ModHandle, Side};

        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process; the loader
        // makes its own handle the host of the copy that runs the packs.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        let declare = |me, side| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick::__private::init_pack(loader.api(), ModHandle(me), side, |load| {
                    load.rate(Rate::SkillPoints).map(drop)
                })
            }
        };
        assert_eq!(declare(1, Side::SERVER), Status::REFUSED);
        assert_eq!(declare(2, Side::BOTH), Status::OK, "{:?}", loader.logs());
    }
}
