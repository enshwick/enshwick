//! A mod writes a value of its own world settings, such as a password it
//! generates: checked as an edit is, written to the world's file, raised as a
//! change and read back at the next load, a secret never logged.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::config::{self, SetSettingError};
    use enshwick::{Load, Mod, Off, events};

    /// Remote console.
    #[enshwick::config(validate = Remote::check)]
    #[derive(Default)]
    struct Remote {
        /// The password.
        #[config(secret)]
        password: String,
        /// How many may connect.
        #[config(range = 1..=16)]
        limit: u32,
    }

    impl Remote {
        fn check(&self) -> Result<(), String> {
            (self.limit != 13)
                .then_some(())
                .ok_or_else(|| "13 is unlucky".to_owned())
        }
    }

    static SEEN: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

    struct Console;

    impl Mod for Console {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_settings_change(|remote: &Remote| {
                SEEN.lock()
                    .unwrap()
                    .push((remote.password.clone(), remote.limit));
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Console,
        r#"
        id = "you.console"
        side = "server"
        "#
    );

    fn last_seen() -> Option<(String, u32)> {
        SEEN.lock().unwrap().last().cloned()
    }

    #[test]
    fn a_generated_secret_is_written_heard_at_once_read_at_the_next_load_and_never_logged() {
        let loader = enshwick::test::FakeHost::new()
            .world_key("world0")
            .settings("limit = 4\n")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Console)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        config::set::<Remote>("password", "hunter2").unwrap();
        assert_eq!(last_seen(), Some(("hunter2".to_owned(), 4)));
        let text = loader.settings_text("settings").unwrap();
        assert!(text.contains("password = \"hunter2\""), "{text}");
        assert!(text.contains("limit = 4"), "{text}");

        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
        assert_eq!(last_seen(), Some(("hunter2".to_owned(), 4)));
        assert!(
            loader
                .logs()
                .iter()
                .all(|(_, line)| !line.contains("hunter2")),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_refused_value_writes_nothing_and_says_why() {
        let loader = enshwick::test::FakeHost::new()
            .world_key("world0")
            .settings("limit = 4\n")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Console)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let heard = SEEN.lock().unwrap().len();

        assert_eq!(
            config::set::<Remote>("limit", 40),
            Err(SetSettingError::BadValue)
        );
        assert_eq!(
            config::set::<Remote>("limit", "many"),
            Err(SetSettingError::BadValue)
        );
        assert_eq!(
            config::set::<Remote>("limit", 13),
            Err(SetSettingError::Refused)
        );
        assert_eq!(
            config::set::<Remote>("nope", 1),
            Err(SetSettingError::NoSetting)
        );
        assert_eq!(
            loader.settings_text("settings").as_deref(),
            Some("limit = 4\n")
        );
        assert_eq!(SEEN.lock().unwrap().len(), heard, "no change was raised");

        config::set::<Remote>("limit", 6).unwrap();
        assert!(
            loader
                .settings_text("settings")
                .unwrap()
                .contains("limit = 6")
        );
    }

    #[test]
    fn nothing_is_written_while_no_world_runs() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Console)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            config::set::<Remote>("password", "x"),
            Err(SetSettingError::NoWorld)
        );
        assert!(loader.settings_text("settings").is_none());
    }
}
