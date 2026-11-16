//! `enshwick.terrain.add_terrain_material`: a script's own terrain material,
//! as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_script_adds_a_material_at_the_first_free_id_and_a_bad_one_raises_an_error() {
        let loader = run(
            FakeHost::new(),
            r#"
            local id = enshwick.terrain.add_terrain_material({
                id = "you.luau:tar-sand",
                like = 9,
                fallback = 24,
                health_points = 7,
                can_be_flattened = false,
            })
            enshwick.log.info(`added {id}`)
            local ok = pcall(enshwick.terrain.add_terrain_material, { id = "you.luau:odd", like = 117 })
            enshwick.log.info(`odd {ok}`)
            "#,
        );
        assert!(logged(&loader, "added 117"), "{:?}", loader.logs());
        assert!(logged(&loader, "odd false"), "{:?}", loader.logs());
    }
}
