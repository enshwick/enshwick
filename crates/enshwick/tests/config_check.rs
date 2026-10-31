//! The settings check the loader runs before it writes a change.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::config::ConfigError;
    use enshwick::{Load, Mod, Off};

    /// Altar settings.
    #[enshwick::config(validate = Settings::check)]
    #[derive(Default)]
    struct Settings {
        /// Altars at the first level.
        #[config(range = 1..=20)]
        first: u32,
        /// Altars at the last level.
        #[config(range = 1..=20)]
        last: u32,
    }

    impl Settings {
        fn check(&self) -> Result<(), ConfigError> {
            if self.first > self.last {
                return Err("first is more than last".into());
            }
            if self.last == 20 {
                return Err(ConfigError::text("config.last.max", &[("max", "19")]));
            }
            Ok(())
        }
    }

    struct Altars;

    impl Mod for Altars {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _settings: Settings = load.config()?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Altars,
        r#"
        id = "you.altars"
        side = "server"
        "#
    );

    #[test]
    fn the_loader_checks_a_change_with_the_mods_own_check() {
        let loader = enshwick::test::FakeHost::new()
            .settings("first = 1\nlast = 4\n")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Altars)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.check_settings("first = 2\nlast = 4\n"), Ok(()));
        assert_eq!(
            loader.check_settings("first = 5\nlast = 4\n"),
            Err(enshwick::test::SettingsRefused {
                reason: "first is more than last".to_owned()
            })
        );
        assert_eq!(
            loader.check_settings("first = 2\nlast = 20\n"),
            Err(enshwick::test::SettingsRefused {
                reason: "config.last.max\tmax=19".to_owned()
            })
        );
        assert!(loader.check_settings("first = 50\nlast = 60\n").is_err());
        assert!(loader.check_settings("first = \"many\"").is_err());
    }
}
