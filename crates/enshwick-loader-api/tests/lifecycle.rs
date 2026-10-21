//! A mod's whole life against the fake loader: init, settings, hooks, a panic
//! inside a detour, a settings change.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick_loader_api::abi::{
        Event, Header, HookFlags, Level, ManifestFlags, ModHandle, ModInfo, Side, Status, Str,
        UpdateSource,
    };
    use enshwick_loader_api::mods::Updates;
    use enshwick_loader_api::test::{FakeHost, address};
    use enshwick_loader_api::{Next, events, hook};

    /// Enshwick's release key, which the loader's entry names.
    const KEY: &str = "RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF";

    static ORIGINAL_CALLS: AtomicU32 = AtomicU32::new(0);
    static LIVE_LIMIT: AtomicU32 = AtomicU32::new(0);

    fn evaluated() -> u32 {
        panic!("a filtered line evaluates nothing")
    }

    extern "system" fn original(value: u32) -> u32 {
        ORIGINAL_CALLS.fetch_add(1, Ordering::Relaxed);
        value + 1
    }

    extern "system" fn tripled(value: u32) -> u32 {
        value * 3
    }

    enshwick_loader_api::detour! {
        static DOUBLE: extern "system" fn(value: u32) -> u32 = double, priority = -5;
    }

    fn double(next: Next<extern "system" fn(u32) -> u32>, value: u32) -> u32 {
        assert_ne!(value, 13, "unlucky");
        next.get()(value) * 2
    }

    #[hook(priority = 5, flags = HookFlags::EXCLUSIVE)]
    fn halve(next: Next<unsafe extern "system" fn(u32) -> u32>, value: u32) -> u32 {
        // SAFETY: the original takes the value the game passed.
        unsafe { next.get()(value) / 2 }
    }

    #[derive(serde::Deserialize)]
    struct Settings {
        limit: u32,
    }

    /// Test settings.
    #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
    struct Live {
        /// A limit, the same on the server and its players.
        #[config(sync)]
        limit: u32,
    }

    struct Test {
        limit: u32,
    }

    impl enshwick_loader_api::Mod for Test {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "test-mod", side: BOTH, join: true, requires: ["other" => "^1"])
        }

        fn init(
            load: &mut enshwick_loader_api::Load<'_>,
        ) -> Result<Self, enshwick_loader_api::Off> {
            let settings: Settings = load.settings("limit = 1\n")?;
            assert_eq!(load.symbol("npc.lookup")?, 0x1_4020_2e80);
            assert!(load.symbol("nope").is_err());
            // SAFETY: the fake's symbol is a function of the hook's type.
            unsafe { load.hook(&DOUBLE, load.system("anything")?) }?;
            // SAFETY: the fake's symbol is a function of the hook's type.
            unsafe { load.hook(&HALVE, load.symbol("tripled")?) }?;
            enshwick_loader_api::info!("limit {}", settings.limit);
            enshwick_loader_api::debug!("{}", evaluated());
            for _ in 0..3 {
                enshwick_loader_api::warn_once!("once");
                enshwick_loader_api::warn_every!(std::time::Duration::from_secs(3600), "every");
            }
            load.on_config_change(|live: Live| LIVE_LIMIT.store(live.limit, Ordering::Relaxed))?;
            Ok(Self {
                limit: settings.limit,
            })
        }
    }

    enshwick_loader_api::declare_mod!(Test);

    #[test]
    fn a_mod_lives_its_whole_life() {
        let manifest = enshwick_mod_query();
        // SAFETY: the manifest is static.
        let manifest = unsafe { &*manifest };
        // SAFETY: static strings.
        assert_eq!(unsafe { manifest.id.as_str() }, Some("test-mod"));
        assert_eq!(manifest.dependency_count, 1);
        assert_eq!(enshwick_mod_abi(), 1);

        let target = address(original as extern "system" fn(u32) -> u32);
        let loader = FakeHost::new()
            .symbol("npc.lookup", 0x1_4020_2e80)
            .symbol(
                "tripled",
                address(tripled as extern "system" fn(u32) -> u32),
            )
            .system("anything", target)
            .settings("limit = 7\n")
            .log_level(Level::WARN)
            .loaded(ModInfo {
                header: Header::of::<ModInfo>(),
                id: Str::new("enshwick"),
                version: Str::new("0.2.0"),
                side: Side::BOTH,
                flags: ManifestFlags::NONE,
                off: 0,
                file: Str::new("mods/enshwick/enshwick.dll"),
                updates: UpdateSource::github("themaiby/enshwick", "enshwick-loader"),
                signed_by: [Str::new(KEY), Str::new("RWnot-a-key")],
                ..ModInfo::EMPTY
            })
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.handle(), ModHandle(1));
        assert_eq!(Test::instance().map(|t| t.limit), Some(7));
        assert_eq!(
            enshwick_loader_api::host().map(|h| h.build().side),
            Some(Side::SERVER)
        );
        let listed = enshwick_loader_api::mods();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].file, "mods/enshwick/enshwick.dll");
        assert_eq!(
            listed[0].updates.as_ref().map(Updates::to_line).as_deref(),
            Some("github:themaiby/enshwick:enshwick-loader")
        );
        assert_eq!(listed[0].signed_by, [KEY]);
        let hooks = loader.hooks();
        assert_eq!(hooks[0].target, target);
        assert_eq!(hooks[0].priority, -5);
        assert_eq!(
            (hooks[1].priority, hooks[1].flags),
            (5, HookFlags::EXCLUSIVE)
        );
        // The host keeps `warn` and above: `info` is filtered, repeats are not.
        let messages: Vec<String> = loader.logs().into_iter().map(|(_, m)| m).collect();
        assert_eq!(messages, ["once", "every"]);

        // The chain: the detour doubles what the original returns.
        let entry = loader.detour(&DOUBLE).unwrap();
        assert_eq!(entry(2), 6);
        let halve = loader.detour(&HALVE).unwrap();
        // SAFETY: the chain ends in `tripled`, which takes any value.
        assert_eq!(unsafe { halve(4) }, 6);

        // A panic in the detour reports a fault and falls through to the original.
        let calls = ORIGINAL_CALLS.load(Ordering::Relaxed);
        assert_eq!(entry(13), 14);
        assert_eq!(ORIGINAL_CALLS.load(Ordering::Relaxed), calls + 1);
        assert_eq!(loader.faults(), ["panic in hook DOUBLE"]);

        // SAFETY: as for init.
        unsafe { loader.ready(enshwick_mod_ready) };

        // A settings change reaches the handler; a broken file is skipped.
        let watched = [
            Event::LOADER_SETTINGS_CHANGED,
            Event::WORLD_SETTINGS,
            Event::SETTINGS_CHANGED,
        ];
        assert_eq!(loader.subscriptions(), watched);
        loader.set_settings("limit = 9\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(LIVE_LIMIT.load(Ordering::Relaxed), 9);
        loader.set_settings("limit = \"x\"\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(LIVE_LIMIT.load(Ordering::Relaxed), 9);
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.starts_with("settings change skipped"))
        );
        assert_eq!(loader.raise::<events::LoaderSettingsChanged>(&()), 1);

        // A joined server's join values reach the handler over the file's,
        // and the file's come back without them.
        loader.set_settings("limit = 9\n");
        loader.set_server_synced(Some("{ limit = 12 }"));
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(LIVE_LIMIT.load(Ordering::Relaxed), 12);
        assert_eq!(
            loader.reported_synced().last().map(String::as_str),
            Some("{ limit = 9 }")
        );
        loader.set_server_synced(Some("{ limit = 'x' }"));
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(LIVE_LIMIT.load(Ordering::Relaxed), 9);
        loader.set_server_synced(None);
        loader.set_settings("limit = 8\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(LIVE_LIMIT.load(Ordering::Relaxed), 8);
    }
}
