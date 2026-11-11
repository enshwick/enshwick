//! `enshwick.map`: the pings and waypoints a script hears, as a Rust mod's.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::abi::{Header, MapFrameInfo, PlayerInfo};
    use enshwick::content::EngineFacts;
    use enshwick::test::{FakeHost, facts};
    use enshwick::{Entity, PlayerKey};

    use crate::harness::{logged, run};

    const ANN: Entity = Entity(40);

    fn fake() -> FakeHost {
        FakeHost::new()
            .world()
            .map_waypoints()
            .effect_events()
            .map_frame(MapFrameInfo {
                origin: [-1000.0; 2],
                size: [2000.0; 2],
                metres_per_cell: 8.0,
                ..MapFrameInfo::EMPTY
            })
            .player(PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: 0,
                entity: ANN,
                key: PlayerKey::new(7),
                permissions: 0,
                name_len: 0,
                name: [0; 64],
            })
    }

    #[test]
    fn a_script_hears_a_ping_and_a_waypoint_set_then_cleared() {
        let fake = fake();
        let ping_ty = fake.hash(facts::UI_PING_EVENT.id()).unwrap();
        let ping_size = fake.size(facts::UI_PING_EVENT.id()).unwrap();
        let sender = fake
            .offset(facts::UI_PING_EVENT_SENDER_ENTITY_ID.id())
            .unwrap();
        let ping_at = fake.offset(facts::UI_PING_EVENT_POSITION.id()).unwrap();
        let way_ty = fake.hash(facts::PLAYER_WAYPOINT.id()).unwrap();
        let shown = fake.offset(facts::PLAYER_WAYPOINT_ENABLED.id()).unwrap();
        let place = fake.offset(facts::PLAYER_WAYPOINT_POSITION.id()).unwrap();
        let waypoint = |enabled: u8, at: [f64; 3]| {
            let mut bytes = vec![0u8; place + 24];
            bytes[shown] = enabled;
            for (axis, value) in enshwick::ecs::fixed(at).iter().enumerate() {
                bytes[place + 8 * axis..][..8].copy_from_slice(&value.to_le_bytes());
            }
            bytes
        };
        let loader = run(
            fake.component(ANN, way_ty, &waypoint(0, [0.0; 3])),
            r"
            enshwick.map.on_ping(function(ping)
                enshwick.log.info(`ping {ping.entity} {ping.position.x}`)
            end)
            enshwick.map.on_waypoint(function(change)
                local at = if change.position then change.position.x else 'none'
                enshwick.log.info(`waypoint {change.entity} {at}`)
            end)
            ",
        );
        let mut ping = vec![0u8; ping_size];
        ping[sender..][..4].copy_from_slice(&ANN.0.to_le_bytes());
        for (axis, value) in enshwick::ecs::fixed([3.0, 2.0, 1.0]).iter().enumerate() {
            ping[ping_at + 8 * axis..][..8].copy_from_slice(&value.to_le_bytes());
        }
        let _heard = loader.raise_game_event(ping_ty, &ping);
        let step = || {
            let _ran = loader.tick(Duration::from_millis(100));
        };
        step();
        assert!(loader.set_component(ANN, way_ty, &waypoint(1, [12.5, 0.0, 0.0])));
        step();
        assert!(loader.set_component(ANN, way_ty, &waypoint(0, [0.0; 3])));
        step();
        for line in ["ping 40 3", "waypoint 40 12.5", "waypoint 40 none"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_hears_fog_its_player_uncovered() {
        let fog = |window| enshwick::abi::FogInfo {
            width: 4,
            height: 3,
            metres_per_pixel: 8.0,
            size_z: 24.0,
            session: 1,
            uncovered: window,
            uncovered_pixels: 2,
            uncovered_min: [1, 1],
            uncovered_size: [2, 1],
            ..enshwick::abi::FogInfo::EMPTY
        };
        let loader = run(
            FakeHost::new()
                .side(enshwick::abi::Side::CLIENT)
                .fog(fog(1), vec![0; 12]),
            r"
            enshwick.map.on_fog_uncovered(function(new)
                enshwick.log.info(`uncovered {new.pixels} from {new.min[1]},{new.min[2]} to {new.max[1]},{new.max[2]}`)
            end)
            ",
        );
        let _first_look = loader.draw_frame();
        loader.set_fog(fog(2), vec![0; 12]);
        std::thread::sleep(Duration::from_millis(260));
        let _new_window = loader.draw_frame();
        assert!(
            logged(&loader, "uncovered 2 from 8,8 to 24,16"),
            "{:?}",
            loader.logs()
        );
    }
}
