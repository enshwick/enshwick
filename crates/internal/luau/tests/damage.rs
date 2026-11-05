//! `enshwick.damage`: a script's filter sees and changes each hit, as a
//! Rust mod's, through one object it keeps.

#[cfg(test)]
mod tests {
    use enshwick::abi::{DamageCheck, Header};
    use enshwick::damage::DamageTypeNew;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    fn check(cause: u32, total: f32) -> DamageCheck {
        DamageCheck {
            header: Header::of::<DamageCheck>(),
            cause,
            target: enshwick::Entity(2),
            source: enshwick::Entity(0),
            attacker: enshwick::Entity(0),
            amounts: [0.0; enshwick::abi::DAMAGE_TYPES],
            total,
        }
    }

    #[test]
    fn a_filter_changes_each_hit_through_the_one_object_it_gets() {
        let loader = run(
            FakeHost::new(),
            r#"
            local kept = nil
            local hits = 0
            enshwick.damage.on_damage(function(damage)
                hits += 1
                local same = kept == nil or rawequal(kept, damage)
                kept = damage
                if damage:cause() == "FallDamage" then
                    damage:scale(0.5)
                elseif damage:cause() == "DeadlyFog" then
                    damage:cancel()
                else
                    damage:set("PhysicalCut", damage:amount("PhysicalCut") + 1)
                end
                enshwick.log.info(`hit {hits} {damage:cause()} same {same}`)
            end)
            "#,
        );
        assert!((loader.damage(check(DamageCheck::FALL_DAMAGE, 40.0)).total - 20.0).abs() < 1e-6);
        assert!(
            loader
                .damage(check(DamageCheck::DEADLY_FOG, 300.0))
                .total
                .abs()
                < 1e-6
        );
        let mut cut = check(DamageCheck::IMPACT, 0.0);
        cut.amounts[DamageTypeNew::PhysicalCut.index()] = 4.0;
        let after = loader.damage(cut);
        assert!((after.amounts[DamageTypeNew::PhysicalCut.index()] - 5.0).abs() < 1e-6);
        for line in [
            "hit 1 FallDamage same true",
            "hit 2 DeadlyFog same true",
            "hit 3 Impact same true",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }
}
