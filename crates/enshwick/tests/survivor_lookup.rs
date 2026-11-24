//! Survivor lookups: the fake host hands a mod's answer back as it means it,
//! and the lookups and menu merges of two mods chain in load order.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    use enshwick::Entity;
    use enshwick::abi::{ModHandle, SURVIVOR_STATUS_SET, Side, Status, SurvivorLookup};
    use enshwick::survivors::{Answer, NpcId, Purpose};
    use enshwick::{Load, Mod, Off};

    /// A mod loaded before `Housing`, registering through the ABI.
    const EARLIER: ModHandle = ModHandle(77);

    /// The earlier mod's lookup: it answers only for `HOMELESS`, with
    /// entity 5, and passes on the rest.
    unsafe extern "system" fn earlier_lookup(
        call: *const SurvivorLookup,
        _user: *mut c_void,
        out: *mut Entity,
    ) -> u8 {
        // SAFETY: the fake host passes a valid lookup and out for the call.
        let survivor = unsafe { (*call).survivor };
        if survivor != HOMELESS {
            return 0;
        }
        // SAFETY: the fake host passes an `out` writable for one `Entity` for the call.
        unsafe { out.write(Entity(5)) };
        1
    }

    /// The earlier mod's menu merge: it sets 9 for `HOMELESS` and passes on
    /// the rest.
    unsafe extern "system" fn earlier_status(
        survivor: NpcId,
        _statuses: *const u8,
        _count: u32,
        _user: *mut c_void,
    ) -> u32 {
        if survivor == HOMELESS {
            SURVIVOR_STATUS_SET | 9
        } else {
            0
        }
    }

    /// A later mod's menu merge, which always sets 4.
    unsafe extern "system" fn later_status(
        _survivor: NpcId,
        _statuses: *const u8,
        _count: u32,
        _user: *mut c_void,
    ) -> u32 {
        SURVIVOR_STATUS_SET | 4
    }

    const HOMELESS: NpcId = NpcId(7);
    const ROAMER: NpcId = NpcId(8);

    struct Housing;

    impl Mod for Housing {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_survivor_lookup(|lookup| match (lookup.survivor, lookup.purpose) {
                (HOMELESS, _) => Answer::Nobody,
                (ROAMER, Purpose::Roaming { entity, .. }) => Answer::Entity(entity),
                _ => Answer::Vanilla,
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Housing,
        r#"
        id = "you.housing"
        side = "server"
        "#
    );

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
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(ask(HOMELESS, None), Answer::Nobody);
        assert_eq!(ask(ROAMER, Some(Entity(11))), Answer::Entity(Entity(11)));
        assert_eq!(ask(NpcId(9), None), Answer::Vanilla);
    }

    #[test]
    fn two_mods_lookups_chain_in_load_order_and_the_first_answer_decides() {
        let loader = enshwick::test::FakeHost::new().other_mod(EARLIER).install();
        // SAFETY: the installed fake's table, alive while `loader` is.
        let api = unsafe { &*loader.api() };
        let earlier = (api.survivor_lookup)(EARLIER, Some(earlier_lookup), core::ptr::null_mut());
        assert_eq!(earlier, Status::OK);
        // SAFETY: both functions are the exports of this file's `declare_mod!(Housing)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "a second mod's lookup is not refused");
        let ask = |survivor, entity| {
            loader.survivor_lookup(survivor, None, SurvivorLookup::ROAMING, entity, [0; 3], &[])
        };
        assert_eq!(
            ask(HOMELESS, None),
            Answer::Entity(Entity(5)),
            "the earlier mod decides"
        );
        assert_eq!(
            ask(ROAMER, Some(Entity(11))),
            Answer::Entity(Entity(11)),
            "the earlier mod passes on to the later"
        );
        assert_eq!(ask(NpcId(9), None), Answer::Vanilla, "both pass on");
    }

    #[test]
    fn two_mods_menu_merges_chain_in_load_order_and_the_first_set_status_decides() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .other_mod(EARLIER)
            .other_mod(ModHandle(78))
            .install();
        // SAFETY: the installed fake's table, alive while `loader` is.
        let api = unsafe { &*loader.api() };
        let null = core::ptr::null_mut();
        assert_eq!(
            (api.survivor_status)(EARLIER, Some(earlier_status), null),
            Status::OK
        );
        let later = (api.survivor_status)(ModHandle(78), Some(later_status), null);
        assert_eq!(later, Status::OK, "a second mod's merge is not refused");
        assert_eq!(loader.survivor_status(HOMELESS, &[1, 2]), Some(9));
        assert_eq!(loader.survivor_status(ROAMER, &[1, 2]), Some(4));
    }
}
