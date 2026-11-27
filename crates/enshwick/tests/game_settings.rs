//! Game settings: typed reads and a change, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::WorldError;
    use enshwick::abi::Status;
    use enshwick::game_settings::{
        GameSettings, GameSettingsPreset, RandomSpawnerAmount, SettingsRead,
        TamingStartleRepercussion, WeatherFrequency,
    };
    use enshwick::{Load, Mod, Off};

    static SETTINGS: OnceLock<GameSettings> = OnceLock::new();

    struct Tuning;

    impl Mod for Tuning {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = SETTINGS.set(load.game_settings()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Tuning,
        r#"
        id = "you.tuning"
        side = "server"
        "#
    );

    #[test]
    fn settings_read_typed_and_change_in_place() {
        let loader = enshwick::test::FakeHost::new().game_settings().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Tuning)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let settings = SETTINGS.get().unwrap();

        assert_eq!(
            settings.read().err(),
            Some(WorldError::Unavailable),
            "no world runs yet"
        );
        loader.set_default_game_settings();
        let elsewhere = std::thread::spawn(|| SETTINGS.get().unwrap().read().err())
            .join()
            .unwrap();
        assert_eq!(
            elsewhere,
            Some(WorldError::WrongThread),
            "read on the game thread only"
        );
        let SettingsRead {
            settings: now,
            preset,
            ..
        } = settings.read().unwrap();
        assert_eq!(preset, GameSettingsPreset::Default);
        assert!(now.enable_durability);
        assert_eq!(now.weather_frequency, WeatherFrequency::Normal);
        assert_eq!(now.day_time_duration, Duration::from_mins(30));
        assert_eq!(now.flame_level_fog_removal, 11);
        assert!((now.perk_upgrade_recycling_factor - 0.5).abs() < f32::EPSILON);
        assert_eq!(now.random_spawner_amount, RandomSpawnerAmount::Automatic);
        assert_eq!(
            now.taming_startle_repercussion,
            TamingStartleRepercussion::LoseSomeProgress
        );
        assert_eq!(now.from_hunger_to_starving, Duration::from_mins(10));

        let changed = settings
            .change(|s| {
                s.enemy_damage_factor = 2.5;
                s.weather_frequency = WeatherFrequency::Disabled;
            })
            .unwrap();
        assert!((changed.enemy_damage_factor - 2.5).abs() < f32::EPSILON);
        let SettingsRead {
            settings: after,
            preset,
            ..
        } = settings.read().unwrap();
        assert_eq!(
            preset,
            GameSettingsPreset::Custom,
            "a change makes the preset custom"
        );
        assert!((after.enemy_damage_factor - 2.5).abs() < f32::EPSILON);
        assert_eq!(after.weather_frequency, WeatherFrequency::Disabled);
        assert_eq!(after.day_time_duration, now.day_time_duration, "untouched");

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

        // SAFETY: the fake's table is a static, aligned and never written.
        let api = unsafe { &*loader.api() };
        let mask = [1u8; 4];
        let forget = |mask: *const u8, size| {
            (api.set_game_settings)(loader.handle(), core::ptr::null(), mask, size)
        };
        assert_eq!(
            forget(mask.as_ptr(), 4),
            Status::BAD_ARGS,
            "a mask with no data"
        );
        assert_eq!(
            forget(core::ptr::null(), 4),
            Status::BAD_ARGS,
            "a size with no data"
        );
        assert_eq!(forget(core::ptr::null(), 0), Status::OK);
    }
}
