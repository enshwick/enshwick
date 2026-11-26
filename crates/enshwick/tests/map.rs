//! The map's markers, its frame and each player's waypoint, as the loader
//! answers them.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::Entity;
    use enshwick::WorldError;
    use enshwick::abi::{AltarInfo, Bounds, MapFrameInfo, MapMarkerInfo, PlayerInfo, Status};
    use enshwick::altars::Altars;
    use enshwick::content::{EngineFacts, Guid};
    use enshwick::map::{
        Map, MapAltar, MapFrame, MapMarker, MapMarkerTypeId, MapPlayer, MapPoint, MapSnapshot,
    };
    use enshwick::test::facts::{self, Fact};
    use enshwick::{Load, Mod, Off};

    const TOWN: Guid = Guid::parse("5d2e6a2b-0c3f-4d8e-9a71-3e5f0b1c2d4e");
    const LABEL: Guid = Guid::parse("0b6f4c1a-7e2d-4a90-b3c8-1d2e3f405162");

    // The map and altars of the mod the running test started: installs take
    // turns, so each start sets them afresh.
    static MAP: Mutex<Option<Map>> = Mutex::new(None);
    static ALTARS: Mutex<Option<Altars>> = Mutex::new(None);
    /// Whether the map's reads in init found no world.
    static NO_WORLD_IN_INIT: Mutex<Option<bool>> = Mutex::new(None);

    struct Cartographer;

    impl Mod for Cartographer {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let map = load.map()?;
            *NO_WORLD_IN_INIT.lock().unwrap() = Some(
                map.markers() == Err(WorldError::Unavailable)
                    && map.frame() == Err(WorldError::Unavailable),
            );
            *MAP.lock().unwrap() = Some(map);
            *ALTARS.lock().unwrap() = Some(load.altars()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Cartographer,
        r#"
        id = "you.cartographer"
        side = "server"
        "#
    );

    /// A marker of the scene at `position`, as the loader reads it.
    fn marker(
        fake: enshwick::test::FakeHost,
        position: [f64; 3],
        label: Guid,
        marker_type: Guid,
    ) -> enshwick::test::FakeHost {
        fake.map_marker(enshwick::abi::headed!(MapMarkerInfo {
            position,
            marker_type,
            label,
            ..MapMarkerInfo::EMPTY
        }))
    }

    fn waypoint(
        fake: enshwick::test::FakeHost,
        entity: u32,
        enabled: u8,
        at: [f64; 3],
    ) -> enshwick::test::FakeHost {
        let field = |fact: Fact| fake.offset(fact.id()).unwrap();
        let (shown, place) = (
            field(facts::PLAYER_WAYPOINT_ENABLED),
            field(facts::PLAYER_WAYPOINT_POSITION),
        );
        let mut bytes = vec![0u8; place + 24];
        bytes[shown] = enabled;
        for (axis, value) in enshwick::ecs::fixed(at).iter().enumerate() {
            bytes[place + 8 * axis..][..8].copy_from_slice(&value.to_le_bytes());
        }
        let ty = fake.hash(facts::PLAYER_WAYPOINT.id()).unwrap();
        fake.component(Entity::from_engine(entity), ty, &bytes)
    }

    /// The world's own types and the map's markers, as on a build without
    /// waypoints.
    fn markers_only() -> enshwick::test::FakeHost {
        enshwick::test::FakeHost::new().world()
    }

    fn fake() -> enshwick::test::FakeHost {
        markers_only().map_waypoints()
    }

    fn start(fake: enshwick::test::FakeHost) -> (enshwick::test::Installed, Map) {
        let loader = fake.install();
        *MAP.lock().unwrap() = None;
        *ALTARS.lock().unwrap() = None;
        // SAFETY: both functions are the exports of this file's `declare_mod!(Cartographer)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let map = MAP.lock().unwrap().unwrap();
        assert_eq!(*NO_WORLD_IN_INIT.lock().unwrap(), Some(true));
        // The first call of a world starts the loader's read of the scene;
        // the reads in init, with no world, started none.
        assert_eq!(map.markers(), Err(WorldError::NotLoaded));
        assert_eq!(map.frame(), Err(WorldError::NotLoaded));
        (loader, map)
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
                {
                    let mut value = MapMarker::new([100.0, 20.0, -50.0]);
                    value.marker_type = Some(MapMarkerTypeId::from_guid(TOWN));
                    value.label = Some(LABEL);
                    value
                },
                MapMarker::new([-10.0, 0.0, 10.0]),
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

        assert_eq!(
            map.waypoint(Entity::from_engine(40)),
            Ok(Some([12.5, 3.0, -7.25]))
        );
        assert_eq!(map.waypoint(Entity::from_engine(41)), Ok(None));
        assert_eq!(map.waypoint(Entity::from_engine(42)), Ok(None));
    }

    #[test]
    fn a_waypoint_fails_while_no_world_runs() {
        let fake = waypoint(fake(), 40, 1, [12.5, 3.0, -7.25]);
        let (loader, map) = start(fake);
        loader.stop_world();

        assert_eq!(
            map.waypoint(Entity::from_engine(40)),
            Err(WorldError::Unavailable)
        );
        assert_eq!(map.markers(), Err(WorldError::Unavailable));
    }

    #[test]
    fn a_waypoint_fails_off_the_game_thread() {
        let fake = waypoint(fake(), 40, 1, [12.5, 3.0, -7.25]);
        let (_loader, map) = start(fake);

        let elsewhere = std::thread::spawn(move || map.waypoint(Entity::from_engine(40)))
            .join()
            .unwrap();
        assert_eq!(elsewhere, Err(WorldError::WrongThread));
    }

    #[test]
    fn a_build_without_waypoints_keeps_the_markers() {
        let fake = marker(markers_only(), [1.0, 2.0, 3.0], LABEL, TOWN);
        let (_loader, map) = start(fake);

        assert_eq!(map.markers().unwrap().len(), 1);
        assert_eq!(
            map.waypoint(Entity::from_engine(40)),
            Err(WorldError::Unavailable)
        );
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
        assert_eq!(map.markers(), Err(WorldError::Unavailable));
    }

    #[test]
    fn a_waypoint_field_past_its_type_turns_waypoints_off_alone() {
        let fake = fake();
        let place = fake.offset(facts::PLAYER_WAYPOINT_POSITION.id()).unwrap();
        let short = u32::try_from(place + 16).unwrap();
        let fake = marker(
            fake.type_size(facts::PLAYER_WAYPOINT.id(), short),
            [1.0, 2.0, 3.0],
            LABEL,
            TOWN,
        );
        let fake = waypoint(fake, 40, 1, [12.5, 3.0, -7.25]);
        let (_loader, map) = start(fake);

        assert_eq!(map.markers().unwrap().len(), 1);
        assert_eq!(
            map.waypoint(Entity::from_engine(40)),
            Err(WorldError::Unavailable)
        );
    }

    fn player(slot: u32, entity: u32, name: &str) -> PlayerInfo {
        let mut info = enshwick::abi::headed!(PlayerInfo {
            slot,
            entity: Entity::from_engine(entity),
            key: enshwick::PlayerKey::new(u64::from(entity) * 100),
            permissions: 0,
            name_len: u32::try_from(name.len()).unwrap(),
            name: [0; PlayerInfo::NAME_MAX],
        });
        info.name[..name.len()].copy_from_slice(name.as_bytes());
        info
    }

    /// An altar at the middle of its zone, whose cells are 0.5 m.
    fn altar(entity: u32, level: u8, min: [i64; 3], max: [i64; 3]) -> AltarInfo {
        let mut info = AltarInfo::EMPTY;
        info.entity = Entity::from_engine(entity);
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

        let snapshot = map.snapshot(*ALTARS.lock().unwrap()).unwrap();
        assert_eq!(
            snapshot.players,
            [
                {
                    let mut value =
                        MapPlayer::new(enshwick::PlayerKey::new(4000), "Bob".to_owned());
                    value.waypoint = Some([12.5, 3.0, -7.25]);
                    value
                },
                MapPlayer::new(enshwick::PlayerKey::new(4100), "Ann".to_owned()),
            ]
        );
        let bases: Vec<Option<u32>> = snapshot.altars.iter().map(|altar| altar.base).collect();
        assert_eq!(bases, [Some(0), Some(1), Some(1), None]);
        assert_eq!(snapshot.altars[0], {
            let mut value = MapAltar::new([52.5, 0.0, 52.5], 3, [[100, 0, 100], [110, 10, 110]]);
            value.base = Some(0);
            value
        });
        assert_eq!(
            snapshot.markers,
            [{
                let mut value = MapPoint::new([100.0, 20.0, -50.0]);
                value.marker_type = Some(TOWN.fnv1a());
                value.label = Some(LABEL.fnv1a());
                value
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

    /// The frame of a 10 240 m world of 8 m cells, as the loader reads it.
    const FRAME: MapFrameInfo = enshwick::abi::headed!(MapFrameInfo {
        origin: [-512.0, 256.0],
        size: [10240.0, 10240.0],
        metres_per_cell: 8.0,
        ..MapFrameInfo::EMPTY
    });

    #[test]
    fn a_frame_gives_the_map_s_origin_size_and_cell_in_metres() {
        let (_loader, map) = start(fake().map_frame(FRAME));

        assert_eq!(
            map.frame(),
            Ok(MapFrame::new([-512.0, 256.0], [10240.0, 10240.0], 8.0))
        );
    }

    #[test]
    fn an_uncover_reaches_only_a_client_that_runs_enshwick_and_checks_its_circle() {
        let (with, without) = (enshwick::PlayerKey::new(7), enshwick::PlayerKey::new(8));
        let (loader, map) = start(
            fake()
                .map_frame(FRAME)
                .net_peer(enshwick::net::Peer::Player(with)),
        );

        assert_eq!(map.uncover(with, [1250.0, 0.0, -830.0], 200.0), Ok(()));
        assert_eq!(
            map.uncover(without, [1250.0, 0.0, -830.0], 200.0),
            Err(WorldError::Unavailable),
            "a client without Enshwick is not reached"
        );
        for (at, radius) in [
            ([f64::NAN, 0.0, 0.0], 10.0),
            ([0.0; 3], 0.0),
            ([0.0; 3], 200_000.0),
        ] {
            assert_eq!(map.uncover(with, at, radius), Err(WorldError::Refused));
        }
        assert_eq!(map.uncover_all(with), Ok(()));

        let sent = loader.fog_uncovers();
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!(sent[0], (with, [1250.0, -830.0], 200.0));
        let (_, centre, radius) = sent[1];
        assert_eq!(centre, [4608.0, 5376.0], "the frame's centre");
        assert!(
            (radius - 7240.77).abs() < 0.01,
            "half its diagonal: {radius}"
        );
    }

    #[test]
    fn a_client_that_hosts_uncovers_its_own_player_s_fog_with_no_link() {
        let (host, guest) = (enshwick::PlayerKey::new(7), enshwick::PlayerKey::new(8));
        let (loader, map) = start(
            fake()
                .side(enshwick::abi::Side::CLIENT)
                .hosting(true)
                .host_key(host)
                .map_frame(FRAME),
        );
        assert_eq!(map.uncover_all(host), Ok(()));
        assert_eq!(
            map.uncover(guest, [0.0; 3], 50.0),
            Err(WorldError::Unavailable),
            "a guest without Enshwick is not reached"
        );
        assert_eq!(loader.fog_uncovers().len(), 1);
    }

    #[test]
    fn a_client_in_another_s_game_uncovers_nothing() {
        let key = enshwick::PlayerKey::new(7);
        let (loader, map) = start(
            fake()
                .side(enshwick::abi::Side::CLIENT)
                .net_peer(enshwick::net::Peer::Player(key)),
        );
        assert_eq!(
            map.uncover(key, [0.0; 3], 50.0),
            Err(WorldError::Unavailable)
        );
        assert!(loader.fog_uncovers().is_empty());
    }

    #[test]
    fn a_scene_without_a_frame_answers_missing() {
        let (_loader, map) = start(fake());

        assert_eq!(map.frame(), Err(WorldError::Unavailable));
    }

    #[test]
    fn a_frame_waits_while_the_loader_reads_the_scene() {
        let (loader, map) = start(fake().map_frame(FRAME));

        loader.read_map_markers(true);
        assert_eq!(map.frame(), Err(WorldError::NotLoaded));
        loader.read_map_markers(false);
        assert_eq!(map.frame().map(|frame| frame.metres_per_cell), Ok(8.0));
    }

    #[test]
    fn a_frame_fails_while_no_world_runs_or_off_the_game_thread() {
        let (loader, map) = start(fake().map_frame(FRAME));

        let elsewhere = std::thread::spawn(move || map.frame()).join().unwrap();
        assert_eq!(elsewhere, Err(WorldError::WrongThread));
        loader.stop_world();
        assert_eq!(map.frame(), Err(WorldError::Unavailable));
    }

    #[test]
    fn a_frame_no_map_can_have_is_refused_as_missing() {
        for frame in [
            enshwick::abi::headed!(MapFrameInfo {
                size: [0.0, 10240.0],
                ..FRAME
            }),
            enshwick::abi::headed!(MapFrameInfo {
                metres_per_cell: -8.0,
                ..FRAME
            }),
            enshwick::abi::headed!(MapFrameInfo {
                origin: [f64::NAN, 0.0],
                ..FRAME
            }),
            enshwick::abi::headed!(MapFrameInfo {
                size: [f64::INFINITY, 10240.0],
                ..FRAME
            }),
        ] {
            let (_loader, map) = start(fake().map_frame(frame));
            assert_eq!(map.frame(), Err(WorldError::Unavailable), "{frame:?}");
        }
    }

    #[test]
    fn a_snapshot_carries_the_frame_and_none_without_one() {
        {
            let (_loader, map) = start(fake().map_frame(FRAME));
            assert_eq!(map.snapshot(None).unwrap().frame, map.frame().ok());
        }
        let (_loader, map) = start(fake());
        assert_eq!(map.snapshot(None).unwrap().frame, None);
    }
}
