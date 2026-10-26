use crate::abi::StatRef as Ref;
use crate::content::categories::ItemCategoryMask;
use crate::stats::Stat;

use super::*;

fn stat(index: u32) -> Stat {
    // Stats are handed out by the loader; tests build them by their raw
    // form.
    Stat::from_ref(Ref { index, key: index })
}

fn pool() -> Vec<(AffixKey, AffixDef)> {
    vec![
        (
            AffixKey(1),
            AffixDef::new("bear", ItemCategoryMask::EQUIPMENT)
                .with(Range::new(stat(0), Op::Add, 10.0, 30.0).step(1.0)),
        ),
        (
            AffixKey(2),
            AffixDef::new(
                "fire",
                ItemCategoryMask::EQUIPMENT | ItemCategoryMask::WEAPONS,
            )
            .with(Range::new(stat(1), Op::Add, 0.05, 0.15))
            .group("element"),
        ),
        (
            AffixKey(3),
            AffixDef::new("ice", ItemCategoryMask::EQUIPMENT)
                .with(Range::new(stat(2), Op::Add, 0.05, 0.15))
                .group("element"),
        ),
        (
            AffixKey(4),
            AffixDef::new("myth", ItemCategoryMask::EQUIPMENT)
                .with(Range::new(stat(3), Op::Multiply, 1.5, 2.0))
                .rarity(ItemRarity::Legendary)
                .weight(1),
        ),
    ]
}

fn candidates(pool: &[(AffixKey, AffixDef)]) -> Vec<Candidate<'_>> {
    pool.iter()
        .map(|(key, def)| Candidate { key: *key, def })
        .collect()
}

#[test]
fn the_same_seed_rolls_the_same_whatever_the_pool_order() {
    let pool = pool();
    let mut reversed = candidates(&pool);
    reversed.reverse();
    for seed in 0..200 {
        let rules = RollRules::new(1, 3);
        assert_eq!(
            roll(&candidates(&pool), 1, rules, seed),
            roll(&reversed, 1, rules, seed)
        );
    }
}

#[test]
fn rolls_keep_groups_ranges_steps_and_rarity() {
    let pool = pool();
    let all = candidates(&pool);
    for seed in 0..500 {
        let rolled = roll(
            &all,
            1,
            RollRules::new(4, 4).max_rarity(ItemRarity::Epic),
            seed,
        );
        assert!(rolled.len() <= 3, "the legendary one stays out");
        assert!(rolled.iter().all(|r| r.affix != AffixKey(4)));
        let elements = rolled
            .iter()
            .filter(|r| r.affix == AffixKey(2) || r.affix == AffixKey(3))
            .count();
        assert!(elements <= 1, "one of a group");
        for r in &rolled {
            let def = &pool.iter().find(|(k, _)| *k == r.affix).unwrap().1;
            for (value, range) in r.values.iter().zip(&def.ranges) {
                assert!((range.min..=range.max).contains(value));
                if range.step > 0.0 {
                    assert!((value - value.round()).abs() < 1e-4, "whole steps");
                }
            }
        }
    }
}

#[test]
fn categories_decide_what_fits_and_weights_how_often() {
    let pool = pool();
    let all = candidates(&pool);
    for seed in 0..100 {
        let rolled = roll(&all, 2, RollRules::new(1, 1), seed);
        assert_eq!(
            rolled.iter().map(|r| r.affix).collect::<Vec<_>>(),
            [AffixKey(2)]
        );
    }
    let myths = (0..2000)
        .filter(|&seed| {
            roll(&all, 1, RollRules::new(1, 1), seed)
                .iter()
                .any(|r| r.affix == AffixKey(4))
        })
        .count();
    assert!(myths < 40, "weight 1 of 301 rolls rarely: {myths}");
    assert!(roll(&all, 1, RollRules::new(0, 0), 7).is_empty());
}

#[test]
fn definitions_need_modifiers_and_sane_ranges() {
    let empty = AffixDef::new("x", ItemCategoryMask::WEAPONS);
    assert!(empty.check().is_err());
    let backwards = empty.clone().with(Range::new(stat(0), Op::Add, 3.0, 1.0));
    assert!(backwards.check().is_err());
    let fine = empty.with(Range::new(stat(0), Op::Add, 1.0, 3.0));
    assert!(fine.check().is_ok());
}
