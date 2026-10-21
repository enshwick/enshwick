//! A world's settings apply when it loads, the ones that wait for a world
//! too; the ones that wait for a start of the game keep their first values.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::{Load, Mod, Off, events};

    /// Limits.
    #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
    struct Settings {
        /// Read once per world.
        #[config(startup)]
        limit: u32,
        /// Applied live.
        rate: u32,
        /// Read as the game starts.
        #[config(restart)]
        workers: u32,
    }

    static LIMIT: AtomicU32 = AtomicU32::new(0);
    static RATE: AtomicU32 = AtomicU32::new(0);
    static WORKERS: AtomicU32 = AtomicU32::new(0);

    struct Limits;

    impl Mod for Limits {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "limits", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let settings: Settings = load.config()?;
            LIMIT.store(settings.limit, Ordering::Relaxed);
            load.on_config_change(|settings: Settings| {
                LIMIT.store(settings.limit, Ordering::Relaxed);
                RATE.store(settings.rate, Ordering::Relaxed);
                WORKERS.store(settings.workers, Ordering::Relaxed);
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Limits);

    #[test]
    fn startup_settings_wait_for_the_next_world() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .settings("limit = 4\nrate = 1\nworkers = 2\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(LIMIT.load(Ordering::Relaxed), 4);
        loader.set_settings("limit = 9\nrate = 2\nworkers = 8\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(LIMIT.load(Ordering::Relaxed), 4);
        assert_eq!(RATE.load(Ordering::Relaxed), 2);
        assert_eq!(WORKERS.load(Ordering::Relaxed), 2);
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
        assert_eq!(LIMIT.load(Ordering::Relaxed), 9);
        assert_eq!(
            WORKERS.load(Ordering::Relaxed),
            2,
            "a world does not restart"
        );
        loader.set_settings("limit = 3\nrate = 2\nworkers = 8\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(LIMIT.load(Ordering::Relaxed), 9);
        assert_eq!(WORKERS.load(Ordering::Relaxed), 2);
    }
}
