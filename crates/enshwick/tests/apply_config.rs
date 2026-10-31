//! A mod applies its settings once at init and again after every change.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::Status;
    use enshwick::events;
    use enshwick::{Load, Mod, Off};

    static APPLIED: AtomicU32 = AtomicU32::new(0);
    static CALLS: AtomicU32 = AtomicU32::new(0);
    static RETURNED: AtomicU32 = AtomicU32::new(0);

    /// The limit's settings.
    #[enshwick::config]
    #[derive(Default)]
    struct Settings {
        /// The limit.
        limit: u32,
    }

    struct Limit;

    impl Mod for Limit {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let settings = load.apply_config(|settings: &Settings| {
                APPLIED.store(settings.limit, Ordering::Relaxed);
                CALLS.fetch_add(1, Ordering::Relaxed);
            })?;
            RETURNED.store(settings.limit, Ordering::Relaxed);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Limit,
        r#"
        id = "you.limit"
        side = "server"
        "#
    );

    #[test]
    fn settings_apply_at_init_and_after_each_change_and_come_back() {
        let loader = enshwick::test::FakeHost::new()
            .settings("limit = 4\n")
            .install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Limit)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        assert_eq!(RETURNED.load(Ordering::Relaxed), 4);
        assert_eq!(APPLIED.load(Ordering::Relaxed), 4);
        assert_eq!(CALLS.load(Ordering::Relaxed), 1);

        loader.set_settings("limit = 9\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(APPLIED.load(Ordering::Relaxed), 9);
        assert_eq!(CALLS.load(Ordering::Relaxed), 2);
    }
}
