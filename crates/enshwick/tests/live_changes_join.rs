//! A mod without `join: true` cannot declare a change players' games read,
//! and may declare one only the server reads.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::content::changes::{Field, Items, Target};
    use enshwick::test::FakeHost;

    struct Alone;

    impl enshwick::Mod for Alone {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.alone", side: SERVER)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let target = Target::Items(Items::Stackable);
            if load
                .declare_change(target, Field::MaxStackSize, "x")
                .is_ok()
            {
                return Err(enshwick::Off::new("a stack size took without join"));
            }
            let torch = Target::Entry(Guid([7; 16]));
            load.declare_change(torch, Field::StackSize, "loot")?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Alone);

    #[test]
    fn a_change_players_read_needs_a_mod_that_joins_and_loot_does_not() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Alone)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let live = loader.live_changes();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].field, Field::StackSize);
    }
}
