//! Affixes through the fake loader: an affix defined, an item rolled with a
//! seed and rolled again the same, a second roll refused, the worn item's
//! affix counted in its wearer's stat, and its lines named.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{ItemData, Status};
    use enshwick::affixes::{AffixDef, Affixes, ItemRarity, Range, RollError, RollRules};

    use enshwick::Entity;
    use enshwick::content::categories::ItemCategoryMask;
    use enshwick::content::id;
    use enshwick::inventory::ItemInstance;
    use enshwick::stats::{Op, Stat, StatDef, Stats};
    use enshwick::{Load, Mod, Off};

    const RING: u32 = 0x5e01;
    const SWORD: u32 = 0x5e02;

    struct Kit {
        affixes: Affixes,
        stats: Stats,
        luck: Stat,
    }

    static KIT: OnceLock<Kit> = OnceLock::new();

    struct Loot;

    impl Mod for Loot {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let luck = load.define_stat(id!("luck"), &StatDef::additive().range(0.0, 10.0))?;
            load.define_affix(
                id!("lucky"),
                &AffixDef::new(ItemCategoryMask::EQUIPMENT)
                    .with(Range::new(luck, Op::Add, 1.0, 5.0).step(1.0))
                    .rarity(ItemRarity::Rare),
            )?;
            assert!(
                load.define_affix(id!("lucky"), &AffixDef::new(ItemCategoryMask::EQUIPMENT))
                    .is_err(),
                "no modifiers"
            );
            let _first = KIT.set(Kit {
                affixes: load.affixes(),
                stats: load.stats()?,
                luck,
            });
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Loot,
        r#"
        id = "you.hello"
        side = "both"
        "#
    );

    #[test]
    fn items_roll_once_the_same_from_a_seed_and_count_when_worn() {
        let loader = enshwick::test::FakeHost::new()
            .item(enshwick::ItemId::new(RING), 1, ItemData::HAS_PIDE)
            .item_category(enshwick::ItemId::new(RING), 1)
            .item(enshwick::ItemId::new(SWORD), 1, ItemData::HAS_PIDE)
            .item_category(enshwick::ItemId::new(SWORD), 2)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Loot)`.
        let started = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(started, Status::OK);
        let Kit {
            affixes,
            stats,
            luck,
        } = KIT.get().unwrap();
        let ring = ItemInstance {
            pide: Entity(900),
            item: enshwick::ItemId::new(RING),
        };
        let rules = RollRules::new(1, 1);

        assert_eq!(affixes.roll(ring, rules, Some(7), true), Ok(1));
        let first = stats.item(ring);
        assert!(first.rolled);
        assert_eq!(
            affixes.roll(ring, rules, Some(7), true),
            Err(RollError::Rolled)
        );
        assert_eq!(affixes.roll(ring, rules, Some(7), false), Ok(1));
        assert_eq!(stats.item(ring), first, "the same seed rolls the same");

        let sword = ItemInstance {
            pide: Entity(901),
            item: enshwick::ItemId::new(SWORD),
        };
        assert_eq!(affixes.roll(sword, rules, Some(7), true), Ok(0));
        assert!(!stats.item(sword).rolled, "no affix fits a weapon");

        loader.wear(Entity(2), ring);
        let rolled = first.affixes[0].values[0];
        assert!((1.0..=5.0).contains(&rolled));
        assert!((stats.value(Entity(2), *luck) - rolled).abs() < 1e-5);

        let lines = affixes.lines(ring);
        assert_eq!(lines.len(), 1);
        let info = affixes.info(lines[0].affix.unwrap()).unwrap();
        assert_eq!(info.text, "lucky.name");
        assert_eq!(info.rarity, ItemRarity::Rare);
    }
}
