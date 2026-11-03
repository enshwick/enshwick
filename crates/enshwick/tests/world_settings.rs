//! Settings come with the world: the defaults at init, the world's file as
//! each world loads, a change at the next load unless the mod follows
//! changes, and a refused file never turns the mod off.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::{Level, Status};
    use enshwick::{Load, Mod, Off, events};

    /// Limits.
    #[enshwick::config(validate = Limits::check)]
    #[derive(Default)]
    struct Limits {
        /// How many.
        #[config(range = 1..=16)]
        limit: u32,
    }

    impl Limits {
        fn check(&self) -> Result<(), String> {
            (self.limit != 13)
                .then_some(())
                .ok_or_else(|| "13 is unlucky".to_owned())
        }
    }

    /// Rates, in a file of their own.
    #[enshwick::config(file = "rates")]
    #[derive(Default)]
    struct Rates {
        /// The rate.
        rate: u32,
    }

    static LIMIT: AtomicU32 = AtomicU32::new(99);
    static RATE: AtomicU32 = AtomicU32::new(99);

    struct World;

    impl Mod for World {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_settings(|limits: &Limits| LIMIT.store(limits.limit, Ordering::Relaxed))?;
            load.on_settings_change(|rates: &Rates| RATE.store(rates.rate, Ordering::Relaxed))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        World,
        r#"
        id = "you.world"
        side = "server"
        "#
    );

    fn values() -> (u32, u32) {
        (LIMIT.load(Ordering::Relaxed), RATE.load(Ordering::Relaxed))
    }

    #[test]
    fn settings_come_as_each_world_loads_and_follow_changes_only_where_asked() {
        let loader = enshwick::test::FakeHost::new()
            .settings("limit = 4\n")
            .settings_file("rates", "rate = 2\n")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(World)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(values(), (0, 0), "init runs on the defaults");
        assert!(loader.config_schema("settings").is_some());
        assert!(loader.config_schema("rates").is_some());

        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 2);
        assert_eq!(values(), (4, 2), "the world's values as it loads");

        loader.set_settings("settings", "limit = 9\n");
        loader.set_settings("rates", "rate = 5\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(
            values(),
            (4, 5),
            "only the mod that follows changes sees one"
        );

        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 2);
        assert_eq!(values(), (9, 5), "the next world load applies the change");
    }

    #[test]
    fn a_file_that_does_not_read_or_check_never_turns_the_mod_off() {
        let loader = enshwick::test::FakeHost::new()
            .settings("limit = \"many\"\n")
            .settings_file("rates", "rate = 2\n")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(World)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 2);
        assert_eq!(
            values(),
            (0, 2),
            "a value that does not read takes its default"
        );
        assert!(
            loader
                .logs()
                .iter()
                .any(|(level, line)| *level == Level::WARN
                    && line == "settings.toml: limit takes a whole number; the default applies")
        );

        loader.set_settings("settings", "limit = 6\n");
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 2);
        assert_eq!(values().0, 6);
        loader.set_settings("settings", "limit = 13\n");
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 2);
        assert_eq!(values().0, 0, "a refused file reads as the defaults");
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }
}
