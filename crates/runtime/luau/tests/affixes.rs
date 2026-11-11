//! `enshwick.affixes`: the pure rolls a script runs, the same as Rust's, and
//! the affixes it defines while it starts, under the same checks.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::affixes::{AffixDef, AffixKey, Candidate, RollRules, roll};
    use enshwick::content::categories::ItemCategoryMask;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run, start};

    #[test]
    fn a_script_s_roll_matches_rust_s_for_the_same_pool_and_seed() {
        let (name, mask) = ItemCategoryMask::NAMES[0];
        let category = (0..=u8::MAX).find(|&c| mask.holds(c)).unwrap();
        let ring = AffixDef::new(mask).weight(3);
        let amulet = AffixDef::new(mask).weight(5);
        let pool = [
            Candidate {
                key: AffixKey(1),
                def: &ring,
            },
            Candidate {
                key: AffixKey(2),
                def: &amulet,
            },
        ];
        let expected = roll(&pool, category, RollRules::new(1, 2), 42);
        let keys: Vec<String> = expected.iter().map(|r| r.affix.0.to_string()).collect();
        let loader = run(
            FakeHost::new(),
            &format!(
                r#"
                local function def(weight)
                    return {{ ranges = {{}}, weight = weight, rarity = "Common", categories = {{ "{name}" }} }}
                end
                local rolled = enshwick.affixes.roll_pool(
                    {{ {{ key = 1, def = def(3) }}, {{ key = 2, def = def(5) }} }},
                    {category},
                    {{ min = 1, max = 2, max_rarity = "Legendary" }},
                    "42"
                )
                local keys = {{}}
                for _, r in rolled do table.insert(keys, tostring(r.affix)) end
                enshwick.log.info(`rolled {{table.concat(keys, ",")}}`)
                "#
            ),
        );
        assert!(
            logged(&loader, &format!("rolled {}", keys.join(","))),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn an_affix_without_a_modifier_refuses_the_start_with_why() {
        let (name, _) = ItemCategoryMask::NAMES[0];
        let (loader, status) = start(
            FakeHost::new(),
            &format!(
                r#"enshwick.affixes.define_affix("you.luau:bear", {{ ranges = {{}}, weight = 100, rarity = "Common", categories = {{ "{name}" }} }})"#
            ),
        );
        assert_ne!(status, Status::OK);
        assert!(
            logged(&loader, "enshwick.affixes.define_affix"),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "needs a modifier"));
    }
}
