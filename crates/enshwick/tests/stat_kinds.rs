//! Stat kinds and modifiers through the fake loader: a kind defined and a
//! built-in one opened, modifiers from a mod, a buff and a worn item summed
//! in source order, changes raised once, and what is refused.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::{BuffTypeInfo, Header, Status};
    use enshwick::buffs::{Apply, BuffId, Buffs};
    use enshwick::ecs::Entity;
    use enshwick::item_data::ItemInstance;
    use enshwick::stats::{
        ItemStats, Lasting, Modifier, Op, Origin, Stat, StatChange, StatDef, Stats, builtin,
    };
    use enshwick::{Load, Mod, Off};

    const BLESSED: u32 = 0xb1e5;

    struct Kit {
        buffs: Buffs,
        stats: Stats,
        luck: Stat,
        fire: Stat,
    }

    static KIT: OnceLock<Kit> = OnceLock::new();
    static CHANGES: Mutex<Vec<StatChange>> = Mutex::new(Vec::new());

    struct Rpg;

    impl Mod for Rpg {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "fake", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let luck = load.define_stat("luck", &StatDef::additive("stat.luck").range(0.0, 1.0))?;
            let fire = load.open_stat(builtin::RESISTANCE_FIRE)?;
            assert!(
                load.define_stat("other:luck", &StatDef::additive("x"))
                    .is_err(),
                "another mod's namespace"
            );
            assert!(
                load.define_stat("luck", &StatDef::additive("x")).is_err(),
                "defined twice"
            );
            load.buff_modifiers(BuffId(BLESSED), &[Modifier::new(luck, Op::Add, 0.25)])?;
            load.on_stats_changed(|change| {
                CHANGES
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(*change);
            })?;
            let stats = load.stats()?;
            let buffs = load.buffs();
            let _first = KIT.set(Kit {
                buffs,
                stats,
                luck,
                fire,
            });
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Rpg);

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn sources_add_up_in_order_and_changes_are_raised_once() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Rpg)`.
        let started = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(started, Status::OK);
        let Kit {
            buffs,
            stats,
            luck,
            fire,
        } = KIT.get().unwrap();
        let (buffs, stats, luck, fire) = (*buffs, *stats, *luck, *fire);
        let player = Entity(2);

        assert!(close(stats.value(player, luck), 0.0));
        let info = stats.info(fire).unwrap();
        assert_eq!(info.id, builtin::RESISTANCE_FIRE);
        assert_eq!(stats.info(luck).unwrap().id, "fake:luck");

        stats
            .set(
                player,
                "bless",
                Modifier::new(luck, Op::Add, 0.1),
                Lasting::UntilRemoved,
            )
            .unwrap();
        stats
            .set(
                player,
                "bless",
                Modifier::new(luck, Op::Add, 0.2),
                Lasting::UntilRemoved,
            )
            .unwrap();
        assert!(
            close(stats.value(player, luck), 0.2),
            "the same key replaces"
        );

        let blessed = BuffTypeInfo {
            header: Header::of::<BuffTypeInfo>(),
            id: BLESSED,
            slot: BuffTypeInfo::BUFF,
            flags: 0,
            name: 0,
            duration: 60_000_000_000,
        };
        loader.set_buffs(&[blessed], &[player.0]);
        buffs.apply(player, BuffId(BLESSED), Apply::new()).unwrap();
        let ring = ItemInstance {
            pide: Entity(900),
            item: 0x5e01,
        };
        loader.wear(player.0, ring);
        stats
            .set_item(
                ring,
                &ItemStats {
                    modifiers: vec![
                        Modifier::new(luck, Op::Multiply, 2.0).to_item(),
                        Modifier::new(fire, Op::Add, 0.3).to_item(),
                    ],
                    ..ItemStats::default()
                },
            )
            .unwrap();
        // (0.25 + 0.2) * 2, kept within the range.
        assert!(close(stats.value(player, luck), 0.9));
        assert!(close(stats.value(player, fire), 0.3));

        let order: Vec<Origin> = stats
            .sources(player, Some(luck))
            .iter()
            .map(|s| s.origin)
            .collect();
        assert!(matches!(
            order.as_slice(),
            [
                Origin::Item(_),
                Origin::Buff(BuffId(BLESSED)),
                Origin::Mod { .. }
            ]
        ));

        assert_eq!(loader.settle_stats(), 2);
        assert_eq!(loader.settle_stats(), 0, "nothing moved since");
        let changes = CHANGES.lock().unwrap().clone();
        assert!(changes.iter().any(|c| c.stat == luck && close(c.new, 0.9)));

        stats.remove(player, "bless", luck).unwrap();
        assert!(close(stats.value(player, luck), 0.5));
        assert!(loader.stat_modifiers().is_empty());
    }
}
