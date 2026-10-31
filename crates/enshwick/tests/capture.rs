//! Props in a box and captures of a region, as the loader answers them.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::__private::ids;
    use enshwick::Entity;
    use enshwick::ItemId;
    use enshwick::abi::Status;
    use enshwick::content::EngineFacts;
    use enshwick::voxels::{Voxel, VoxelMaterial, VoxelPos};
    use enshwick::world::{CAPTURE_TILES, Capture, CapturedProp, Prop, World, WorldTransform};
    use enshwick::{Load, Mod, Off, WorldError};

    const BED: u32 = 3;
    const ANIMAL: u32 = 4;
    const SURVIVOR: u32 = 5;
    const ALTAR: u32 = 6;
    const TURNED: [f32; 4] = [
        0.0,
        core::f32::consts::FRAC_1_SQRT_2,
        0.0,
        core::f32::consts::FRAC_1_SQRT_2,
    ];
    const STILL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    /// The world of the mod the running test started: installs take turns,
    /// so each start sets it afresh.
    static WORLD: Mutex<Option<World>> = Mutex::new(None);

    struct Surveyor;

    impl Mod for Surveyor {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *WORLD.lock().unwrap() = Some(load.world()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Surveyor,
        r#"
        id = "you.surveyor"
        side = "server"
        "#
    );

    /// The template a player places from `item`.
    fn template_of(item: u32) -> [u8; 16] {
        let mut guid = [0x5a; 16];
        guid[..4].copy_from_slice(&item.to_le_bytes());
        guid
    }

    /// A placed prop through the fake's entity builders: its `UsedItem`,
    /// a `CurrentTransform` holding the position, then the orientation, and
    /// the template its item places.
    fn prop(
        fake: enshwick::test::FakeHost,
        entity: u32,
        item: u32,
        position: [f64; 3],
        orientation: [f32; 4],
    ) -> enshwick::test::FakeHost {
        thing(fake, entity, item, position, orientation)
            .item(ItemId::new(item), 1, 0)
            .item_places(ItemId::new(item), template_of(item))
            .template(Entity(entity), template_of(item))
    }

    /// An entity with a `UsedItem` and a `CurrentTransform`, as a prop has
    /// and some things a world comes with have too.
    fn thing(
        fake: enshwick::test::FakeHost,
        entity: u32,
        item: u32,
        position: [f64; 3],
        orientation: [f32; 4],
    ) -> enshwick::test::FakeHost {
        let mut transform: Vec<u8> = enshwick::ecs::fixed(position)
            .iter()
            .flat_map(|axis| axis.to_le_bytes())
            .collect();
        transform.extend(orientation.iter().flat_map(|part| part.to_le_bytes()));
        let world = enshwick::test::FakeHost::new().world();
        let used_item = world.hash(ids::USED_ITEM).unwrap();
        let current = world.hash(ids::CURRENT_TRANSFORM).unwrap();
        fake.placed(used_item, Entity(entity), position)
            .placed(current, Entity(entity), position)
            .component(Entity(entity), used_item, &item.to_le_bytes())
            .component(Entity(entity), current, &transform)
    }

    fn fake() -> enshwick::test::FakeHost {
        enshwick::test::FakeHost::new()
            .world()
            .type_hash("animal_saved", ANIMAL)
            .type_hash("active_npc_state_saved", SURVIVOR)
            .type_hash("base_id_component", ALTAR)
            .constant("current_transform.transform", 0)
            .constant("world_transform.orientation", 24)
            .constant("item_info.placed_entity", 0x3c0)
    }

    fn start(fake: enshwick::test::FakeHost) -> (enshwick::test::Installed, World) {
        let loader = fake.install();
        *WORLD.lock().unwrap() = None;
        // SAFETY: both functions are the exports of this file's `declare_mod!(Surveyor)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.lock().unwrap().take().unwrap();
        (loader, world)
    }

    fn rock(i: usize) -> Voxel {
        Voxel::new(
            VoxelMaterial::Terrain(u8::try_from(i % 100).unwrap() + 1),
            255,
        )
    }

    #[test]
    fn props_in_a_box_carry_their_item_and_transform() {
        let fake = fake().placed(BED, Entity(9), [11.0, 1.0, 11.0]);
        let fake = prop(fake, 10, 1_042, [11.0, 1.0, 12.0], TURNED);
        let fake = prop(fake, 11, 77, [10.0, 0.0, 10.0], STILL);
        let fake = prop(fake, 12, 77, [30.0, 0.0, 10.0], STILL);
        let fake = thing(fake, 13, 77, [12.0, 0.0, 12.0], STILL).template(Entity(13), [0x11; 16]);
        let (_loader, world) = start(fake);

        let props = world
            .props_in([20.0, 4.0, 20.0], [10.0, 0.0, 10.0])
            .unwrap();
        assert_eq!(
            props,
            [
                Prop {
                    entity: Entity(10),
                    item: ItemId::new(1_042),
                    transform: WorldTransform {
                        position: [11.0, 1.0, 12.0],
                        orientation: TURNED,
                    },
                },
                Prop {
                    entity: Entity(11),
                    item: ItemId::new(77),
                    transform: WorldTransform {
                        position: [10.0, 0.0, 10.0],
                        orientation: STILL,
                    },
                },
            ]
        );
        assert_eq!(
            world.props_in([f64::NAN, 0.0, 0.0], [1.0; 3]),
            Err(WorldError::Refused)
        );
    }

    #[test]
    fn creatures_and_flame_altars_are_no_props() {
        let fake = prop(fake(), 10, 50, [10.0; 3], STILL);
        let fake = prop(fake, 11, 51, [11.0; 3], STILL).component(Entity(11), ANIMAL, &[0; 4]);
        let fake = prop(fake, 12, 52, [12.0; 3], STILL).component(Entity(12), SURVIVOR, &[0; 4]);
        let fake = prop(fake, 13, 53, [13.0; 3], STILL).component(Entity(13), ALTAR, &[0; 4]);
        let (_loader, world) = start(fake);
        let props = world.props_in([0.0; 3], [20.0; 3]).unwrap();
        let entities: Vec<Entity> = props.iter().map(|prop| prop.entity).collect();
        assert_eq!(entities, [Entity(10)]);
    }

    #[test]
    fn props_are_missing_on_a_build_without_the_creature_marks() {
        let fake = enshwick::test::FakeHost::new()
            .world()
            .constant("current_transform.transform", 0)
            .constant("world_transform.orientation", 24)
            .constant("item_info.placed_entity", 0x3c0);
        let (_loader, world) = start(prop(fake, 10, 50, [10.0; 3], STILL));
        assert_eq!(
            world.props_in([0.0; 3], [20.0; 3]),
            Err(WorldError::Missing)
        );
    }

    #[test]
    fn props_off_the_game_thread_are_refused() {
        let (_loader, world) = start(prop(fake(), 10, 1, [1.0; 3], STILL));

        let elsewhere = std::thread::spawn(move || world.props_in([0.0; 3], [2.0; 3]))
            .join()
            .unwrap();
        assert_eq!(elsewhere, Err(WorldError::WrongThread));
    }

    #[test]
    fn props_are_missing_on_a_build_without_the_orientation() {
        let fake = enshwick::test::FakeHost::new().world();
        let (_loader, world) = start(prop(fake, 10, 1, [1.0; 3], STILL));

        assert_eq!(world.props_in([0.0; 3], [2.0; 3]), Err(WorldError::Missing));
    }

    #[test]
    fn props_are_missing_on_a_build_without_the_placed_template() {
        let fake = enshwick::test::FakeHost::new()
            .world()
            .constant("current_transform.transform", 0)
            .constant("world_transform.orientation", 24);
        let (_loader, world) = start(prop(fake, 10, 1, [1.0; 3], STILL));

        assert_eq!(world.props_in([0.0; 3], [2.0; 3]), Err(WorldError::Missing));
    }

    #[test]
    fn props_are_missing_before_the_game_knows_its_items() {
        let fake = thing(fake(), 10, 1, [1.0; 3], STILL).template(Entity(10), template_of(1));
        let (_loader, world) = start(fake);

        assert_eq!(world.props_in([0.0; 3], [2.0; 3]), Err(WorldError::Missing));
        assert_eq!(world.props_in([5.0; 3], [6.0; 3]), Ok(Vec::new()));
    }

    #[test]
    fn a_capture_tiles_reads_and_holds_the_region_from_its_corner() {
        let min = VoxelPos::new(100, 10, 200);
        let size = [70u32, 2, 3];
        let voxels: Vec<Voxel> = (0..70 * 2 * 3).map(rock).collect();
        let fake = fake().voxels(min, size, &voxels);
        let fake = prop(fake, 10, 5, [50.25, 5.5, 101.0], TURNED);
        let fake = prop(fake, 11, 6, [85.0, 5.0, 101.0], STILL);
        let (_loader, world) = start(fake);

        let far = VoxelPos::new(169, 11, 202);
        let capture = world.capture(far, min).unwrap();
        assert_eq!(capture.origin(), min);
        assert_eq!(capture.size(), size);
        assert_eq!(capture.voxels().min(), VoxelPos::new(0, 0, 0));
        assert_eq!(capture.voxels().as_slice(), voxels.as_slice());
        assert_eq!(
            capture.props(),
            [CapturedProp {
                item: ItemId::new(5),
                transform: WorldTransform {
                    position: [0.25, 0.5, 1.0],
                    orientation: TURNED,
                },
            }],
            "a prop on the far face lies outside the region"
        );
    }

    #[test]
    fn a_capture_passes_not_loaded_and_outside_the_world_on() {
        let (_loader, world) = start(fake().world_extent([400, 400, 400]).voxels(
            VoxelPos::new(0, 0, 0),
            [64, 1, 1],
            &[Voxel::default(); 64],
        ));

        assert_eq!(
            world
                .capture(VoxelPos::new(0, 0, 0), VoxelPos::new(70, 0, 0))
                .err(),
            Some(WorldError::NotLoaded)
        );
        assert_eq!(
            world
                .capture(VoxelPos::new(390, 0, 0), VoxelPos::new(410, 0, 0))
                .err(),
            Some(WorldError::OutsideWorld)
        );
    }

    #[test]
    fn a_capture_reads_every_tile_after_one_not_loaded() {
        let (loader, world) =
            start(fake().voxels(VoxelPos::new(64, 0, 0), [6, 1, 1], &[Voxel::default(); 6]));

        assert_eq!(
            world
                .capture(VoxelPos::new(0, 0, 0), VoxelPos::new(69, 0, 0))
                .err(),
            Some(WorldError::NotLoaded)
        );
        assert_eq!(
            loader.voxel_reads(),
            [([0, 0, 0], [64, 1, 1]), ([64, 0, 0], [6, 1, 1])]
        );
    }

    #[test]
    fn a_capture_takes_at_most_its_tiles_however_thin() {
        let (loader, world) = start(fake());
        let tiles = 64 * CAPTURE_TILES;

        assert_eq!(
            world
                .capture(VoxelPos::new(0, 0, 0), VoxelPos::new(8191, 0, 8191))
                .err(),
            Some(WorldError::Refused),
            "a thin region of many tiles"
        );
        assert_eq!(
            world
                .capture(VoxelPos::new(0, 0, 0), VoxelPos::new(tiles, 0, 0))
                .err(),
            Some(WorldError::Refused),
            "one tile past the most"
        );
        assert!(
            loader.voxel_reads().is_empty(),
            "a refused capture reads nothing"
        );
        assert_eq!(
            world
                .capture(VoxelPos::new(0, 0, 0), VoxelPos::new(tiles - 1, 0, 0))
                .err(),
            Some(WorldError::NotLoaded)
        );
        assert_eq!(loader.voxel_reads().len(), CAPTURE_TILES as usize);
    }

    #[test]
    fn a_capture_survives_a_round_trip_through_serde() {
        let min = VoxelPos::new(4, 4, 4);
        let voxels: Vec<Voxel> = (0..8).map(rock).collect();
        let fake = prop(
            fake().voxels(min, [2, 2, 2], &voxels),
            10,
            5,
            [2.5; 3],
            TURNED,
        );
        let (_loader, world) = start(fake);

        let capture = world.capture(min, VoxelPos::new(5, 5, 5)).unwrap();
        let bytes = postcard::to_allocvec(&capture).unwrap();
        let back: Capture = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, capture);
        assert_eq!(back.props().len(), 1);
    }
}
