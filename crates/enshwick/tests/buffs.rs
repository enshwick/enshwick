//! Buffs: types, an entity's buffs, giving and removing, as the loader
//! receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::abi::{BuffTypeInfo, Header, Status};
    use enshwick::buffs::{Apply, BuffId, BuffKind, Buffs};
    use enshwick::ecs::Entity;
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    static BUFFS: OnceLock<Buffs> = OnceLock::new();

    struct Blessing;

    impl Mod for Blessing {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "blessing", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = BUFFS.set(load.buffs());
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Blessing);

    #[test]
    fn buffs_are_listed_given_renewed_and_removed() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let buffs = BUFFS.get().unwrap();

        let swift = BuffTypeInfo {
            header: Header::of::<BuffTypeInfo>(),
            id: 7,
            slot: BuffTypeInfo::BUFF,
            flags: BuffTypeInfo::SAVED,
            name: 99,
            duration: 60_000_000_000,
        };
        loader.set_buffs(&[swift], &[2]);
        let types = buffs.types();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].kind, BuffKind::Buff);
        assert!(types[0].saved && !types[0].hidden);
        assert_eq!(types[0].duration, Duration::from_mins(1));

        let player = Entity(2);
        buffs.apply(player, BuffId(7), Apply::new()).unwrap();
        let on = buffs.of(player).unwrap();
        assert_eq!(on.len(), 1);
        assert_eq!(on[0].remaining, Duration::from_mins(1), "the type's own");
        assert_eq!(on[0].level, None);
        buffs
            .apply(
                player,
                BuffId(7),
                Apply::new().lasting(Duration::from_mins(5)),
            )
            .unwrap();
        let on = buffs.of(player).unwrap();
        assert_eq!(on.len(), 1, "renewed, not doubled");
        assert_eq!(on[0].duration, Duration::from_mins(5));
        assert!(buffs.has(player, BuffId(7)));

        assert_eq!(
            buffs.apply(player, BuffId(8), Apply::new()),
            Err(WorldError::Refused),
            "unknown type"
        );
        assert_eq!(
            buffs.apply(Entity(3), BuffId(7), Apply::new()),
            Err(WorldError::Lacks)
        );

        assert_eq!(buffs.remove(player, BuffId(7)), Ok(true));
        assert_eq!(buffs.remove(player, BuffId(7)), Ok(false));
        assert!(!buffs.has(player, BuffId(7)));
    }
}
