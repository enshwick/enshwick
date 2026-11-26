//! `enshwick.vetoes`: a script's build filter refuses, with or without a
//! reason, as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::{BuildCheck, PlayerInfo};
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId, PlayerKey};

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
        let build = |item, y: i32| {
            enshwick::abi::headed!(BuildCheck {
                character: Entity::from_engine(2),
                item: ItemId::new(item),
                position: [0.0, f64::from(y), 0.0],
                reason: core::ptr::null_mut(),
                account: PlayerKey::default(),
            })
        };
        assert_eq!(loader.build(&build(7, 5)), Ok(()));
        assert_eq!(
            loader.build(&build(7, -1)),
            Err(Some("picky.below_sea".to_owned()))
        );
        assert_eq!(loader.build(&build(13, 5)), Err(None));
    }

    #[test]
    fn a_script_s_build_filter_judges_the_builder_by_account() {
        let ann = PlayerInfo::with_name(Entity::from_engine(2), PlayerKey::new(7), 0, "Ann");
        let loader = run(
            FakeHost::new().player(ann),
            r#"
            enshwick.vetoes.on_build(function(placement)
                if placement.account == "7" then
                    return "zone.not_yours"
                end
                return placement.account == nil
            end)
            "#,
        );
        let build = |character| {
            enshwick::abi::headed!(BuildCheck {
                character: Entity::from_engine(character),
                item: ItemId::new(7),
                position: [0.0; 3],
                reason: core::ptr::null_mut(),
                account: PlayerKey::default(),
            })
        };
        assert_eq!(
            loader.build(&build(2)),
            Err(Some("zone.not_yours".to_owned()))
        );
        assert_eq!(loader.build(&build(9)), Ok(()));
    }
}
