//! Mods add Flame Altar kinds: a prop copied from a vanilla altar whose
//! altars take the kind's zone at each level, chained with other zone
//! filters.

#[cfg(test)]
mod tests {
    use enshwick::abi::{CraftCheck, Header, Status};
    use enshwick::altars::{KindZone, NewFlameAltar};
    use enshwick::content::items::NewItem;
    use enshwick::content::{Guid, Id};
    use enshwick::test::{FakeHost, Installed, PROP_ITEM};
    use enshwick::{Entity, Load, Mod, Off, RecipeId};

    const WIDE: Id = Id::new("you.kinds:wide");
    const KIND_ALTAR: Entity = Entity(10);
    const OTHER_ALTAR: Entity = Entity(11);
    const CLAIM: Guid = Guid([0xc0; 16]);
    const UPGRADE: Guid = Guid([0xc1; 16]);
    const TORCH: u32 = 0x7042;

    struct Kinds;

    impl Mod for Kinds {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_flame_altar(NewFlameAltar {
                item: NewItem::new(WIDE, PROP_ITEM),
                zones: vec![
                    KindZone {
                        grow: 5,
                        height: 40,
                    },
                    KindZone {
                        grow: 9,
                        height: 50,
                    },
                ],
                recipe_cost: Some(1),
                max_level: Some(1),
            })?;
            // A later filter sees the kind's size and builds on it.
            load.on_altar_zone(|zone| zone.grow += 1)?;
            // A later craft filter keeps its own say beside the limit.
            load.on_craft(|craft| craft.recipe_id != RecipeId(TORCH))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Kinds,
        r#"
        id = "you.kinds"
        side = "both"
        "#
    );

    fn start() -> Installed {
        let kind = WIDE.part("template").guid().0;
        let loader = FakeHost::new()
            .props()
            .upgrade_recipes(&[(CLAIM, 0), (UPGRADE, 1)])
            .template(KIND_ALTAR, kind)
            .template(OTHER_ALTAR, [7; 16])
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Kinds)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn craft(loader: &Installed, station: Entity, recipe: RecipeId) -> bool {
        loader.craft(&CraftCheck {
            header: Header::of::<CraftCheck>(),
            crafting_operator_id: Entity(2),
            crafting_station_id: station,
            recipe_id: recipe,
            recipe_amount: 1,
            reserved: 0,
        })
    }

    #[test]
    fn a_kind_s_altars_refuse_an_upgrade_past_its_maximum_level_only() {
        let loader = start();
        let (claim, upgrade) = (RecipeId(CLAIM.fnv1a()), RecipeId(UPGRADE.fnv1a()));

        assert!(craft(&loader, KIND_ALTAR, claim), "below the maximum");
        assert!(!craft(&loader, KIND_ALTAR, upgrade), "past the maximum");
        assert!(craft(&loader, OTHER_ALTAR, upgrade), "another kind's altar");
        assert!(craft(&loader, Entity(0), upgrade), "by hand");
        assert!(
            !craft(&loader, OTHER_ALTAR, RecipeId(TORCH)),
            "the later filter's own refusal"
        );
    }

    #[test]
    fn a_kind_s_altars_take_its_zone_per_level_and_other_filters_follow() {
        let loader = start();

        assert_eq!(loader.altar_zone(KIND_ALTAR, 1, 30), (6, 40));
        assert_eq!(loader.altar_zone(KIND_ALTAR, 2, 30), (10, 50));
        assert_eq!(
            loader.altar_zone(KIND_ALTAR, 5, 30),
            (10, 50),
            "the last level's"
        );
        assert_eq!(
            loader.altar_zone(OTHER_ALTAR, 1, 30),
            (1, 30),
            "another kind's"
        );
        assert!(
            loader
                .content_ids()
                .contains(&WIDE.part("recipe").to_string()),
            "the recipe the cost asks for: {:?}",
            loader.content_ids()
        );
    }
}
