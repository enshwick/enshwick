//! Craft and build filters, as the loader asks them.

#[cfg(test)]
mod tests {
    use enshwick::abi::{BuildCheck, CraftCheck, Header, Status};
    use enshwick::{Load, Mod, Off};

    struct Picky;

    impl Mod for Picky {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_craft(|craft| craft.crafting_station_id.is_some() || craft.recipe_amount < 10)?;
            load.on_build(|placement| {
                placement.position[1] >= 0.0 && placement.item != enshwick::ItemId::new(13)
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Picky,
        r#"
        id = "you.picky"
        side = "both"
        "#
    );

    #[test]
    fn filters_see_typed_requests_and_refuse_by_returning_false() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Picky)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let craft = |crafting_station_id, recipe_amount| CraftCheck {
            header: Header::of::<CraftCheck>(),
            crafting_operator_id: enshwick::Entity(2),
            crafting_station_id,
            recipe_id: enshwick::RecipeId::new(5),
            recipe_amount,
            reserved: 0,
        };
        assert!(loader.craft(&craft(enshwick::Entity(0), 3)));
        assert!(!loader.craft(&craft(enshwick::Entity(0), 12)));
        assert!(loader.craft(&craft(enshwick::Entity(40), 12)));
        let build = |item, y: i64| BuildCheck {
            header: Header::of::<BuildCheck>(),
            player: enshwick::Entity(2),
            item,
            position: [0, y << 32, 0],
        };
        assert!(loader.build(&build(enshwick::ItemId::new(7), 5)));
        assert!(!loader.build(&build(enshwick::ItemId::new(7), -1)));
        assert!(!loader.build(&build(enshwick::ItemId::new(13), 5)));
    }
}
