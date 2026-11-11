//! `enshwick.on` for terrain pages: a script hears a page unload and load
//! with its place, as a Rust mod's handler does.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, TerrainPageData};
    use enshwick::events::{TerrainPageLoaded, TerrainPageUnloaded};
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_script_hears_a_page_unload_and_load_with_its_place() {
        let loader = run(
            FakeHost::new(),
            r#"
            enshwick.on("TerrainPageUnloaded", function(page)
                enshwick.log.info(`dropped {page.x},{page.z} {page.size}`)
            end)
            enshwick.on("TerrainPageLoaded", function(page)
                enshwick.log.info(`loaded {page.x},{page.z} {page.size}`)
            end)
            "#,
        );
        let page = TerrainPageData {
            header: Header::of::<TerrainPageData>(),
            x: 40,
            z: 7,
            size: 128,
        };
        assert_ne!(loader.raise::<TerrainPageUnloaded>(&page), 0);
        assert_ne!(loader.raise::<TerrainPageLoaded>(&page), 0);
        assert!(logged(&loader, "dropped 40,7 128"), "{:?}", loader.logs());
        assert!(logged(&loader, "loaded 40,7 128"), "{:?}", loader.logs());
    }
}
