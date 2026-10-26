//! Game settings: typed reads and a change, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::abi::Status;
    use enshwick::game_settings::{GameSettings, Preset, WeatherFrequency};
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    const TYPE: &str = "keen::ecs::GameSettings";
    /// The layout of the 1.0 build: reflected name and offset.
    const FIELDS: [(&str, u64); 38] = [
        ("playerHealthFactor", 0x00),
        ("playerManaFactor", 0x04),
        ("playerStaminaFactor", 0x08),
        ("playerBodyHeatFactor", 0x0c),
        ("playerDivingTimeFactor", 0x10),
        ("enableDurability", 0x14),
        ("enableStarvingDebuff", 0x15),
        ("foodBuffDurationFactor", 0x18),
        ("fromHungerToStarving", 0x20),
        ("shroudTimeFactor", 0x28),
        ("tombstoneMode", 0x2c),
        ("enableGliderTurbulences", 0x2d),
        ("weatherFrequency", 0x2e),
        ("fishingDifficulty", 0x2f),
        ("miningDamageFactor", 0x30),
        ("plantGrowthSpeedFactor", 0x34),
        ("resourceDropStackAmountFactor", 0x38),
        ("factoryProductionSpeedFactor", 0x3c),
        ("perkUpgradeRecyclingFactor", 0x40),
        ("perkCostFactor", 0x44),
        ("experienceCombatFactor", 0x48),
        ("experienceMiningFactor", 0x4c),
        ("experienceExplorationQuestsFactor", 0x50),
        ("randomSpawnerAmount", 0x54),
        ("aggroPoolAmount", 0x55),
        ("enemyDamageFactor", 0x58),
        ("enemyHealthFactor", 0x5c),
        ("enemyStaminaFactor", 0x60),
        ("enemyPerceptionRangeFactor", 0x64),
        ("bossDamageFactor", 0x68),
        ("bossHealthFactor", 0x6c),
        ("threatBonus", 0x70),
        ("pacifyAllEnemies", 0x74),
        ("tamingStartleRepercussion", 0x75),
        ("dayTimeDuration", 0x78),
        ("nightTimeDuration", 0x80),
        ("curseModifier", 0x88),
        ("flameLevelFogRemoval", 0x89),
    ];

    static SETTINGS: OnceLock<GameSettings> = OnceLock::new();

    struct Tuning;

    impl Mod for Tuning {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "tuning", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = SETTINGS.set(load.game_settings()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Tuning);

    fn defaults() -> Vec<u8> {
        let mut bytes = vec![0u8; 144];
        for (name, at) in FIELDS {
            let at = usize::try_from(at).unwrap();
            if name.ends_with("Factor") {
                bytes[at..at + 4].copy_from_slice(&1.0f32.to_le_bytes());
            }
        }
        bytes[0x14] = 1;
        bytes[0x2e] = 2;
        bytes[0x78..0x80].copy_from_slice(&1_800_000_000_000i64.to_le_bytes());
        bytes[0x80..0x88].copy_from_slice(&720_000_000_000i64.to_le_bytes());
        bytes[0x89] = 11;
        bytes
    }

    #[test]
    fn settings_read_typed_and_change_in_place() {
        let mut host = enshwick::test::FakeHost::new().type_size(TYPE, 144);
        for (name, at) in FIELDS {
            host = host.field_offset(TYPE, name, at);
        }
        let loader = host.install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let settings = SETTINGS.get().unwrap();

        assert_eq!(settings.read(), None, "no world runs yet");
        loader.set_game_settings(defaults(), 0);
        let (now, preset) = settings.read().unwrap();
        assert_eq!(preset, Preset::Default);
        assert!(now.durability);
        assert_eq!(now.weather, WeatherFrequency::Normal);
        assert_eq!(now.day_length, Duration::from_mins(30));
        assert_eq!(now.flame_level_fog_removal, 11);

        let changed = settings
            .change(|s| {
                s.enemy_damage = 2.5;
                s.weather = WeatherFrequency::Disabled;
            })
            .unwrap();
        assert!((changed.enemy_damage - 2.5).abs() < f32::EPSILON);
        let (bytes, preset) = loader.game_settings().unwrap();
        assert_eq!(preset, 4, "a change makes the preset custom");
        assert_eq!(&bytes[0x58..0x5c], &2.5f32.to_le_bytes());
        assert_eq!(bytes[0x2e], 0);
        assert_eq!(&bytes[0x78..0x80], &defaults()[0x78..0x80], "untouched");

        assert_eq!(
            settings.change(|s| s.night_length = Duration::ZERO),
            Err(WorldError::Refused)
        );
        assert_eq!(
            settings.change(|s| s.player_health = f32::NAN),
            Err(WorldError::Refused)
        );

        assert!(
            loader.kept_game_settings().is_empty(),
            "a change keeps nothing"
        );
        settings.keep(|s| s.boss_health = 3.0).unwrap();
        let kept = loader.kept_game_settings();
        let held: Vec<usize> = (0..kept.len()).filter(|&i| kept[i].is_some()).collect();
        assert_eq!(held, vec![0x6e, 0x6f], "only the bytes the change moved");
        assert_eq!(
            &loader.game_settings().unwrap().0[0x6c..0x70],
            &3.0f32.to_le_bytes()
        );
        settings.forget_kept().unwrap();
        assert!(loader.kept_game_settings().is_empty());
    }
}
