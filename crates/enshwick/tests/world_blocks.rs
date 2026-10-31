//! Blocks and voxels in transactions, and transactions for a player, as the loader holds them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::abi::{Header, PlayerInfo, Status, TerrainMaterialInfo, TransactionInfo};
    use enshwick::test::FakeChange;
    use enshwick::voxels::{Voxel, VoxelBox, VoxelMaterial, VoxelPos};
    use enshwick::world::{
        BuildRefusal, Player, PlayerPermissions, TransactionStatus, World, WorldTransform,
    };
    use enshwick::{ItemId, Load, Mod, Off, WorldError};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Mason;

    impl Mod for Mason {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Mason,
        r#"
        id = "you.mason"
        side = "server"
        "#
    );

    const SHAPE: ItemId = ItemId::new(10);
    const STONE: ItemId = ItemId::new(20);
    const DIRT: ItemId = ItemId::new(21);
    const WORLDER: u32 = 2;
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
        WORLD.get().unwrap().player(Entity(entity)).unwrap()
    }

    fn started() -> (enshwick::test::Installed, World) {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player(info(WORLDER, PlayerInfo::CAN_EDIT_WORLD))
            .player(info(GUEST, PlayerInfo::CAN_ACCESS_INVENTORIES))
            .building_area([0.0; 3], [1000.0; 3])
            .blueprint(SHAPE, [4, 4, 4])
            .building_material(STONE, 0x83)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Mason)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader.set_terrain(&[TerrainMaterialInfo {
            header: Header::of::<TerrainMaterialInfo>(),
            id: 9,
            flags: 0,
            hardness: 3,
            reserved: 0,
            item: DIRT,
            health: 500,
        }]);
        (loader, *WORLD.get().unwrap())
    }

    fn stone() -> Voxel {
        Voxel::new(VoxelMaterial::Building(0x83), 0)
    }

    #[test]
    fn voxels_erases_and_blocks_reach_the_loader_in_order() {
        let (loader, world) = started();
        let wall = VoxelBox::new(VoxelPos::new(200, 80, 200), [2, 1, 1], vec![stone(); 2]).unwrap();
        let at = WorldTransform::at([100.0, 40.0, 100.0]);
        let id = world
            .transaction()
            .write_voxels(&wall)
            .erase_block(VoxelPos::new(210, 80, 200), [4, 4, 4])
            .place_block(SHAPE, STONE, at)
            .commit()
            .unwrap();

        let held = loader.transactions();
        assert_eq!(held.len(), 1);
        match held[0].changes.as_slice() {
            [
                FakeChange::Voxels { min, size, voxels },
                FakeChange::Erase {
                    min: erased,
                    size: erased_size,
                },
                FakeChange::Block(block),
            ] => {
                assert_eq!((*min, *size), ([200, 80, 200], [2, 1, 1]));
                assert_eq!(voxels.len(), 2);
                assert_eq!(
                    (voxels[0].kind, voxels[0].material),
                    (enshwick::abi::Voxel::BUILDING, 0x83)
                );
                assert_eq!((*erased, *erased_size), ([210, 80, 200], [4, 4, 4]));
                assert_eq!((block.shape, block.material), (SHAPE, STONE));
                assert_eq!(block.position, at.position);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(held[0].actor, 0, "the mod's own change");
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Applied)
        );
    }

    #[test]
    fn a_block_needs_a_known_shape_and_a_building_material() {
        let (loader, world) = started();
        let at = WorldTransform::at([100.0, 40.0, 100.0]);
        for (shape, material) in [
            (ItemId::new(99), STONE),
            (SHAPE, DIRT),
            (SHAPE, ItemId::new(98)),
        ] {
            assert_eq!(
                world
                    .transaction()
                    .place_block(shape, material, at)
                    .commit(),
                Err(WorldError::Refused),
                "{shape:?} of {material:?}"
            );
        }
        let skewed = WorldTransform {
            orientation: [f32::NAN, 0.0, 0.0, 1.0],
            ..at
        };
        assert_eq!(
            world
                .transaction()
                .place_block(SHAPE, STONE, skewed)
                .commit(),
            Err(WorldError::Refused)
        );
        assert!(loader.transactions().is_empty());
    }

    #[test]
    fn a_voxel_box_holds_exactly_its_voxels() {
        let min = VoxelPos::new(1, 2, 3);
        assert!(VoxelBox::new(min, [2, 2, 2], vec![stone(); 7]).is_none());
        assert!(VoxelBox::new(min, [0, 2, 2], Vec::new()).is_none());
        let mut block = VoxelBox::new(min, [2, 2, 2], vec![Voxel::default(); 8]).unwrap();
        assert!(block.set(VoxelPos::new(2, 3, 4), stone()));
        assert!(
            !block.set(VoxelPos::new(3, 3, 4), stone()),
            "outside the box"
        );
        assert_eq!(block.get(VoxelPos::new(2, 3, 4)), Some(stone()));
        let moved = block.moved_to(VoxelPos::new(11, 12, 13));
        assert_eq!(moved.get(VoxelPos::new(12, 13, 14)), Some(stone()));
        assert_eq!(moved.size(), [2, 2, 2]);
    }

    #[test]
    fn a_player_the_server_refuses_gets_nothing_written_and_the_status_says_why() {
        let (loader, world) = started();
        let id = world
            .transaction_for(&player(GUEST))
            .erase_block(VoxelPos::new(200, 80, 200), [4, 4, 4])
            .commit()
            .unwrap();
        assert_eq!(loader.transactions()[0].actor, GUEST);
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::NotAllowed(BuildRefusal::NoPermission(
                PlayerPermissions::CanEditWorld
            )))
        );
        assert_eq!(loader.transactions()[0].info.state, TransactionInfo::FAILED);

        let allowed = world
            .transaction_for(&player(WORLDER))
            .erase_block(VoxelPos::new(200, 80, 200), [4, 4, 4])
            .commit()
            .unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(allowed),
            Some(TransactionStatus::Applied)
        );
    }

    #[test]
    fn a_commit_for_no_character_is_refused() {
        let (loader, _world) = started();
        let region = enshwick::abi::VoxelRegion {
            header: Header::of::<enshwick::abi::VoxelRegion>(),
            min: [200, 80, 200],
            size: [4, 4, 4],
        };
        let op = enshwick::abi::TransactionOp {
            kind: enshwick::abi::TransactionOp::ERASE,
            reserved: 0,
            data: (&raw const region).cast(),
        };
        let mut id = 0u64;
        // SAFETY: `api` points at the fake's table, which lives for the
        // process.
        let api = unsafe { &*loader.api() };
        let status = (api.transaction_commit_for)(
            enshwick::abi::ModHandle(1),
            Entity(0),
            &raw const op,
            1,
            &raw mut id,
        );
        assert_eq!(status, Status::BAD_ARGS);
    }

    #[test]
    fn the_fake_refuses_voxels_whose_kind_is_not_their_material_s() {
        use enshwick::abi::{TransactionOp, Voxel as Raw, VoxelWrite};

        let (loader, _world) = started();
        // SAFETY: `api` points at the fake's table, which lives for the
        // process.
        let api = unsafe { &*loader.api() };
        let raw = |kind, material, reserved| Raw {
            kind,
            material,
            fill: 0,
            reserved,
        };
        let commit = |voxel: Raw| {
            let voxels = [voxel];
            let write = VoxelWrite {
                header: Header::of::<VoxelWrite>(),
                min: [200, 80, 200],
                size: [1, 1, 1],
                voxels: voxels.as_ptr(),
                count: 1,
            };
            let op = TransactionOp {
                kind: TransactionOp::VOXELS,
                reserved: 0,
                data: (&raw const write).cast(),
            };
            let mut id = 0u64;
            (api.transaction_commit)(enshwick::abi::ModHandle(1), &raw const op, 1, &raw mut id)
        };
        for good in [
            raw(Raw::AIR, 0, 0),
            raw(Raw::TERRAIN, 1, 0),
            raw(Raw::TERRAIN, 0x7f, 0),
            raw(Raw::BUILDING, 0x80, 0),
        ] {
            assert_eq!(commit(good), Status::OK, "{good:?}");
        }
        for bad in [
            raw(Raw::AIR, 5, 0),
            raw(Raw::TERRAIN, 0, 0),
            raw(Raw::TERRAIN, 0x80, 0),
            raw(Raw::BUILDING, 0x7f, 0),
            raw(3, 0, 0),
            raw(Raw::AIR, 0, 1),
        ] {
            assert_eq!(commit(bad), Status::BAD_ARGS, "{bad:?}");
        }
    }
}
