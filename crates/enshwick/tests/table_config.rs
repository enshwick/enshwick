//! Settings described by a schema at run time: the file, the checks, the
//! stages and the join values, as a typed `Config` has them.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::config::{Field, Schema, Stage};
    use enshwick::{Load, Mod, Off, events};

    static VALUES: Mutex<Option<toml::Table>> = Mutex::new(None);

    fn schema() -> Schema {
        Schema::new(
            "Greeter settings.",
            "config",
            vec![
                Field::new("greeting", Stage::Runtime, "hello").with_doc("What to say.", ""),
                Field::new("limit", Stage::Startup, &4)
                    .with_range(Some(1.0), Some(16.0))
                    .with_doc("How many.", ""),
                Field::new("rule", Stage::Sync, &false).with_doc("The world's rule.", ""),
                Field::new("level", Stage::Sync, &1)
                    .with_range(Some(1.0), Some(5.0))
                    .with_doc("The world's level.", ""),
                Field::new("cache", Stage::Restart, &16).with_doc("Kept at start.", ""),
                Field::new("colour", Stage::Client, "red").with_doc("This player's own.", ""),
            ],
        )
    }

    struct Greeter;

    impl Mod for Greeter {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.greeter", side: BOTH, join: true)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let (settings, values) = load.table_config(schema())?;
            *VALUES.lock().unwrap() = Some(values);
            settings.on_change(load, |values| *VALUES.lock().unwrap() = Some(values))?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Greeter);

    fn value(key: &str) -> toml::Value {
        VALUES.lock().unwrap().as_ref().unwrap()[key].clone()
    }

    fn start(settings: Option<&str>) -> (enshwick::test::Installed, Status) {
        let mut host = enshwick::test::FakeHost::new();
        if let Some(text) = settings {
            host = host.settings(text);
        }
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Greeter)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        (loader, status)
    }

    #[test]
    fn a_first_start_writes_the_template_and_reads_the_defaults() {
        let (loader, status) = start(None);
        assert_eq!(status, Status::OK);
        let template = loader.settings_defaults().pop().unwrap();
        assert!(
            template.contains("# What to say.\ngreeting = \"hello\"\n"),
            "{template}"
        );
        assert!(
            template.contains("# Takes effect when the world loads.\nlimit = 4\n"),
            "{template}"
        );
        assert_eq!(value("greeting").as_str(), Some("hello"));
        assert_eq!(loader.config_schema(), Some(schema()));
    }

    #[test]
    fn a_missing_value_reads_as_its_default_and_numbers_are_held_in_range() {
        let (_loader, status) = start(Some("limit = 40\n"));
        assert_eq!(status, Status::OK);
        assert_eq!(value("limit").as_integer(), Some(16));
        assert_eq!(value("greeting").as_str(), Some("hello"));
    }

    #[test]
    fn a_value_of_the_wrong_kind_refuses_the_mod() {
        let (_loader, status) = start(Some("greeting = 5\n"));
        assert_ne!(status, Status::OK);
    }

    #[test]
    fn a_runtime_value_changes_at_once_and_a_startup_one_with_the_next_world() {
        let (loader, _) = start(Some("limit = 4\n"));
        loader.set_settings("greeting = \"hi\"\nlimit = 8\n");
        let _handled = loader.raise::<events::SettingsChanged>(&());
        assert_eq!(value("greeting").as_str(), Some("hi"));
        assert_eq!(value("limit").as_integer(), Some(4));
        let _handled = loader.raise::<events::SettingsReplaced>(&());
        assert_eq!(value("limit").as_integer(), Some(8));
    }

    #[test]
    fn the_loader_s_check_refuses_a_value_the_schema_refuses() {
        let (loader, _) = start(None);
        assert!(loader.check_settings("limit = 99").is_err());
        assert!(loader.check_settings("limit = 9").is_ok());
    }

    #[test]
    fn a_joined_client_runs_with_the_server_s_join_values() {
        let (loader, _) = start(Some("rule = false\n"));
        assert_eq!(
            loader.reported_synced().last().map(String::as_str),
            Some("{ level = 1, rule = false }")
        );
        loader.set_server_synced(Some("{ rule = true }"));
        let _handled = loader.raise::<events::SettingsChanged>(&());
        assert_eq!(value("rule").as_bool(), Some(true));
    }

    #[test]
    fn a_restart_value_holds_through_every_world() {
        let (loader, _) = start(Some("cache = 16\n"));
        loader.set_settings("cache = 32\n");
        let _handled = loader.raise::<events::SettingsReplaced>(&());
        assert_eq!(value("cache").as_integer(), Some(16));
        let _handled = loader.raise::<events::SettingsChanged>(&());
        assert_eq!(value("cache").as_integer(), Some(16));
    }

    #[test]
    fn a_client_value_is_never_reported_to_a_server() {
        let (loader, _) = start(Some("colour = \"blue\"\n"));
        let reported = loader.reported_synced();
        assert!(
            reported.iter().all(|r| !r.contains("colour")),
            "{reported:?}"
        );
    }

    #[test]
    fn server_values_out_of_range_are_refused_and_the_file_s_apply() {
        let (loader, _) = start(Some("level = 2\n"));
        loader.set_server_synced(Some("{ level = 9 }"));
        let _handled = loader.raise::<events::SettingsChanged>(&());
        assert_eq!(value("level").as_integer(), Some(2));
    }
}
