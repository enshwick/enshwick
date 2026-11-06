//! `enshwick.vetoes`: a script's build filter refuses, with or without a
//! reason, as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::{BuildCheck, Header};
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId};

    use crate::harness::run;

    #[test]
    fn a_script_s_build_filter_refuses_with_its_string_as_the_reason() {
        let loader = run(
            FakeHost::new(),
            r#"
            enshwick.vetoes.on_build(function(placement)
                if placement.item == 13 then
                    return false
                end
                if placement.position.y < 0 then
                    return "picky.below_sea"
                end
                return nil
            end)
            "#,
        );
        let build = |item, y: i64| BuildCheck {
            header: Header::of::<BuildCheck>(),
            player: Entity(2),
            item: ItemId::new(item),
            position: [0, y << 32, 0],
            reason: core::ptr::null_mut(),
        };
        assert_eq!(loader.build(&build(7, 5)), Ok(()));
        assert_eq!(
            loader.build(&build(7, -1)),
            Err(Some("picky.below_sea".to_owned()))
        );
        assert_eq!(loader.build(&build(13, 5)), Err(None));
    }
}
