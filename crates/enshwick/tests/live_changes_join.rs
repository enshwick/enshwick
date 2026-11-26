//! A mod or pack of side server cannot declare a change players' games
//! read, and may declare one only the server reads; a pack of side both may.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ChangeId, ChangeOp, DataChange, DataField, DataSelect, Status, Str};
    use enshwick::content::Guid;
    use enshwick::content::changes::{Field, Target};
    use enshwick::test::FakeHost;

    struct Alone;

    impl enshwick::Mod for Alone {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let wood = Target::Entry(Guid([3; 16]));
            if load.declare_change(wood, Field::BurnTime, "x").is_ok() {
                return Err(enshwick::Off::new("a burn time took without join"));
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

    /// A raw record of a change of `field` with `op` and `value` through
    /// `select`.
    fn record(field: DataField, select: DataSelect, op: ChangeOp, value: f64) -> DataChange {
        enshwick::abi::headed!(DataChange {
            entry: if select == DataSelect::ENTRY {
                Guid([7; 16])
            } else {
                Guid::NIL
            },
            field,
            op,
            value,
            source: Str::new("raw"),
            item: Guid::NIL,
            select,
            categories: 0,
        })
    }

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

        let loader = FakeHost::new()
            .pack(ModHandle(2), Side::BOTH)
            .pack(ModHandle(3), Side::SERVER)
            .install();
        // SAFETY: the fake host's table lives for the process; the loader
        // makes its own handle the host of the copy that runs the packs.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle::NONE) };
        let declare = |me, side| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick::__private::init_pack(loader.api(), ModHandle(me), side, |load| {
                    let target = Target::Entry(Guid([3; 16]));
                    load.declare_change(target, Field::BurnTime, "x").map(drop)
                })
            }
        };
        assert_eq!(declare(1, Side::SERVER), Status::REFUSED);
        assert_eq!(declare(2, Side::BOTH), Status::OK, "{:?}", loader.logs());
        let live = loader.live_changes();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].field, Field::BurnTime);

        // The raw entry refuses by the caller's own side, as the loader does.
        let raw = |me, field: DataField| {
            let change = record(field, DataSelect::ENTRY, ChangeOp::TIMES, 1.0);
            let mut id = ChangeId(0);
            // SAFETY: the fake host's table lives for the process; `change`
            // and `id` are locals alive for the call.
            (unsafe { &*loader.api() }.change_data)(ModHandle(me), &raw const change, &raw mut id)
        };
        let burn = Field::BurnTime.to_abi();
        assert_eq!(raw(3, burn), Status::REFUSED);
        assert_eq!(raw(4, burn), Status::BAD_ARGS, "a handle never given out");
        assert_eq!(raw(2, burn), Status::OK);
        assert_eq!(
            raw(3, Field::StackSize.to_abi()),
            Status::OK,
            "loot only the server reads"
        );
        // A stack size and container slots, which only `unstable` names,
        // are items' fields whatever the build.
        assert_eq!(raw(3, DataField::MAX_STACK), Status::REFUSED);
        assert_eq!(raw(2, DataField::CONTAINER_SLOTS), Status::OK);
    }

    #[test]
    fn a_raw_stack_size_or_slots_change_is_answered_as_the_loader_answers_it() {
        let loader = FakeHost::new().install();
        let me = loader.handle();
        let fixed = |change: DataChange| {
            // SAFETY: the fake host's table lives for the process; `change`
            // is a local alive for the call.
            (unsafe { &*loader.api() }.change_data)(me, &raw const change, core::ptr::null_mut())
        };
        let (slots, stacks) = (DataField::CONTAINER_SLOTS, DataField::MAX_STACK);
        let containers = DataSelect::CONTAINERS;
        assert_eq!(
            fixed(record(slots, containers, ChangeOp::SET, 48.0)),
            Status::OK
        );
        assert_eq!(
            fixed(record(stacks, DataSelect::ENTRY, ChangeOp::TIMES, 2.0)),
            Status::OK
        );
        assert_eq!(
            fixed(record(slots, containers, ChangeOp::SET, 50.0)),
            Status::BAD_ARGS,
            "slots come in pages of 8"
        );
        assert_eq!(
            fixed(record(slots, containers, ChangeOp::TIMES, 0.5)),
            Status::BAD_ARGS,
            "slots only grow"
        );
        assert_eq!(
            fixed(record(stacks, DataSelect::RECIPE_KIND, ChangeOp::SET, 10.0)),
            Status::BAD_ARGS,
            "no recipe selection reaches a stack size"
        );

        // A live one starts as times 1, whatever the record says.
        let mut id = ChangeId(0);
        let live = record(slots, containers, ChangeOp::SET, 50.0);
        // SAFETY: the fake host's table lives for the process; `live` and `id` are locals alive for the call.
        let status = (unsafe { &*loader.api() }.change_data)(me, &raw const live, &raw mut id);
        assert_eq!(status, Status::OK);
        let set = |op, value| {
            // SAFETY: the fake host's table lives for the process.
            (unsafe { &*loader.api() }.set_change)(me, id, op, value)
        };
        assert_eq!(set(ChangeOp::SET, 64.0), Status::OK);
        assert_eq!(set(ChangeOp::SET, 60.0), Status::BAD_ARGS);
        assert_eq!(set(ChangeOp::RESET, 0.0), Status::OK);
    }
}
