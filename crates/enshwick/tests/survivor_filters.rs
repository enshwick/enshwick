//! Survivor beds, summoning staffs and the survivor menu: what the game asks
//! a mod reaches its filter and its answer comes back as the mod means it.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{self, Header, Side, Status};
    use enshwick::survivors::{NpcId, SummonVerdict};
    use enshwick::{Entity, ItemId, Load, Mod, Off};

    const STAFF: ItemId = ItemId::new(0x5e01);
    const OTHER_STAFF: ItemId = ItemId::new(0x5e02);
    const BANNED: ItemId = ItemId::new(0x5e03);
    const FARMER: NpcId = NpcId(1_201_801_546);

    /// What the summon handler saw: item, own entity, meter, whether taken.
    static SUMMONED: Mutex<Vec<(ItemId, Option<Entity>, Option<[u16; 2]>, bool)>> =
        Mutex::new(Vec::new());

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
            Ok(Self)
        }
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
        abi::BedCheck {
            header: Header::of::<abi::BedCheck>(),
            npc_id: FARMER,
            entity: Entity(1),
            bed_id: Entity(2),
            home,
            bed_at,
        }
    }

    fn summon(item: ItemId, ammo: ItemId, consumed: u8) -> abi::Summon {
        abi::Summon {
            header: Header::of::<abi::Summon>(),
            item,
            ammo,
            template: [0; 16],
            target: [1, 2, 3],
            slot: 0,
            pide: Entity(900),
            meter: [3, 10],
            consumed,
            reserved: [0; 3],
        }
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
            [(STAFF, Some(Entity(900)), Some([3, 10]), true)]
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

    #[test]
    fn a_status_merge_is_refused_on_the_server() {
        let (loader, status) = menu_on(Side::SERVER);
        assert_ne!(status, Status::OK);
        assert_eq!(loader.survivor_status(FARMER, &[1, 3]), None);
    }
}
