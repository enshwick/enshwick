//! Terrain pages loading and unloading, as the loader hands them over.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Header, Status, TerrainPageData};
    use enshwick::events::{TerrainPageLoaded, TerrainPageUnloaded};
    use enshwick::{Load, Mod, Off};

    static HEARD: Mutex<Vec<(bool, u32, u32, u32)>> = Mutex::new(Vec::new());

    struct Keeper;

    impl Mod for Keeper {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on::<TerrainPageLoaded>(|page| {
                HEARD
                    .lock()
                    .unwrap()
                    .push((true, page.x, page.z, page.size));
            })?;
            load.on::<TerrainPageUnloaded>(|page| {
                HEARD
                    .lock()
                    .unwrap()
                    .push((false, page.x, page.z, page.size));
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Keeper,
        r#"
        id = "you.keeper"
        side = "server"
        "#
    );

    fn page(x: u32, z: u32) -> TerrainPageData {
        TerrainPageData {
            header: Header::of::<TerrainPageData>(),
            x,
            z,
            size: 128,
        }
    }

    #[test]
    fn a_mod_hears_each_page_unload_and_load_in_the_order_raised() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Keeper)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        assert_eq!(loader.raise::<TerrainPageUnloaded>(&page(40, 7)), 1);
        assert_eq!(loader.raise::<TerrainPageLoaded>(&page(40, 7)), 1);
        assert_eq!(loader.raise::<TerrainPageLoaded>(&page(0, 79)), 1);
        assert_eq!(
            *HEARD.lock().unwrap(),
            [(false, 40, 7, 128), (true, 40, 7, 128), (true, 0, 79, 128)]
        );
    }
}
