//! Build rights: whether a player may edit a box, as the loader answers.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::WorldError;
    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::world::{BuildRefusal, Player, PlayerPermissions, World};
    use enshwick::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Builder;

    impl Mod for Builder {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Builder,
        r#"
        id = "you.builder"
        side = "server"
        "#
    );

    const WORLDER: u32 = 2;
    const BUILDER: u32 = 3;
    const GUEST: u32 = 4;

    fn info(entity: u32, permissions: u8) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity: Entity(entity),
            key: enshwick::PlayerKey::new(u64::from(entity)),
            permissions,
            name_len: 0,
            name: [0; 64],
        }
    }

    fn player(entity: u32) -> Player {
        Player {
            entity: Entity(entity),
            slot: entity,
            key: enshwick::PlayerKey::new(u64::from(entity)),
            permissions: 0,
            name: String::new(),
        }
    }

    /// A base's zone from 0 to 40 m, a world area of 1 km, a strict
    /// no-build zone at 100 m and a lenient one at 200 m; one player with
    /// `CanEditWorld` alone, one with `CanEditBase` too, one with neither.
    fn installed() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .player(info(WORLDER, PlayerInfo::CAN_EDIT_WORLD))
            .player(info(
                BUILDER,
                PlayerInfo::CAN_EDIT_WORLD | PlayerInfo::CAN_EDIT_BASE,
            ))
            .player(info(GUEST, PlayerInfo::CAN_ACCESS_INVENTORIES))
            .building_zone([0.0; 3], [40.0; 3])
            .building_area([0.0; 3], [1000.0; 3])
            .no_build_zone([100.0; 3], [110.0; 3], true)
            .no_build_zone([200.0; 3], [210.0; 3], false)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Builder)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    fn can(entity: u32, min: f64, max: f64) -> Result<(), BuildRefusal> {
        WORLD
            .get()
            .unwrap()
            .can_build(&player(entity), [min; 3], [max; 3])
    }

    #[test]
    fn every_edit_needs_can_edit_world() {
        let _loader = installed();
        assert_eq!(
            can(GUEST, 50.0, 52.0),
            Err(BuildRefusal::NoPermission(PlayerPermissions::CanEditWorld))
        );
    }

    #[test]
    fn a_player_no_longer_in_game_has_no_permissions() {
        let _loader = installed();
        assert_eq!(
            can(99, 50.0, 52.0),
            Err(BuildRefusal::NoPermission(PlayerPermissions::CanEditWorld))
        );
    }

    #[test]
    fn an_edit_in_a_building_zone_needs_can_edit_base() {
        let _loader = installed();
        assert_eq!(
            can(WORLDER, 38.0, 42.0),
            Err(BuildRefusal::NoPermission(PlayerPermissions::CanEditBase))
        );
        assert_eq!(can(BUILDER, 38.0, 42.0), Ok(()));
    }

    #[test]
    fn outside_every_building_zone_can_edit_world_is_enough() {
        let _loader = installed();
        assert_eq!(can(WORLDER, 50.0, 52.0), Ok(()));
        assert_eq!(
            can(WORLDER, 40.0, 42.0),
            Ok(()),
            "sharing a face is outside"
        );
    }

    #[test]
    fn a_corner_past_the_building_area_is_refused() {
        let _loader = installed();
        assert_eq!(
            can(BUILDER, 998.0, 1000.0),
            Err(BuildRefusal::CursorOutsideBuildingZone)
        );
    }

    #[test]
    fn only_a_strict_no_build_zone_refuses() {
        let _loader = installed();
        assert_eq!(
            can(BUILDER, 105.0, 106.0),
            Err(BuildRefusal::CursorInsideStrictBuildInhibitorZone)
        );
        assert_eq!(can(BUILDER, 205.0, 206.0), Ok(()));
    }

    #[test]
    fn corners_come_in_any_order() {
        let _loader = installed();
        let world = WORLD.get().unwrap();
        assert_eq!(
            world.can_build(&player(WORLDER), [42.0, 30.0, 42.0], [38.0, 35.0, 38.0]),
            Err(BuildRefusal::NoPermission(PlayerPermissions::CanEditBase))
        );
    }

    #[test]
    fn a_corner_that_is_not_finite_is_refused_unchecked() {
        let _loader = installed();
        let world = WORLD.get().unwrap();
        assert_eq!(
            world.can_build(&player(BUILDER), [f64::NAN, 0.0, 0.0], [1.0; 3]),
            Err(BuildRefusal::Unchecked(WorldError::Refused))
        );
    }

    #[test]
    fn a_check_off_the_game_thread_is_refused_unchecked() {
        let _loader = installed();
        let world = *WORLD.get().unwrap();
        let elsewhere =
            std::thread::spawn(move || world.can_build(&player(BUILDER), [50.0; 3], [52.0; 3]))
                .join()
                .unwrap();
        assert_eq!(
            elsewhere,
            Err(BuildRefusal::Unchecked(WorldError::WrongThread))
        );
    }
}
