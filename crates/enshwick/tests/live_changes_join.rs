//! A mod or pack of side server cannot declare a change players' games
//! read, and may declare one only the server reads; a pack of side both may.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::content::changes::{Field, Items, Target};
    use enshwick::test::FakeHost;

    struct Alone;

    impl enshwick::Mod for Alone {
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

    enshwick::test::declare_mod!(
        Alone,
        r#"
        id = "you.alone"
        side = "server"
        "#
    );

    #[test]
    fn a_change_players_read_needs_a_mod_that_joins_and_loot_does_not() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Alone)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let live = loader.live_changes();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].field, Field::StackSize);
    }

    #[test]
    fn a_pack_of_side_both_declares_a_change_players_read_and_one_of_side_server_does_not() {
        use enshwick::abi::{ModHandle, Side};

        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process; the loader
        // makes its own handle the host of the copy that runs the packs.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        let declare = |me, side| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick::__private::init_pack(loader.api(), ModHandle(me), side, |load| {
                    let target = Target::Items(Items::Stackable);
                    load.declare_change(target, Field::MaxStackSize, "x")
                        .map(drop)
                })
            }
        };
        assert_eq!(declare(1, Side::SERVER), Status::REFUSED);
        assert_eq!(declare(2, Side::BOTH), Status::OK, "{:?}", loader.logs());
        let live = loader.live_changes();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].field, Field::MaxStackSize);
    }
}
