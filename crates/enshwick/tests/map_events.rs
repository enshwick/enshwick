//! Waypoint and ping events, as the loader's reads and game events raise
//! them.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use enshwick::abi::{Header, MapFrameInfo, PlayerInfo, Status};
    use enshwick::content::EngineFacts;
    use enshwick::map::{MapPinged, WaypointChanged};
    use enshwick::test::facts::{self, Fact};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    static WAYPOINTS: Mutex<Vec<WaypointChanged>> = Mutex::new(Vec::new());
    static PINGS: Mutex<Vec<MapPinged>> = Mutex::new(Vec::new());

    struct Watcher;

    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_waypoint(|change| WAYPOINTS.lock().unwrap().push(*change))?;
            load.on_ping(|ping| PINGS.lock().unwrap().push(*ping))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Watcher,
        r#"
        id = "you.watcher"
        side = "server"
        "#
    );

    const ANN: Entity = Entity(40);

    fn ann() -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: ANN,
            key: PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    /// A `PlayerWaypoint`'s bytes, on or off at `at`.
    fn waypoint(fake: &enshwick::test::FakeHost, enabled: u8, at: [f64; 3]) -> Vec<u8> {
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
        bytes
    }

    fn frame() -> MapFrameInfo {
        MapFrameInfo {
            origin: [-1000.0; 2],
            size: [2000.0; 2],
            metres_per_cell: 8.0,
            ..MapFrameInfo::EMPTY
        }
    }

    #[test]
    fn a_waypoint_set_moved_and_cleared_raises_one_change_each() {
        let fake = enshwick::test::FakeHost::new()
            .world()
            .map_waypoints()
            .effect_events()
            .map_frame(frame())
            .player(ann());
        let ty = fake.hash(facts::PLAYER_WAYPOINT.id()).unwrap();
        let off = waypoint(&fake, 0, [0.0; 3]);
        let set = waypoint(&fake, 1, [12.5, 3.0, -7.25]);
        let moved = waypoint(&fake, 1, [20.0, 3.0, -7.25]);
        let outside = waypoint(&fake, 1, [5000.0, 0.0, 0.0]);
        let loader = fake.component(ANN, ty, &off).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let step = || {
            let _ran = loader.tick(Duration::from_millis(100));
        };
        step();
        step();
        assert!(WAYPOINTS.lock().unwrap().is_empty(), "joining is no change");
        for bytes in [&set, &set, &moved, &outside, &off] {
            assert!(loader.set_component(ANN, ty, bytes));
            step();
        }
        let positions: Vec<Option<[f64; 3]>> = WAYPOINTS
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.position)
            .collect();
        assert_eq!(
            positions,
            [Some([12.5, 3.0, -7.25]), Some([20.0, 3.0, -7.25]), None],
            "a waypoint outside the map is left out"
        );
        assert!(
            WAYPOINTS
                .lock()
                .unwrap()
                .iter()
                .all(|c| c.player == PlayerKey::new(7) && c.entity == ANN)
        );
    }

    #[test]
    fn a_ping_names_its_player_and_place_and_a_sender_is_heard_once_a_tenth_of_a_second() {
        let fake = enshwick::test::FakeHost::new()
            .world()
            .map_waypoints()
            .effect_events()
            .map_frame(frame())
            .player(ann());
        let ty = fake.hash(facts::UI_PING_EVENT.id()).unwrap();
        let size = fake.size(facts::UI_PING_EVENT.id()).unwrap();
        let sender = fake
            .offset(facts::UI_PING_EVENT_SENDER_ENTITY_ID.id())
            .unwrap();
        let place = fake.offset(facts::UI_PING_EVENT_POSITION.id()).unwrap();
        let ping = |from: Entity, at: [f64; 3]| {
            let mut bytes = vec![0u8; size];
            bytes[sender..][..4].copy_from_slice(&from.0.to_le_bytes());
            for (axis, value) in enshwick::ecs::fixed(at).iter().enumerate() {
                bytes[place + 8 * axis..][..8].copy_from_slice(&value.to_le_bytes());
            }
            bytes
        };
        let loader = fake.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.raise_game_event(ty, &ping(ANN, [1.0, 2.0, 3.0])), 1);
        let _again = loader.raise_game_event(ty, &ping(ANN, [4.0, 2.0, 3.0]));
        let _far = loader.raise_game_event(ty, &ping(Entity(5), [9000.0, 0.0, 0.0]));
        let _other = loader.raise_game_event(ty, &ping(Entity(5), [-5.0, 0.0, 6.0]));
        let pings: Vec<(Option<PlayerKey>, Entity, [f64; 3])> = PINGS
            .lock()
            .unwrap()
            .iter()
            .map(|p| (p.player, p.entity, p.position))
            .collect();
        assert_eq!(
            pings,
            [
                (Some(PlayerKey::new(7)), ANN, [1.0, 2.0, 3.0]),
                (None, Entity(5), [-5.0, 0.0, 6.0]),
            ]
        );
    }
}
