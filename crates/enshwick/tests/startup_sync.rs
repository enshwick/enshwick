//! A `startup, sync` setting: applied when a world loads, and a joined
//! client runs with the server's value, which stays while the world runs.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off, events};

    /// World rules.
    #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
    struct Settings {
        /// The world's rule, the same for its players.
        #[config(startup, sync)]
        rule: bool,
    }

    static RULE: AtomicBool = AtomicBool::new(false);

    struct Rules;

    impl Mod for Rules {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "rules", side: BOTH, join: true)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let settings: Settings = load.config()?;
            RULE.store(settings.rule, Ordering::Relaxed);
            load.on_config_change(|settings: Settings| {
                RULE.store(settings.rule, Ordering::Relaxed);
            })?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Rules);

    fn rule() -> bool {
        RULE.load(Ordering::Relaxed)
    }

    #[test]
    fn a_server_reports_the_value_in_force_until_the_next_world() {
        let loader = enshwick::test::FakeHost::new()
            .settings("rule = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(rule());
        loader.set_settings("rule = false\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert!(rule(), "the world keeps its value");
        assert_eq!(
            loader.reported_synced().last().map(String::as_str),
            Some("{ rule = true }"),
            "joined players keep the value in force"
        );
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
        assert!(!rule());
        assert_eq!(
            loader.reported_synced().last().map(String::as_str),
            Some("{ rule = false }")
        );
    }

    #[test]
    fn a_joined_client_runs_with_the_server_s_value() {
        let loader = enshwick::test::FakeHost::new()
            .side(enshwick::abi::Side::CLIENT)
            .settings("rule = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(rule());
        // Joining a server whose world runs with the rule off.
        loader.set_server_synced(Some("{ rule = false }"));
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert!(!rule(), "the server's value over the file's");
        // The client's own file changes while it plays there.
        loader.set_settings("rule = true\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert!(!rule(), "still the server's");
        // Leaving: the file's value from the start again.
        loader.set_server_synced(None);
        loader.set_settings("rule = false\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert!(rule(), "the value in force before the join");
    }
}
