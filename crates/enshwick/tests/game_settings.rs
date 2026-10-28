//! Game settings: typed reads and a change, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::WorldError;
    use enshwick::abi::Status;
    use enshwick::game_settings::{GameSettings, GameSettingsPreset, WeatherFrequency};
    use enshwick::{Load, Mod, Off};

    const TYPE: &str = "game_settings";
    /// The layout of the 1.0 build: id and offset.
    const FIELDS: [(&str, u64); 38] = [
        ("game_settings.player_health_factor", 0x00),
        ("game_settings.player_mana_factor", 0x04),
        ("game_settings.player_stamina_factor", 0x08),
        ("game_settings.player_body_heat_factor", 0x0c),
        ("game_settings.player_diving_time_factor", 0x10),
        ("game_settings.enable_durability", 0x14),
        ("game_settings.enable_starving_debuff", 0x15),
        ("game_settings.food_buff_duration_factor", 0x18),
        ("game_settings.from_hunger_to_starving", 0x20),
        ("game_settings.shroud_time_factor", 0x28),
        ("game_settings.tombstone_mode", 0x2c),
        ("game_settings.enable_glider_turbulences", 0x2d),
        ("game_settings.weather_frequency", 0x2e),
        ("game_settings.fishing_difficulty", 0x2f),
        ("game_settings.mining_damage_factor", 0x30),
        ("game_settings.plant_growth_speed_factor", 0x34),
        ("game_settings.resource_drop_stack_amount_factor", 0x38),
        ("game_settings.factory_production_speed_factor", 0x3c),
        ("game_settings.perk_upgrade_recycling_factor", 0x40),
        ("game_settings.perk_cost_factor", 0x44),
        ("game_settings.experience_combat_factor", 0x48),
        ("game_settings.experience_mining_factor", 0x4c),
        ("game_settings.experience_exploration_quests_factor", 0x50),
        ("game_settings.random_spawner_amount", 0x54),
        ("game_settings.aggro_pool_amount", 0x55),
        ("game_settings.enemy_damage_factor", 0x58),
        ("game_settings.enemy_health_factor", 0x5c),
        ("game_settings.enemy_stamina_factor", 0x60),
        ("game_settings.enemy_perception_range_factor", 0x64),
        ("game_settings.boss_damage_factor", 0x68),
        ("game_settings.boss_health_factor", 0x6c),
        ("game_settings.threat_bonus", 0x70),
        ("game_settings.pacify_all_enemies", 0x74),
        ("game_settings.taming_startle_repercussion", 0x75),
        ("game_settings.day_time_duration", 0x78),
        ("game_settings.night_time_duration", 0x80),
        ("game_settings.curse_modifier", 0x88),
        ("game_settings.flame_level_fog_removal", 0x89),
    ];

    /// The fields that are factors, 1 for the game's normal value.
    const FACTORS: [&str; 22] = [
        "game_settings.player_health_factor",
        "game_settings.player_mana_factor",
        "game_settings.player_stamina_factor",
        "game_settings.player_body_heat_factor",
        "game_settings.player_diving_time_factor",
        "game_settings.food_buff_duration_factor",
        "game_settings.shroud_time_factor",
        "game_settings.mining_damage_factor",
        "game_settings.plant_growth_speed_factor",
        "game_settings.resource_drop_stack_amount_factor",
        "game_settings.factory_production_speed_factor",
        "game_settings.perk_upgrade_recycling_factor",
        "game_settings.perk_cost_factor",
        "game_settings.experience_combat_factor",
        "game_settings.experience_mining_factor",
        "game_settings.experience_exploration_quests_factor",
        "game_settings.enemy_damage_factor",
        "game_settings.enemy_health_factor",
        "game_settings.enemy_stamina_factor",
        "game_settings.enemy_perception_range_factor",
        "game_settings.boss_damage_factor",
        "game_settings.boss_health_factor",
    ];

    /// The game's values of the enum constants the test reads and writes.
    const ENUM_VALUES: [(&str, u64); 5] = [
        ("game_settings_preset.default", 0),
        ("game_settings_preset.custom", 4),
        ("weather_frequency.disabled", 0),
        ("weather_frequency.normal", 2),
        ("weather_frequency.often", 3),
    ];

    static SETTINGS: OnceLock<GameSettings> = OnceLock::new();

    struct Tuning;

    impl Mod for Tuning {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.tuning", side: SERVER)
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
            if FACTORS.contains(&name) {
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
        for (name, at) in FIELDS.into_iter().chain(ENUM_VALUES) {
            host = host.constant(name, at);
        }
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Tuning)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let settings = SETTINGS.get().unwrap();

        assert_eq!(settings.read(), None, "no world runs yet");
        loader.set_game_settings(defaults(), 0);
        let (now, preset) = settings.read().unwrap();
        assert_eq!(preset, GameSettingsPreset::Default);
        assert!(now.enable_durability);
        assert_eq!(now.weather_frequency, WeatherFrequency::Normal);
        assert_eq!(now.day_time_duration, Duration::from_mins(30));
        assert_eq!(now.flame_level_fog_removal, 11);

        let changed = settings
            .change(|s| {
                s.enemy_damage_factor = 2.5;
                s.weather_frequency = WeatherFrequency::Disabled;
            })
            .unwrap();
        assert!((changed.enemy_damage_factor - 2.5).abs() < f32::EPSILON);
        let (bytes, preset) = loader.game_settings().unwrap();
        assert_eq!(preset, 4, "a change makes the preset custom");
        assert_eq!(&bytes[0x58..0x5c], &2.5f32.to_le_bytes());
        assert_eq!(bytes[0x2e], 0);
        assert_eq!(&bytes[0x78..0x80], &defaults()[0x78..0x80], "untouched");

        assert_eq!(
            settings.change(|s| s.night_time_duration = Duration::ZERO),
            Err(WorldError::Refused)
        );
        assert_eq!(
            settings.change(|s| s.player_health_factor = f32::NAN),
            Err(WorldError::Refused)
        );

        assert!(
            loader.kept_game_settings().is_empty(),
            "a change keeps nothing"
        );
        settings.keep(|s| s.boss_health_factor = 3.0).unwrap();
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
