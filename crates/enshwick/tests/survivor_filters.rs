//! Survivor beds, summoning staffs and the survivor menu: what the game asks
//! a mod reaches its filter and its answer comes back as the mod means it.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use std::sync::Mutex;

    use enshwick::abi::{self, Side, Status};
    use enshwick::survivors::{NpcId, SummonVerdict};
    use enshwick::{Entity, ItemId, Load, Mod, Off};

    const STAFF: ItemId = ItemId::new(0x5e01);
    const OTHER_STAFF: ItemId = ItemId::new(0x5e02);
    const BANNED: ItemId = ItemId::new(0x5e03);
    const FARMER: NpcId = NpcId(1_201_801_546);

    /// What the summon handler saw of one use: item, own entity, meter,
    /// whether it took the item.
    type Seen = (ItemId, Option<Entity>, Option<[u16; 2]>, bool);

    static SUMMONED: Mutex<Vec<Seen>> = Mutex::new(Vec::new());

    struct Staffs;

    impl Mod for Staffs {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_survivor_bed_check(|check| {
                (0..3).all(|axis| (check.home[axis] - check.bed_at[axis]).abs() < 200)
            })?;
            load.on_summon(|summon| match summon.item {
                STAFF if summon.ammo.is_none() => SummonVerdict::Take,
                BANNED => SummonVerdict::Refuse,
                _ => SummonVerdict::Allow,
            })?;
            load.on_summoned(|summon| {
                SUMMONED.lock().unwrap().push((
                    summon.item,
                    summon.pide,
                    summon.meter.map(|bar| [bar.current, bar.max]),
                    summon.consumed,
                ));
            })?;
            load.add_summon_item(STAFF, FARMER)?;
            let host = enshwick::host().ok_or_else(|| Off::new("no host in init"))?;
            let status =
                (host.api().summon_filter)(host.handle(), Some(unknown), core::ptr::null_mut());
            assert_eq!(status, Status::OK);
            Ok(Self)
        }
    }

    /// A raw filter answering a value no ABI gives a summon filter, which
    /// the loader reads as "let the game decide", never as taking the item.
    unsafe extern "system" fn unknown(_: *const abi::Summon, _: *mut c_void) -> u8 {
        3
    }

    enshwick::test::declare_mod!(
        Staffs,
        r#"
        id = "you.staffs"
        side = "server"
        "#
    );

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        SUMMONED.lock().unwrap().clear();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Staffs)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        loader
    }

    fn bed(home: [i64; 3], bed_at: [i64; 3]) -> abi::BedCheck {
        enshwick::abi::headed!(abi::BedCheck {
            npc_id: FARMER,
            entity: Entity::from_engine(1),
            bed_id: Entity::from_engine(2),
            home,
            bed_at,
        })
    }

    fn summon(item: ItemId, ammo: ItemId, consumed: u8) -> abi::Summon {
        enshwick::abi::headed!(abi::Summon {
            item,
            ammo,
            template: enshwick::content::Guid([0; 16]),
            target: [1, 2, 3],
            slot: 0,
            pide: Entity::from_engine(900),
            meter: [3, 10],
            consumed,
            reserved: [0; 3],
        })
    }

    #[test]
    fn a_bed_check_keeps_a_near_bed_and_clears_a_far_one() {
        let loader = started();
        assert!(loader.survivor_bed_check(&bed([0, 0, 0], [100, -100, 199])));
        assert!(!loader.survivor_bed_check(&bed([0, 0, 0], [0, 0, 200])));
    }

    #[test]
    fn a_summon_filter_takes_refuses_or_allows_by_the_item_used() {
        let loader = started();
        assert_eq!(
            loader.summon(&summon(STAFF, ItemId::new(0), 0)),
            abi::Summon::TAKE
        );
        assert_eq!(
            loader.summon(&summon(BANNED, ItemId::new(0), 0)),
            abi::Summon::REFUSE
        );
        assert_eq!(loader.summon(&summon(OTHER_STAFF, ItemId::new(0), 0)), 0);
        assert_eq!(
            loader.summon(&summon(STAFF, ItemId::new(7), 0)),
            0,
            "with ammo"
        );
    }

    #[test]
    fn a_summon_handler_sees_the_item_its_meter_and_whether_it_was_taken() {
        let loader = started();
        assert_eq!(loader.summoned(&summon(STAFF, ItemId::new(0), 1)), 1);
        assert_eq!(
            *SUMMONED.lock().unwrap(),
            [(STAFF, Some(Entity::from_engine(900)), Some([3, 10]), true)]
        );
    }

    #[test]
    fn a_summon_item_is_filed_for_its_survivor() {
        let loader = started();
        assert_eq!(loader.summon_items(), [(STAFF, FARMER)]);
    }

    struct Menu;

    impl Mod for Menu {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_survivor_status(|_, statuses| {
                (statuses.len() >= 2).then(|| statuses.iter().fold(u8::MAX, |all, s| all & s))
            })?;
            Ok(Self)
        }
    }

    mod menu {
        enshwick::test::declare_mod!(
            super::Menu,
            r#"
            id = "you.menu"
            side = "both"
            "#
        );
    }

    fn menu_on(side: Side) -> (enshwick::test::Installed, Status) {
        let loader = enshwick::test::FakeHost::new().side(side).install();
        // SAFETY: both functions are the exports of `menu`'s `declare_mod!(Menu)`.
        let status = unsafe {
            loader.start(
                menu::enshwick_mod_query,
                menu::enshwick_mod_init,
                menu::enshwick_mod_ready,
            )
        };
        (loader, status)
    }

    #[test]
    fn a_status_merge_answers_for_several_entities_and_leaves_one_to_the_game() {
        let (loader, status) = menu_on(Side::CLIENT);
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        assert_eq!(
            loader.survivor_status(FARMER, &[0b0111, 0b1101]),
            Some(0b0101)
        );
        assert_eq!(loader.survivor_status(FARMER, &[0b0111]), None);
    }

    unsafe extern "system" fn no_merge(_: NpcId, _: *const u8, _: u32, _: *mut c_void) -> u32 {
        0
    }

    fn ask_merge(loader: &enshwick::test::Installed, as_mod: abi::ModHandle) -> Status {
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        (api.survivor_status)(as_mod, Some(no_merge), core::ptr::null_mut())
    }

    #[test]
    fn a_status_merge_is_unavailable_on_the_server() {
        let loader = enshwick::test::FakeHost::new().side(Side::SERVER).install();
        assert_eq!(ask_merge(&loader, loader.handle()), Status::UNAVAILABLE);
        assert_eq!(loader.survivor_status(FARMER, &[1, 3]), None);
    }

    #[test]
    fn every_mod_may_register_a_status_merge_and_register_its_own_again() {
        let other = abi::ModHandle(2);
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .other_mod(other)
            .install();
        assert_eq!(ask_merge(&loader, loader.handle()), Status::OK);
        assert_eq!(ask_merge(&loader, loader.handle()), Status::OK);
        assert_eq!(ask_merge(&loader, other), Status::OK);
        assert_eq!(
            ask_merge(&loader, abi::ModHandle(3)),
            Status::BAD_ARGS,
            "a handle the loader never gave out"
        );
    }

    #[test]
    fn a_status_merge_after_init_comes_too_late() {
        let (loader, status) = menu_on(Side::CLIENT);
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        assert_eq!(ask_merge(&loader, loader.handle()), Status::TOO_LATE);
    }
}
