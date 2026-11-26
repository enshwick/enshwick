//! Buffs: types, an entity's buffs, giving and removing, as the loader
//! receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::Entity;
    use enshwick::WorldError;
    use enshwick::abi::{BuffTypeInfo, Status};
    use enshwick::buffs::{Apply, BuffSlotType, BuffTypeId, Buffs};
    use enshwick::{Load, Mod, Off};

    static BUFFS: OnceLock<Buffs> = OnceLock::new();

    struct Blessing;

    impl Mod for Blessing {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = BUFFS.set(load.buffs());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Blessing,
        r#"
        id = "you.blessing"
        side = "server"
        "#
    );

    #[test]
    fn buffs_are_listed_given_renewed_and_removed() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Blessing)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let buffs = BUFFS.get().unwrap();

        let swift = enshwick::abi::headed!(BuffTypeInfo {
            id: BuffTypeId::new(7),
            slot: BuffTypeInfo::BUFF,
            flags: BuffTypeInfo::SAVED,
            name: 99,
            duration_ns: 60_000_000_000,
        });
        loader.set_buffs(&[swift], &[Entity::from_engine(2)]);
        let types = buffs.types().unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].slot, BuffSlotType::Buff);
        assert!(types[0].is_saved && !types[0].hide_in_ui);
        assert_eq!(types[0].default_life_time, Duration::from_mins(1));

        let player = Entity::from_engine(2);
        buffs.apply(player, BuffTypeId(7), Apply::new()).unwrap();
        let on = buffs.of(player).unwrap();
        assert_eq!(on.len(), 1);
        assert_eq!(on[0].remaining, Duration::from_mins(1), "the type's own");
        assert_eq!(on[0].level, None);
        buffs
            .apply(
                player,
                BuffTypeId(7),
                Apply::new().lasting(Duration::from_mins(5)),
            )
            .unwrap();
        let on = buffs.of(player).unwrap();
        assert_eq!(on.len(), 1, "renewed, not doubled");
        assert_eq!(on[0].life_time, Duration::from_mins(5));
        assert_eq!(buffs.has(player, BuffTypeId(7)), Ok(true));
        let held = loader.buffs_of(player).expect("the player holds buffs");
        assert_eq!(held.len(), 1, "the fake holds what the mod left");
        assert_eq!(held[0].id, BuffTypeId(7));
        assert_eq!(held[0].duration_ns, 300_000_000_000);

        assert_eq!(
            buffs.apply(player, BuffTypeId(8), Apply::new()),
            Err(WorldError::Refused),
            "unknown type"
        );
        assert_eq!(
            buffs.apply(Entity::from_engine(3), BuffTypeId(7), Apply::new()),
            Err(WorldError::Lacks)
        );

        assert_eq!(buffs.remove(player, BuffTypeId(7)), Ok(true));
        assert_eq!(buffs.remove(player, BuffTypeId(7)), Ok(false));
        assert_eq!(buffs.has(player, BuffTypeId(7)), Ok(false));
        assert_eq!(
            buffs.has(Entity::from_engine(3), BuffTypeId(7)),
            Ok(false),
            "an entity that holds no buffs"
        );

        let elsewhere =
            std::thread::spawn(move || (buffs.types(), buffs.has(player, BuffTypeId(7))))
                .join()
                .unwrap();
        assert_eq!(elsewhere.0, Err(WorldError::WrongThread));
        assert_eq!(elsewhere.1, Err(WorldError::WrongThread));
    }
}
