//! Voxels: reads of loaded boxes, as the loader answers them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::WorldError;
    use enshwick::abi::Status;
    use enshwick::voxels::{Voxel, VoxelMaterial, VoxelPos, Voxels};
    use enshwick::{Load, Mod, Off};

    static VOXELS: OnceLock<Voxels> = OnceLock::new();

    struct Surveyor;

    impl Mod for Surveyor {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = VOXELS.set(load.voxels());
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

    const ROCK: Voxel = Voxel::new(VoxelMaterial::Terrain(9), 255);
    const WALL: Voxel = Voxel::new(VoxelMaterial::Building(0x83), 0);
    const AIR: Voxel = Voxel::new(VoxelMaterial::Air, 0);

    /// A loaded box of 2 × 2 × 2 from (10, 20, 30): rock below, air above,
    /// and a wall voxel at its top corner.
    fn installed() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .voxels(
                VoxelPos::new(10, 20, 30),
                [2, 2, 2],
                &[ROCK, ROCK, AIR, AIR, ROCK, ROCK, AIR, WALL],
            )
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Surveyor)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn a_read_returns_the_loaded_voxels_in_place() {
        let _loader = installed();
        let voxels = VOXELS.get().unwrap();

        let block = voxels.read(VoxelPos::new(10, 20, 30), [2, 2, 2]).unwrap();
        assert_eq!(block.size(), [2, 2, 2]);
        assert_eq!(block.get(VoxelPos::new(11, 21, 31)), Some(WALL));
        assert_eq!(block.get(VoxelPos::new(10, 21, 30)), Some(AIR));
        assert_eq!(block.iter().filter(|(_, v)| *v == ROCK).count(), 4);

        let column = voxels.read(VoxelPos::new(11, 20, 31), [1, 2, 1]).unwrap();
        assert_eq!(column.as_slice(), [ROCK, WALL]);
        assert_eq!(voxels.sample(VoxelPos::new(11, 21, 31)), Ok(WALL));
        assert_eq!(
            voxels
                .sample(VoxelPos::new(10, 20, 30))
                .map(Voxel::material),
            Ok(VoxelMaterial::Terrain(9))
        );
    }

    #[test]
    fn a_box_not_wholly_loaded_is_not_loaded_yet() {
        let _loader = installed();
        let voxels = VOXELS.get().unwrap();

        assert_eq!(
            voxels.read(VoxelPos::new(11, 20, 30), [2, 1, 1]),
            Err(WorldError::NotLoaded)
        );
        assert_eq!(
            voxels.sample(VoxelPos::new(0, 0, 0)),
            Err(WorldError::NotLoaded)
        );
    }

    #[test]
    fn an_empty_oversized_or_far_box_is_refused() {
        let _loader = installed();
        let voxels = VOXELS.get().unwrap();
        let corner = VoxelPos::new(10, 20, 30);

        assert_eq!(voxels.read(corner, [0, 1, 1]), Err(WorldError::Refused));
        assert_eq!(voxels.read(corner, [65, 64, 64]), Err(WorldError::Refused));
        assert_eq!(
            voxels.read(corner, [64, 64, 64]),
            Err(WorldError::NotLoaded),
            "the largest box is taken, and found not loaded"
        );
        assert_eq!(
            voxels.read(VoxelPos::new(1 << 30, 0, 0), [1, 1, 1]),
            Err(WorldError::Refused)
        );
        assert_eq!(
            voxels.read(VoxelPos::new(0, 0, 0), [1 << 30, 1 << 30, 16]),
            Err(WorldError::Refused),
            "a count that overflows is refused, not wrapped"
        );
    }

    #[test]
    fn a_read_off_the_game_thread_is_refused() {
        let _loader = installed();
        let voxels = *VOXELS.get().unwrap();

        let elsewhere = std::thread::spawn(move || voxels.sample(VoxelPos::new(10, 20, 30)))
            .join()
            .unwrap();
        assert_eq!(elsewhere, Err(WorldError::WrongThread));
    }

    #[test]
    fn the_fake_refuses_what_the_loader_refuses() {
        let loader = installed();
        // SAFETY: the fake's table stays valid while `loader` is installed.
        let api = unsafe { &*loader.api() };
        let min = [10u32, 20, 30];
        let size = [2u32, 2, 2];
        let mut out = [enshwick::abi::Voxel::default(); 8];

        let short = (api.voxel_read)(
            loader.handle(),
            &raw const min,
            &raw const size,
            out.as_mut_ptr(),
            7,
        );
        assert_eq!(short, Status::BAD_ARGS);
        let null = (api.voxel_read)(
            loader.handle(),
            core::ptr::null(),
            &raw const size,
            out.as_mut_ptr(),
            8,
        );
        assert_eq!(null, Status::BAD_ARGS);
        let read = (api.voxel_read)(
            loader.handle(),
            &raw const min,
            &raw const size,
            out.as_mut_ptr(),
            8,
        );
        assert_eq!(read, Status::OK);
        assert_eq!(out[7].kind, enshwick::abi::Voxel::BUILDING);
    }

    #[test]
    fn a_box_past_the_world_s_edge_is_outside_the_world() {
        let loader = enshwick::test::FakeHost::new()
            .world_extent([40, 40, 40])
            .voxels(VoxelPos::new(32, 32, 32), [8, 8, 8], &[AIR; 512])
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Surveyor)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let voxels = VOXELS.get().unwrap();

        assert!(voxels.read(VoxelPos::new(32, 32, 32), [8, 8, 8]).is_ok());
        assert_eq!(
            voxels.read(VoxelPos::new(36, 32, 32), [8, 1, 1]).err(),
            Some(WorldError::OutsideWorld)
        );
        assert_eq!(
            voxels.sample(VoxelPos::new(32, 40, 32)),
            Err(WorldError::OutsideWorld)
        );
        assert_eq!(
            voxels.sample(VoxelPos::new(0, 0, 100)),
            Err(WorldError::OutsideWorld),
            "outside comes before not loaded"
        );
    }
}
