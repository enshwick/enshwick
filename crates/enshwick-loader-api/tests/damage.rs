//! Damage filters: hits by type, falls and the shroud, as the loader runs
//! them.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::{DamageCheck, Header, Status};
    use enshwick_loader_api::damage::{DamageCause, DamageType};
    use enshwick_loader_api::{Load, Mod, Off};

    struct Soft;

    impl Mod for Soft {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "soft", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_damage(|damage| match damage.cause() {
                DamageCause::Fall => damage.scale(0.5),
                DamageCause::ShroudDeath => damage.cancel(),
                DamageCause::Hit if damage.source() == Some(damage.target()) => damage.cancel(),
                DamageCause::Hit => {
                    damage.scale_type(DamageType::Fire, 2.0);
                    let cut = damage.amount(DamageType::Cut);
                    damage.set(DamageType::Cut, cut.min(10.0));
                }
                _ => {}
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Soft);

    fn check(cause: u32, target: u32, source: u32) -> DamageCheck {
        DamageCheck {
            header: Header::of::<DamageCheck>(),
            cause,
            target,
            source,
            attacker: 0,
            amounts: [0.0; 13],
            total: 0.0,
        }
    }

    #[test]
    fn filters_scale_set_and_cancel_by_cause_and_type() {
        let loader = enshwick_loader_api::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        let mut hit = check(DamageCheck::HIT, 2, 9);
        hit.amounts[0] = 30.0;
        hit.amounts[3] = 5.0;
        hit.amounts[7] = 4.0;
        let after = loader.damage(hit);
        assert!((after.amounts[0] - 10.0).abs() < f32::EPSILON, "cut capped");
        assert!(
            (after.amounts[3] - 10.0).abs() < f32::EPSILON,
            "fire doubled"
        );
        assert!(
            (after.amounts[7] - 4.0).abs() < f32::EPSILON,
            "healing kept"
        );

        let mut own = check(DamageCheck::HIT, 2, 2);
        own.amounts[2] = 12.0;
        own.amounts[7] = 3.0;
        let after = loader.damage(own);
        assert!(after.amounts[2].abs() < f32::EPSILON, "cancelled");
        assert!(
            (after.amounts[7] - 3.0).abs() < f32::EPSILON,
            "a cancel keeps healing"
        );

        let mut fall = check(DamageCheck::FALL, 2, 0);
        fall.total = 40.0;
        assert!((loader.damage(fall).total - 20.0).abs() < f32::EPSILON);

        let mut death = check(DamageCheck::SHROUD_DEATH, 2, 0);
        death.total = 300.0;
        assert!(loader.damage(death).total.abs() < f32::EPSILON);
    }
}
