//! The map's markers and each player's waypoint, as the loader answers them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::ecs::Entity;
    use enshwick::map::{Map, MapMarker, MapMarkerTypeId};
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    const MARKER: u32 = 1;
    const WAYPOINT: u32 = 2;
    const TRANSFORM: u32 = 3;
    const USED_ITEM: u32 = 4;
    const TOWN: Guid = Guid::parse("5d2e6a2b-0c3f-4d8e-9a71-3e5f0b1c2d4e");
    const LABEL: Guid = Guid::parse("0b6f4c1a-7e2d-4a90-b3c8-1d2e3f405162");

    static MAP: OnceLock<Map> = OnceLock::new();

    struct Cartographer;

    impl Mod for Cartographer {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.cartographer", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = MAP.set(load.map()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Cartographer);

    /// A `MapMarker` as the game lays it out: label tag, type reference,
    /// then fields only the client fills.
    fn marker_bytes(label: Guid, marker_type: Guid) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(40);
        bytes.extend_from_slice(&label.0);
        bytes.extend_from_slice(&marker_type.0);
        bytes.extend_from_slice(&[0; 8]);
        bytes
    }

    fn marker(
        fake: enshwick::test::FakeHost,
        entity: u32,
        position: [f64; 3],
        label: Guid,
        marker_type: Guid,
    ) -> enshwick::test::FakeHost {
        fake.placed(MARKER, entity, position).component(
            entity,
            MARKER,
            &marker_bytes(label, marker_type),
        )
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
            .type_hash("map_marker", MARKER)
            .type_size("map_marker", 40)
            .constant("map_marker.marker_id", 0)
            .constant("map_marker.marker_type", 0x10)
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
        let fake = marker(fake(), 7, [100.0, 20.0, -50.0], LABEL, TOWN);
        let fake = marker(fake, 5, [-10.0, 0.0, 10.0], Guid([0; 16]), Guid([0; 16]));
        let (_loader, map) = start(fake);

        let markers = map.markers().unwrap();
        assert_eq!(
            markers,
            [
                MapMarker {
                    entity: Entity(5),
                    position: [-10.0, 0.0, 10.0],
                    marker_type: None,
                    label: None,
                },
                MapMarker {
                    entity: Entity(7),
                    position: [100.0, 20.0, -50.0],
                    marker_type: Some(MapMarkerTypeId::from_guid(TOWN)),
                    label: Some(LABEL),
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
        let fake = marker(fake(), 7, [100.0, 20.0, -50.0], LABEL, TOWN);
        let fake = marker(fake, 8, [10.0, 5.0, 10.0], LABEL, TOWN);
        let fake = marker(fake, 9, [20.0, 0.0, 0.0], LABEL, TOWN);
        let fake = marker(fake, 10, [20.0, 0.0, 20.5], LABEL, TOWN);
        let (_loader, map) = start(fake);

        let inside = map.markers_in([20.0, 10.0, 20.0], [0.0, 0.0, 0.0]).unwrap();
        let entities: Vec<Entity> = inside.iter().map(|marker| marker.entity).collect();
        assert_eq!(entities, [Entity(8), Entity(9)]);
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
        let fake = marker(markers_only(), 7, [1.0, 2.0, 3.0], LABEL, TOWN);
        let (_loader, map) = start(fake);

        assert_eq!(map.markers().unwrap().len(), 1);
        assert_eq!(map.waypoint(Entity(40)), Err(WorldError::Missing));
    }

    #[test]
    fn a_build_without_markers_turns_the_map_off() {
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", TRANSFORM)
            .type_hash("used_item", USED_ITEM)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Cartographer)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(status, Status::OK);
        assert!(MAP.get().is_none());
    }

    #[test]
    fn a_field_past_its_type_turns_the_map_off() {
        let loader = fake().type_size("map_marker", 0x18).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Cartographer)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(status, Status::OK);
        assert!(MAP.get().is_none());
    }
}
