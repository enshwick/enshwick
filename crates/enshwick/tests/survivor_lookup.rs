//! Survivor lookups: the fake host hands a mod's answer back as it means it.

#[cfg(test)]
mod tests {
    use enshwick::Entity;
    use enshwick::abi::{Status, SurvivorLookup};
    use enshwick::survivors::{Answer, Purpose, SurvivorId};
    use enshwick::{Load, Mod, Off};

    const HOMELESS: SurvivorId = SurvivorId(7);
    const ROAMER: SurvivorId = SurvivorId(8);

    struct Housing;

    impl Mod for Housing {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.housing", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_survivor_lookup(|lookup| match (lookup.survivor, lookup.purpose) {
                (HOMELESS, _) => Answer::Nobody,
                (ROAMER, Purpose::Roaming { entity, .. }) => Answer::Entity(entity),
                _ => Answer::Vanilla,
            })?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Housing);

    #[test]
    fn a_lookup_answers_nobody_an_entity_or_the_game_s_own() {
        let loader = enshwick::test::FakeHost::new().install();
        let ask = |survivor, entity| {
            loader.survivor_lookup(
                survivor,
                Some(Entity(10)),
                SurvivorLookup::ROAMING,
                entity,
                [0; 3],
                &[],
            )
        };
        assert_eq!(ask(HOMELESS, None), Answer::Vanilla, "before init");

        // SAFETY: both functions are the exports of this file's `declare_mod!(Housing)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(ask(HOMELESS, None), Answer::Nobody);
        assert_eq!(ask(ROAMER, Some(Entity(11))), Answer::Entity(Entity(11)));
        assert_eq!(ask(SurvivorId(9), None), Answer::Vanilla);
    }
}
