//! Damage filters: hits by type, falls and the shroud, as the loader runs
//! them.

#[cfg(test)]
mod tests {
    use enshwick::abi::{DAMAGE_TYPES, DamageCheck, Status};
    use enshwick::damage::{DamageCause, DamageTypeNew};
    use enshwick::{Load, Mod, Off};

    /// The index of a damage type a later game build adds after the 13
    /// this crate names.
    const NEW_TYPE: usize = DAMAGE_TYPES;

    struct Soft;

    impl Mod for Soft {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_damage(|damage| match damage.cause() {
                DamageCause::FallDamage => damage.scale(0.5),
                DamageCause::DeadlyFog => damage.cancel(),
                DamageCause::Impact if damage.source() == Some(damage.target()) => damage.cancel(),
                DamageCause::Impact => {
                    damage.scale_type(DamageTypeNew::MagicalFire, 2.0);
                    let cut = damage.amount(DamageTypeNew::PhysicalCut);
                    damage.set(DamageTypeNew::PhysicalCut, cut.min(10.0));
                    let new = damage.amount_at(NEW_TYPE);
                    damage.set_at(NEW_TYPE, new + 1.0);
                }
                _ => {}
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Soft,
        r#"
        id = "you.soft"
        side = "server"
        "#
    );

    fn check(cause: u32, target: u32, source: u32) -> DamageCheck {
        enshwick::abi::headed!(DamageCheck {
            cause,
            target: enshwick::Entity::from_engine(target),
            source: enshwick::Entity::from_engine(source),
            attacker: enshwick::Entity::from_engine(0),
            total: 0.0,
            amount_count: 0,
            amounts: core::ptr::null_mut(),
        })
    }

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Soft)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < f32::EPSILON
    }

    #[test]
    fn filters_scale_set_and_cancel_by_cause_and_type() {
        let loader = started();

        let mut hit = [0.0; DAMAGE_TYPES];
        hit[DamageTypeNew::PhysicalCut.index()] = 30.0;
        hit[DamageTypeNew::MagicalFire.index()] = 5.0;
        hit[DamageTypeNew::Healing.index()] = 4.0;
        let after = loader.damage(check(DamageCheck::IMPACT, 2, 9), &mut hit);
        assert!(after.amounts.is_null(), "the lent amounts are not kept");
        assert!(
            close(hit[DamageTypeNew::PhysicalCut.index()], 10.0),
            "cut capped"
        );
        assert!(
            close(hit[DamageTypeNew::MagicalFire.index()], 10.0),
            "fire doubled"
        );
        assert!(
            close(hit[DamageTypeNew::Healing.index()], 4.0),
            "healing kept"
        );

        let mut own = [0.0; DAMAGE_TYPES];
        own[DamageTypeNew::PhysicalBlunt.index()] = 12.0;
        own[DamageTypeNew::Healing.index()] = 3.0;
        let _after = loader.damage(check(DamageCheck::IMPACT, 2, 2), &mut own);
        assert!(
            own[DamageTypeNew::PhysicalBlunt.index()].abs() < f32::EPSILON,
            "cancelled"
        );
        assert!(
            close(own[DamageTypeNew::Healing.index()], 3.0),
            "a cancel keeps healing"
        );

        let mut fall = check(DamageCheck::FALL_DAMAGE, 2, 0);
        fall.total = 40.0;
        assert!(close(loader.damage(fall, &mut []).total, 20.0));

        let mut death = check(DamageCheck::DEADLY_FOG, 2, 0);
        death.total = 300.0;
        assert!(loader.damage(death, &mut []).total.abs() < f32::EPSILON);
    }

    #[test]
    fn a_build_with_a_fourteenth_damage_type_reaches_the_filter_by_index() {
        let loader = started();

        let mut hit = [0.0; DAMAGE_TYPES + 1];
        hit[DamageTypeNew::MagicalFire.index()] = 5.0;
        hit[NEW_TYPE] = 2.0;
        let _after = loader.damage(check(DamageCheck::IMPACT, 2, 9), &mut hit);
        assert!(
            close(hit[NEW_TYPE], 3.0),
            "the new type read and set by index"
        );
        assert!(
            close(hit[DamageTypeNew::MagicalFire.index()], 10.0),
            "named types unchanged in place"
        );

        let mut known = [0.0; DAMAGE_TYPES];
        let _after = loader.damage(check(DamageCheck::IMPACT, 2, 9), &mut known);
        assert!(
            known.iter().all(|a| a.abs() < f32::EPSILON),
            "an index past the count reads 0 and sets nothing"
        );
    }
}
