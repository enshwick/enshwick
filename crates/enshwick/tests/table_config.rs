//! Settings described by a schema at run time: the world's file, its
//! reading and the loader's check, as a typed `Config` has them.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::config::{Field, Schema};
    use enshwick::{Load, Mod, Off, events};

    static VALUES: Mutex<Option<toml::Table>> = Mutex::new(None);

    fn schema() -> Schema {
        Schema::new(
            "Greeter settings.",
            "config",
            vec![
                Field::new("greeting", "hello").with_doc("What to say.", ""),
                Field::new("limit", &4)
                    .with_range(Some(1.0), Some(16.0))
                    .with_doc("How many.", ""),
            ],
        )
    }

    struct Greeter;

    impl Mod for Greeter {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let (settings, defaults) = load.table_config(schema())?;
            *VALUES.lock().unwrap() = Some(defaults);
            settings.on_change(load, |values| *VALUES.lock().unwrap() = Some(values))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Greeter,
        r#"
        id = "you.greeter"
        side = "server"
        "#
    );

    fn value(key: &str) -> toml::Value {
        VALUES.lock().unwrap().as_ref().unwrap()[key].clone()
    }

    fn start(settings: Option<&str>) -> enshwick::test::Installed {
        let mut host = enshwick::test::FakeHost::new();
        if let Some(text) = settings {
            host = host.settings(text);
        }
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Greeter)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn init_runs_on_the_defaults_and_hands_the_loader_the_schema() {
        let loader = start(None);
        assert_eq!(value("greeting").as_str(), Some("hello"));
        assert_eq!(loader.config_schema("settings"), Some(schema()));
        let template = schema().template();
        assert!(
            template.contains("# What to say.\ngreeting = \"hello\"\n"),
            "{template}"
        );
    }

    #[test]
    fn the_world_s_file_reads_with_every_missing_or_bad_value_at_its_default() {
        let loader = start(Some("limit = 40\ngreeting = 5\n"));
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
        assert_eq!(value("limit").as_integer(), Some(16));
        assert_eq!(value("greeting").as_str(), Some("hello"));
    }

    #[test]
    fn a_table_follows_its_file_while_the_world_runs() {
        let loader = start(Some("limit = 4\n"));
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
        loader.set_settings("settings", "greeting = \"hi\"\nlimit = 8\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(value("greeting").as_str(), Some("hi"));
        assert_eq!(value("limit").as_integer(), Some(8));
    }

    #[test]
    fn the_loader_s_check_refuses_a_value_the_schema_refuses() {
        let loader = start(None);
        assert!(loader.check_settings("settings", "limit = 99").is_err());
        assert!(loader.check_settings("settings", "limit = 9").is_ok());
    }
}
