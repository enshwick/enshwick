//! A mod sets its rates at init and carries on without a rate its build
//! lacks.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::recipes::RecipeKind;
    use enshwick::rates::{ExperienceGroup, Rate, RateError};
    use enshwick::test::FakeHost;

    const COMBAT: Rate = Rate::Experience(ExperienceGroup::Combat);
    const ROOFS: Rate = Rate::RecipeOutput(RecipeKind::Roof);

    struct Rates;

    impl enshwick::Mod for Rates {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let combat = load.rate(COMBAT)?;
            if combat.set(250.0) != Err(RateError::OutOfRange(250.0)) {
                return Err(enshwick::Off::new("a factor past 100 took"));
            }
            combat.set(3.0)?;
            load.rate(Rate::FactoryProductionSpeed)?.set(2.0)?;
            if let Ok(drops) = load.rate(Rate::ResourceDropStackAmount) {
                drops.set(5.0)?;
            }
            load.rate(ROOFS)?.set(1.5)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Rates,
        r#"
        id = "you.rates"
        side = "both"
        join = true
        "#
    );

    #[test]
    fn rates_take_their_factors_and_a_missing_one_stays_off_alone() {
        let loader = FakeHost::new()
            .without_rate(Rate::ResourceDropStackAmount)
            .install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Rates)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        assert_eq!(loader.rate(COMBAT), Some(3.0));
        assert_eq!(loader.rate(Rate::FactoryProductionSpeed), Some(2.0));
        assert_eq!(loader.rate(Rate::ResourceDropStackAmount), None);
        assert_eq!(loader.rate(Rate::Experience(ExperienceGroup::Mining)), None);
        assert_eq!(loader.rate(ROOFS), Some(1.5));
        assert_eq!(loader.rate(Rate::RecipeOutput(RecipeKind::Other)), None);
    }
}
