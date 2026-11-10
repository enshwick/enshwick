//! Watched entities: an entity that is no player counts its buffs' stat
//! modifiers only while a mod watches it.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::{BuffTypeInfo, Header, Status};
    use enshwick::buffs::{Apply, BuffTypeId, Buffs};
    use enshwick::content::id;
    use enshwick::stats::{Modifier, Op, Stat, StatChange, StatDef, Stats};
    use enshwick::{Entity, Load, Mod, Off};

    const CURSED: BuffTypeId = BuffTypeId::new(0xc0de);
    const ENEMY: Entity = Entity(50);

    static KIT: OnceLock<(Buffs, Stats, Stat)> = OnceLock::new();
    static CHANGES: Mutex<Vec<StatChange>> = Mutex::new(Vec::new());

    struct Curses;

    impl Mod for Curses {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let doom = load.define_stat(id!("doom"), &StatDef::additive())?;
            load.add_buff_modifiers(CURSED, &[Modifier::new(doom, Op::Add, 1.0)])?;
            load.on_stats_changed(|change| CHANGES.lock().unwrap().push(*change))?;
            let _first = KIT.set((load.buffs(), load.stats()?, doom));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Curses,
        r#"
        id = "you.hello"
        side = "server"
        "#
    );

    #[test]
    fn an_entity_counts_its_buffs_only_while_watched() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Curses)`.
        let started =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(started, Status::OK, "{:?}", loader.faults());
        let (buffs, stats, doom) = *KIT.get().unwrap();
        let cursed = BuffTypeInfo {
            header: Header::of::<BuffTypeInfo>(),
            id: CURSED,
            slot: BuffTypeInfo::DEBUFF,
            flags: 0,
            name: 0,
            duration: 60_000_000_000,
        };
        loader.set_buffs(&[cursed], &[ENEMY]);
        buffs.apply(ENEMY, CURSED, Apply::new()).unwrap();

        assert_eq!(loader.settle_stats(), 0, "unwatched");
        stats.watch(ENEMY, true);
        assert_eq!(loader.settle_stats(), 1);
        assert!(
            CHANGES
                .lock()
                .unwrap()
                .iter()
                .any(|c| c.entity == ENEMY && c.stat == doom)
        );
        stats.watch(ENEMY, false);
        CHANGES.lock().unwrap().clear();
        assert_eq!(loader.settle_stats(), 0, "no longer watched");
    }
}
