//! A mod without `join: true` cannot declare a rate clients count too.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::rates::Rate;
    use enshwick::test::FakeHost;

    struct Alone;

    impl enshwick::Mod for Alone {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.alone", side: SERVER)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let roofs = Rate::RecipeOutput(enshwick::content::recipes::RecipeKind::Roof);
            if load.rate(roofs).is_ok() {
                return Err(enshwick::Off::new("a recipe output rate took without join"));
            }
            load.rate(Rate::WorkshopSpeed)?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Alone);

    #[test]
    fn a_rate_clients_count_needs_a_mod_that_joins() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Alone)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::REFUSED);
        assert_eq!(loader.rate(Rate::WorkshopSpeed), None);
    }
}
