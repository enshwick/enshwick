//! The map's markers and each player's waypoint, as the loader answers them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{AltarInfo, Bounds, Header, MapMarkerInfo, PlayerInfo, Status};
    use enshwick::altars::Altars;
    use enshwick::content::Guid;
    use enshwick::ecs::Entity;
    use enshwick::map::{
        Map, MapAltar, MapMarker, MapMarkerTypeId, MapPlayer, MapPoint, MapSnapshot,
    };
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    const WAYPOINT: u32 = 2;
    const TRANSFORM: u32 = 3;
    const USED_ITEM: u32 = 4;
    const TOWN: Guid = Guid::parse("5d2e6a2b-0c3f-4d8e-9a71-3e5f0b1c2d4e");
    const LABEL: Guid = Guid::parse("0b6f4c1a-7e2d-4a90-b3c8-1d2e3f405162");

    static MAP: OnceLock<Map> = OnceLock::new();
    static ALTARS: OnceLock<Altars> = OnceLock::new();

    struct Cartographer;

    impl Mod for Cartographer {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.cartographer", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = MAP.set(load.map()?);
            let _first = ALTARS.set(load.altars()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Cartographer);

    /// A marker of the scene at `position`, as the loader reads it.
    fn marker(
        fake: enshwick::test::FakeHost,
        position: [f64; 3],
        label: Guid,
        marker_type: Guid,
    ) -> enshwick::test::FakeHost {
        fake.map_marker(MapMarkerInfo {
            position,
            marker_type: marker_type.0,
            label: label.0,
            ..MapMarkerInfo::EMPTY
        })
    }

    fn waypoint(
        fake: enshwick::test::FakeHost,
        entity: u32,
        enabled: u8,
        at: [f64; 3],
    ) -> enshwick::test::FakeHost {
        let mut bytes = vec![enabled, 0, 0, 0, 0, 0, 0, 0];
        bytes.extend(
            enshwick::ecs::fixed(at)
                .iter()
                .flat_map(|axis| axis.to_le_bytes()),
        );
        fake.component(entity, WAYPOINT, &bytes)
    }

    /// The world's own types and the map's markers, as on a build without
    /// waypoints.
    fn markers_only() -> enshwick::test::FakeHost {
        enshwick::test::FakeHost::new()
            .type_hash("current_transform", TRANSFORM)
            .type_hash("used_item", USED_ITEM)
    }

    fn fake() -> enshwick::test::FakeHost {
        markers_only()
            .type_hash("player_waypoint", WAYPOINT)
            .type_size("player_waypoint", 32)
            .constant("player_waypoint.enabled", 0)
            .constant("player_waypoint.waypoint_position", 8)
    }

    fn start(fake: enshwick::test::FakeHost) -> (enshwick::test::Installed, Map) {
        let loader = fake.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Cartographer)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        (loader, *MAP.get().unwrap())
    }

    #[test]
    fn markers_carry_their_type_label_and_place() {
        let fake = marker(fake(), [100.0, 20.0, -50.0], LABEL, TOWN);
        let fake = marker(fake, [-10.0, 0.0, 10.0], Guid([0; 16]), Guid([0; 16]));
        let (_loader, map) = start(fake);

        let markers = map.markers().unwrap();
        assert_eq!(
            markers,
            [
                MapMarker {
                    position: [100.0, 20.0, -50.0],
                    marker_type: Some(MapMarkerTypeId::from_guid(TOWN)),
                    label: Some(LABEL),
                },
                MapMarker {
                    position: [-10.0, 0.0, 10.0],
                    marker_type: None,
                    label: None,
                },
            ]
        );
    }

    #[test]
    fn a_marker_type_is_keyed_as_the_game_keys_its_registry() {
        // FNV-1a 32 over the GUID's bytes in memory order, Data1 to Data3
        // little-endian; over the text order it would be 0xf546_8b09.
        assert_eq!(MapMarkerTypeId::from_guid(TOWN).get(), 0xc807_9fa5);
    }

    #[test]
    fn markers_in_a_box_leave_out_the_rest() {
        let fake = marker(fake(), [100.0, 20.0, -50.0], LABEL, TOWN);
        let fake = marker(fake, [10.0, 5.0, 10.0], LABEL, TOWN);
        let fake = marker(fake, [20.0, 0.0, 0.0], LABEL, TOWN);
        let fake = marker(fake, [20.0, 0.0, 20.5], LABEL, TOWN);
        let (_loader, map) = start(fake);

        let inside = map.markers_in([20.0, 10.0, 20.0], [0.0, 0.0, 0.0]).unwrap();
        let places: Vec<[f64; 3]> = inside.iter().map(|marker| marker.position).collect();
        assert_eq!(places, [[10.0, 5.0, 10.0], [20.0, 0.0, 0.0]]);
        assert_eq!(
            map.markers_in([f64::NAN, 0.0, 0.0], [1.0; 3]),
            Err(WorldError::Refused)
        );
    }

    #[test]
    fn a_waypoint_shows_only_while_enabled() {
        let fake = waypoint(fake(), 40, 1, [12.5, 3.0, -7.25]);
        let fake = waypoint(fake, 41, 0, [1.0, 1.0, 1.0]);
        let (_loader, map) = start(fake);

        assert_eq!(map.waypoint(Entity(40)), Ok(Some([12.5, 3.0, -7.25])));
        assert_eq!(map.waypoint(Entity(41)), Ok(None));
        assert_eq!(map.waypoint(Entity(42)), Ok(None));
    }

    #[test]
    fn a_waypoint_fails_while_no_world_runs() {
        let fake = waypoint(fake(), 40, 1, [12.5, 3.0, -7.25]);
        let (loader, map) = start(fake);
        loader.stop_world();

        assert_eq!(map.waypoint(Entity(40)), Err(WorldError::Missing));
        assert_eq!(map.markers(), Err(WorldError::Missing));
    }

    #[test]
    fn a_waypoint_fails_off_the_game_thread() {
        let fake = waypoint(fake(), 40, 1, [12.5, 3.0, -7.25]);
        let (_loader, map) = start(fake);

        let elsewhere = std::thread::spawn(move || map.waypoint(Entity(40)))
            .join()
            .unwrap();
        assert_eq!(elsewhere, Err(WorldError::WrongThread));
    }

    #[test]
    fn a_build_without_waypoints_keeps_the_markers() {
        let fake = marker(markers_only(), [1.0, 2.0, 3.0], LABEL, TOWN);
        let (_loader, map) = start(fake);

        assert_eq!(map.markers().unwrap().len(), 1);
        assert_eq!(map.waypoint(Entity(40)), Err(WorldError::Missing));
    }

    #[test]
    fn markers_wait_while_the_loader_reads_the_scene() {
        let fake = marker(fake(), [1.0, 2.0, 3.0], LABEL, TOWN);
        let (loader, map) = start(fake);

        loader.read_map_markers(true);
        assert_eq!(map.markers(), Err(WorldError::NotLoaded));
        loader.read_map_markers(false);
        assert_eq!(map.markers().map(|markers| markers.len()), Ok(1));
    }

    #[test]
    fn markers_fail_while_no_world_runs_or_off_the_game_thread() {
        let fake = marker(fake(), [1.0, 2.0, 3.0], LABEL, TOWN);
        let (loader, map) = start(fake);

        let elsewhere = std::thread::spawn(move || map.markers()).join().unwrap();
        assert_eq!(elsewhere, Err(WorldError::WrongThread));
        loader.stop_world();
        assert_eq!(map.markers(), Err(WorldError::Missing));
    }

    #[test]
    fn a_waypoint_field_past_its_type_turns_waypoints_off_alone() {
        let fake = marker(
            fake().type_size("player_waypoint", 0x18),
            [1.0, 2.0, 3.0],
            LABEL,
            TOWN,
        );
        let fake = waypoint(fake, 40, 1, [12.5, 3.0, -7.25]);
        let (_loader, map) = start(fake);

        assert_eq!(map.markers().unwrap().len(), 1);
        assert_eq!(map.waypoint(Entity(40)), Err(WorldError::Missing));
    }

    fn player(slot: u32, entity: u32, name: &str) -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot,
            entity,
            key: u64::from(entity) * 100,
            permissions: 0,
            name_len: u8::try_from(name.len()).unwrap(),
            name: [0; 64],
        };
        info.name[..name.len()].copy_from_slice(name.as_bytes());
        info
    }

    /// An altar at the middle of its zone, whose cells are 0.5 m.
    fn altar(entity: u32, level: u8, min: [i64; 3], max: [i64; 3]) -> AltarInfo {
        let mut info = AltarInfo::EMPTY;
        info.entity = entity;
        info.level = level;
        info.zone = Bounds { min, max };
        let middle = |axis: usize| f64::from(i32::try_from(min[axis] + max[axis]).unwrap()) * 0.25;
        info.position = [middle(0), 0.0, middle(2)];
        info
    }

    #[test]
    fn a_snapshot_holds_players_altars_with_their_bases_and_markers() {
        let fake = marker(fake(), [100.0, 20.0, -50.0], LABEL, TOWN);
        let fake = waypoint(fake, 40, 1, [12.5, 3.0, -7.25])
            .player(player(1, 41, "Ann"))
            .player(player(0, 40, "Bob"));
        let (loader, map) = start(fake);
        loader.set_altars(&[
            altar(300, 2, [0, 0, 0], [10, 10, 10]),
            altar(200, 1, [10, 0, 0], [20, 10, 10]),
            altar(100, 3, [100, 0, 100], [110, 10, 110]),
            altar(400, 0, [0; 3], [0; 3]),
        ]);

        let snapshot = map.snapshot(ALTARS.get().copied()).unwrap();
        assert_eq!(
            snapshot.players,
            [
                MapPlayer {
                    key: 4000,
                    name: "Bob".to_owned(),
                    position: None,
                    waypoint: Some([12.5, 3.0, -7.25])
                },
                MapPlayer {
                    key: 4100,
                    name: "Ann".to_owned(),
                    position: None,
                    waypoint: None
                },
            ]
        );
        let bases: Vec<Option<u32>> = snapshot.altars.iter().map(|altar| altar.base).collect();
        assert_eq!(bases, [Some(0), Some(1), Some(1), None]);
        assert_eq!(
            snapshot.altars[0],
            MapAltar {
                position: [52.5, 0.0, 52.5],
                level: 3,
                zone: [[100, 0, 100], [110, 10, 110]],
                base: Some(0)
            }
        );
        assert_eq!(
            snapshot.markers,
            [MapPoint {
                position: [100.0, 20.0, -50.0],
                marker_type: Some(TOWN.id()),
                label: Some(LABEL.id())
            }]
        );
        let bytes = snapshot.to_bytes().unwrap();
        assert_eq!(MapSnapshot::from_bytes(&bytes).unwrap(), snapshot);
    }

    #[test]
    fn a_snapshot_without_altars_has_none() {
        let (loader, map) = start(fake());
        loader.set_altars(&[altar(300, 2, [0, 0, 0], [10, 10, 10])]);

        assert!(map.snapshot(None).unwrap().altars.is_empty());
    }
}
