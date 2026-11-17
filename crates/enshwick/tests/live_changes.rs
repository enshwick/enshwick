//! A mod declares changes at init and sets their values later.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::content::changes::{ChangeHandle, Field, Target, Value};
    use enshwick::state::ModLocal;
    use enshwick::test::FakeHost;

    const WOOD: Guid = Guid::parse("3f4093ae-75ed-4e5d-bd0b-784ba83b88f5");

    static BURN: ModLocal<ChangeHandle> = ModLocal::new();

    struct Fuel;

    impl enshwick::Mod for Fuel {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let burn = load.declare_change(Target::Entry(WOOD), Field::BurnTime, "burn")?;
            let _set = BURN.set(burn);
            load.declare_change(Target::Entry(WOOD), Field::DamageMod, "wood")?;
            if load
                .declare_change(Target::Entry(WOOD), Field::Ingredient, "x")
                .is_ok()
            {
                return Err(enshwick::Off::new("an ingredient took"));
            }
            #[cfg(feature = "unstable")]
            {
                use enshwick::content::changes::Items;
                let stacks = Target::Items(Items::Stackable);
                load.declare_change(stacks, Field::MaxStackSize, "stacks")?;
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Fuel,
        r#"
        id = "you.fuel"
        side = "both"
        "#
    );

    #[test]
    fn a_change_holds_no_value_until_set_and_takes_each_set_and_reset() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Fuel)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let live = loader.live_changes();
        let declared = if cfg!(feature = "unstable") { 3 } else { 2 };
        assert_eq!(live.len(), declared, "{live:?}");
        assert_eq!(live[0].target, Target::Entry(WOOD));
        assert_eq!(live[1].field, Field::DamageMod);
        assert!(live.iter().all(|c| c.value.is_none()));
        #[cfg(feature = "unstable")]
        assert_eq!(
            live[2].target,
            Target::Items(enshwick::content::changes::Items::Stackable)
        );

        let burn = BURN.get().copied().unwrap();
        burn.set(Value::Times(4.0)).unwrap();
        assert_eq!(loader.live_changes()[0].value, Some(Value::Times(4.0)));
        assert!(burn.set(Value::Set(90000.0)).is_err());
        assert_eq!(loader.live_changes()[0].value, Some(Value::Times(4.0)));
        burn.reset().unwrap();
        assert_eq!(loader.live_changes()[0].value, None);
    }
}
