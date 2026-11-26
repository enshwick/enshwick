//! Read-only placement metadata through the fake host, not native geometry proof.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::world::World;
    use enshwick::{Entity, ItemId, Load, Mod, Off, WorldError};
    use std::sync::Mutex;

    static WORLD: Mutex<Option<World>> = Mutex::new(None);
    const ITEM: ItemId = ItemId::new(30);
    const TEMPLATE: Guid = Guid([0x3a; 16]);
    struct Probe;
    impl Mod for Probe {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *WORLD.lock().unwrap() = Some(load.world()?);
            Ok(Self)
        }
    }
    enshwick::test::declare_mod!(
        Probe,
        "id = \"test.prop-placement-read\"\nside = \"server\""
    );
    fn start(host: enshwick::test::FakeHost) -> (enshwick::test::Installed, World) {
        let host = host
            .world()
            .item(ITEM, 1, 0)
            .item_places(ITEM, TEMPLATE)
            .install();
        // SAFETY: the exports belong to this test's declared mod.
        let status =
            unsafe { host.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.lock().unwrap().unwrap();
        (host, world)
    }
    #[test]
    fn metadata_preserves_local_corners_unknown_boxes_and_read_only_refusals() {
        let corners = [[-2.0, 0.0, -0.5], [3.0, 4.0, 0.5]];
        {
            let host = enshwick::test::FakeHost::new()
                .placement_box(ITEM, corners[0], corners[1])
                .item(ItemId::new(31), 1, 0)
                .item(ItemId::new(32), 1, 0)
                .item_places(ItemId::new(32), TEMPLATE)
                .item_not_a_prop(ItemId::new(32))
                .prop(Entity::from_engine(40), ITEM, [100.0; 3]);
            let (host, world) = start(host);
            let before = host.props();
            let read = world.prop_placement_info(ITEM).unwrap();
            assert_eq!(read.item, ITEM);
            assert_eq!(read.template, TEMPLATE);
            assert_eq!(read.local_box.map(|b| [b.min, b.max]), Some(corners));
            assert_eq!(world.prop_placement_info(ITEM).unwrap(), read);
            assert_eq!(
                world.prop_placement_info(ItemId::new(31)),
                Err(WorldError::Refused)
            );
            assert_eq!(
                world.prop_placement_info(ItemId::new(32)),
                Err(WorldError::NotAProp)
            );
            assert_eq!(
                world.prop_placement_info(ItemId::new(99)),
                Err(WorldError::Refused)
            );
            assert_eq!(host.props(), before);
            assert!(host.transactions().is_empty());
            host.stop_world();
            assert_eq!(
                world.prop_placement_info(ITEM),
                Err(WorldError::Unavailable)
            );
        }
        {
            let corners = [[3.0, -2.0, 1.0], [-4.0, 5.0, 1.0]];
            let (host, world) = start(
                enshwick::test::FakeHost::new()
                    .without_entity_commands()
                    .placement_box(ITEM, corners[0], corners[1]),
            );
            assert_eq!(
                world
                    .prop_placement_info(ITEM)
                    .unwrap()
                    .local_box
                    .map(|b| [b.min, b.max]),
                Some(corners)
            );
            assert!(host.props().is_empty());
            assert!(host.transactions().is_empty());
        }
        for bounds in [
            None,
            Some([[0.0; 3]; 2]),
            Some([[f32::NAN; 3], [1.0; 3]]),
            Some([[1.0; 3], [-1.0; 3]]),
        ] {
            let host = enshwick::test::FakeHost::new();
            let host = match bounds {
                Some(bounds) => host.placement_box(ITEM, bounds[0], bounds[1]),
                None => host,
            };
            let (host, world) = start(host);
            assert_eq!(world.prop_placement_info(ITEM).unwrap().local_box, None);
            assert!(host.props().is_empty());
            assert!(host.transactions().is_empty());
        }
    }
}
