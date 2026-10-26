//! A mod declares changes at init and sets their values later.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::content::changes::{ChangeHandle, Field, Items, Target, Value};
    use enshwick::state::ModLocal;
    use enshwick::test::FakeHost;

    const WOOD: Guid = Guid::parse("3f4093ae-75ed-4e5d-bd0b-784ba83b88f5");

    static STACKS: ModLocal<ChangeHandle> = ModLocal::new();

    struct Stacks;

    impl enshwick::Mod for Stacks {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.stacks", side: BOTH, join: true)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let stacks = load.declare_change(
                Target::Items(Items::Stackable),
                Field::MaxStackSize,
                "stacks",
            )?;
            let _set = STACKS.set(stacks);
            load.declare_change(Target::Entry(WOOD), Field::MaxStackSize, "wood")?;
            if load
                .declare_change(Target::Entry(WOOD), Field::Ingredient, "x")
                .is_ok()
            {
                return Err(enshwick::Off::new("an ingredient took"));
            }
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Stacks);

    #[test]
    fn a_change_holds_no_value_until_set_and_takes_each_set_and_reset() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Stacks)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let live = loader.live_changes();
        assert_eq!(live.len(), 2);
        assert_eq!(live[0].target, Target::Items(Items::Stackable));
        assert_eq!(live[1].target, Target::Entry(WOOD));
        assert!(live.iter().all(|c| c.value.is_none()));

        let stacks = STACKS.get().copied().unwrap();
        stacks.set(Value::Times(4.0)).unwrap();
        assert_eq!(loader.live_changes()[0].value, Some(Value::Times(4.0)));
        assert!(stacks.set(Value::Set(70000.0)).is_err());
        assert_eq!(loader.live_changes()[0].value, Some(Value::Times(4.0)));
        stacks.reset().unwrap();
        assert_eq!(loader.live_changes()[0].value, None);
    }
}
