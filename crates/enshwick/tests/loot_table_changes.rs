//! A mod changes an item's loot in one fill table alone.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::content::changes::{Change, Field, Value};
    use enshwick::test::FakeHost;

    const TABLE: Guid = Guid([7; 16]);
    const FUR: Guid = Guid([1; 16]);

    struct Furs;

    impl enshwick::Mod for Furs {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.furs", side: SERVER)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            load.change(
                &Change::in_table(TABLE, FUR, Field::Count, Value::Times(2.0)),
                "furs",
            )?;
            let stacks = Change::in_table(TABLE, FUR, Field::StackSize, Value::Set(2.0));
            if load.change(&stacks, "stacks").is_ok() {
                return Err(enshwick::Off::new("a table took a random loot field"));
            }
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Furs);

    #[test]
    fn a_change_of_one_table_reaches_the_loader_with_its_item() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Furs)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        assert_eq!(
            loader.changes(),
            vec![(
                Change::in_table(TABLE, FUR, Field::Count, Value::Times(2.0)),
                "furs".to_owned()
            )]
        );
    }
}
