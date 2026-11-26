//! `enshwick.damage`: a script's filter sees and changes each hit, as a
//! Rust mod's, through one object it keeps.

#[cfg(test)]
mod tests {
    use enshwick::abi::DamageCheck;
    use enshwick::damage::DamageTypeNew;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    fn check(cause: u32, total: f32) -> DamageCheck {
        enshwick::abi::headed!(DamageCheck {
            cause,
            target: enshwick::Entity::from_engine(2),
            source: enshwick::Entity::from_engine(0),
            attacker: enshwick::Entity::from_engine(0),
            total,
            amount_count: 0,
            amounts: core::ptr::null_mut(),
        })
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
        let fall = loader.damage(check(DamageCheck::FALL_DAMAGE, 40.0), &mut []);
        assert!((fall.total - 20.0).abs() < 1e-6);
        let death = loader.damage(check(DamageCheck::DEADLY_FOG, 300.0), &mut []);
        assert!(death.total.abs() < 1e-6);
        let mut cut = [0.0; enshwick::abi::DAMAGE_TYPES];
        cut[DamageTypeNew::PhysicalCut.index()] = 4.0;
        let _after = loader.damage(check(DamageCheck::IMPACT, 0.0), &mut cut);
        assert!((cut[DamageTypeNew::PhysicalCut.index()] - 5.0).abs() < 1e-6);
        for line in [
            "hit 1 FallDamage same true",
            "hit 2 DeadlyFog same true",
            "hit 3 Impact same true",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_damage_object_kept_past_its_filter_refuses_and_leaves_later_hits_alone() {
        let loader = run(
            FakeHost::new(),
            r"
            local kept = nil
            enshwick.damage.on_damage(function(damage) kept = damage end)
            enshwick.damage.on_damage(function()
                local ok = pcall(function() kept:cancel() end)
                enshwick.log.info(`stale {ok}`)
            end)
            ",
        );
        assert!(
            (loader
                .damage(check(DamageCheck::IMPACT, 40.0), &mut [])
                .total
                - 40.0)
                .abs()
                < 1e-6
        );
        assert!(logged(&loader, "stale false"), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_learns_the_causes_its_filter_does_not_get_once_the_world_loads() {
        let loader = run(
            FakeHost::new().without_damage_sites(&[enshwick::damage::DamageCause::FallDamage]),
            r#"
            enshwick.damage.on_damage(function(damage) damage:scale(2) end)
            local early = pcall(enshwick.damage.causes_off)
            enshwick.log.info(`early {early}`)
            enshwick.on("WorldLoaded", function()
                local off = enshwick.damage.causes_off()
                enshwick.log.info(`off {#off} {off[1]}`)
            end)
            "#,
        );
        assert!(
            loader.raise::<enshwick::events::WorldLoaded>(&()) > 0,
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "early false"), "{:?}", loader.logs());
        assert!(logged(&loader, "off 1 FallDamage"), "{:?}", loader.logs());
    }
}
