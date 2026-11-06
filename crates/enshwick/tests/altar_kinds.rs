//! Mods add Flame Altar kinds: a prop copied from a vanilla altar whose
//! altars take the kind's zone at each level, chained with other zone
//! filters.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::altars::{KindZone, NewFlameAltar};
    use enshwick::content::Id;
    use enshwick::content::items::NewItem;
    use enshwick::test::{FakeHost, PROP_ITEM};
    use enshwick::{Entity, Load, Mod, Off};

    const WIDE: Id = Id::new("you.kinds:wide");
    const KIND_ALTAR: Entity = Entity(10);
    const OTHER_ALTAR: Entity = Entity(11);

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
                recipe_cost: None,
            })?;
            // A later filter sees the kind's size and builds on it.
            load.on_altar_zone(|zone| zone.grow += 1)?;
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

    #[test]
    fn a_kind_s_altars_take_its_zone_per_level_and_other_filters_follow() {
        let kind = WIDE.part("template").guid().0;
        let loader = FakeHost::new()
            .props()
            .template(KIND_ALTAR, kind)
            .template(OTHER_ALTAR, [7; 16])
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Kinds)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

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
    }
}
