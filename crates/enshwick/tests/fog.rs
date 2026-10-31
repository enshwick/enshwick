//! The client's fog of war, as the loader copies it.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{FogInfo, Side, Status};
    use enshwick::map::{FogError, FogOfWar};
    use enshwick::{Load, Mod, Off};

    /// The fog of the mod the running test started: installs take turns,
    /// so each start sets it afresh.
    static FOG: Mutex<Option<FogOfWar>> = Mutex::new(None);

    struct Minimap;

    impl Mod for Minimap {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *FOG.lock().unwrap() = Some(load.fog_of_war()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Minimap,
        r#"
        id = "you.minimap"
        side = "client"
        "#
    );

    /// A fog of 4 × 3 pixels of 8 m, its pixel at (x, y) holding
    /// `10 * y + x`.
    fn small() -> (FogInfo, Vec<u8>) {
        let info = FogInfo {
            width: 4,
            height: 3,
            metres_per_pixel: 8.0,
            size_z: 24.0,
            version: 5,
            session: 1,
            ..FogInfo::EMPTY
        };
        let pixels = (0..3u8)
            .flat_map(|y| (0..4u8).map(move |x| 10 * y + x))
            .collect();
        (info, pixels)
    }

    /// The started loader, its status and the fog the mod got, if any.
    fn start(
        fake: enshwick::test::FakeHost,
    ) -> (enshwick::test::Installed, Status, Option<FogOfWar>) {
        let loader = fake.install();
        *FOG.lock().unwrap() = None;
        // SAFETY: both functions are the exports of this file's `declare_mod!(Minimap)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        let fog = FOG.lock().unwrap().take();
        (loader, status, fog)
    }

    #[test]
    fn a_box_of_the_fog_reads_row_by_row() {
        let (info, pixels) = small();
        let (_loader, status, fog) = start(
            enshwick::test::FakeHost::new()
                .side(Side::CLIENT)
                .fog(info, pixels),
        );
        assert_eq!(status, Status::OK);
        let fog = fog.unwrap();

        let state = fog.state().unwrap();
        assert_eq!((state.width, state.height, state.version), (4, 3, 5));
        let image = fog.read([1, 1], [2, 2]).unwrap();
        assert_eq!(image.coverage(), [11, 12, 21, 22]);
        assert_eq!(image.at([2, 2]), Some(22));
        assert_eq!(image.at([0, 0]), None);
        assert_eq!(fog.read([3, 0], [2, 1]), Err(FogError::Outside));
    }

    #[test]
    fn coverage_flips_z_against_the_world_size() {
        let (info, pixels) = small();
        let (_loader, _status, fog) = start(
            enshwick::test::FakeHost::new()
                .side(Side::CLIENT)
                .fog(info, pixels),
        );
        let fog = fog.unwrap();

        assert_eq!(fog.coverage([1.0, 0.0, 23.0]), Ok(Some(0)));
        assert_eq!(fog.coverage([25.0, 0.0, 1.0]), Ok(Some(23)));
        assert_eq!(fog.coverage([40.0, 0.0, 1.0]), Ok(None));
    }

    #[test]
    fn the_fog_is_missing_until_the_client_enters_a_world() {
        let (loader, _status, fog) = start(enshwick::test::FakeHost::new().side(Side::CLIENT));
        let fog = fog.unwrap();
        assert_eq!(fog.state(), Err(FogError::Missing));

        let (info, pixels) = small();
        loader.set_fog(info, pixels);
        assert_eq!(fog.state().map(|state| state.session), Ok(1));
    }

    #[test]
    fn a_dedicated_server_has_no_fog() {
        let (_loader, status, fog) = start(enshwick::test::FakeHost::new());
        assert_ne!(status, Status::OK);
        assert!(fog.is_none());
    }
}
