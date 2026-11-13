//! Two data packs in one DLL, as the loader runs them, keep their own
//! rewards and settings.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, ModHandle, PlayerData, PlayerInfo, Status, TickData};
    use enshwick::events::{PlayerJoined, Ticked, WorldLoaded};
    use enshwick::packs::{Pack, PackFiles};
    use enshwick::progress::Experience;
    use enshwick::test::FakeHost;
    use enshwick::{Entity, PlayerKey};

    /// A pack whose `welcome` reward gives `bonus` experience, through a
    /// setting of that default when `setting`.
    fn pack(id: &str, bonus: u32, setting: bool) -> Pack {
        let (manifest, experience) = if setting {
            (
                format!(
                    "id = \"{id}\"\nversion = \"1.0.0\"\n\n[[setting]]\nkey = \"bonus\"\ndefault = {bonus}\nmin = 0\n"
                ),
                "{ setting = \"bonus\" }".to_owned(),
            )
        } else {
            (
                format!("id = \"{id}\"\nversion = \"1.0.0\"\n"),
                bonus.to_string(),
            )
        };
        let rewards = format!(
            "[[reward]]\nid = \"welcome\"\non = {{ join = true }}\ngive = [{{ experience = {experience} }}]\n"
        );
        let files = PackFiles::new()
            .with("pack.toml", manifest)
            .with("rewards.toml", rewards);
        Pack::parse_with(id, &files, None).unwrap()
    }

    #[test]
    fn two_packs_keep_their_own_rewards_and_settings() {
        let data =
            std::env::temp_dir().join(format!("enshwick-pack-handles-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let loader = FakeHost::new()
            .world()
            .player(PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: 0,
                entity: Entity(2),
                key: PlayerKey::new(7),
                permissions: 0,
                name_len: 0,
                name: [0; 64],
            })
            .data_dir(data.to_str().unwrap())
            .world_key("world0")
            .install();
        for (me, id, bonus, setting) in [
            (ModHandle(1), "you.first", 5, true),
            (ModHandle(2), "you.second", 9, false),
        ] {
            let pack = pack(id, bonus, setting);
            // SAFETY: the fake host's table lives for the process.
            let status = unsafe {
                enshwick::__private::init_pack(
                    loader.api(),
                    me,
                    enshwick::abi::Side::SERVER,
                    |load| {
                        load.add_pack(&pack)?;
                        let declared = enshwick::packs::declared_settings(load).map(|(_, v)| v);
                        let bonus = declared.and_then(|v| v.get("bonus")?.as_integer());
                        assert_eq!(
                            bonus,
                            setting.then_some(5),
                            "{id} sees only its own settings"
                        );
                        Ok(())
                    },
                )
            };
            assert_eq!(status, Status::OK, "{id}: {:?}", loader.logs());
        }
        assert_eq!(
            loader.raise::<WorldLoaded>(&()),
            4,
            "each pack's settings and rewards"
        );
        let joined = PlayerData {
            header: Header::of::<PlayerData>(),
            slot: 0,
            entity: Entity(2),
            key: PlayerKey::new(7),
        };
        let _heard = loader.raise::<PlayerJoined>(&joined);
        let _ran = loader.raise::<Ticked>(&TickData::new(1));
        let mut rows = [Experience {
            entity: Entity(2),
            level: 1,
            progress: 0,
            need: 1000,
            gained: 0,
        }];
        loader.experience(&mut rows, 25, |level| level * 1000);
        assert_eq!(rows[0].gained, 14, "each pack's welcome, at its own bonus");
        let _gone = std::fs::remove_dir_all(&data);
    }

    fn handle() -> Option<ModHandle> {
        enshwick::host().map(|host| host.handle())
    }

    #[test]
    fn a_pack_s_init_and_later_calls_see_the_pack_as_their_host() {
        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process; the loader
        // makes its own handle the host of the copy that runs the packs.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(
                loader.api(),
                ModHandle(1),
                enshwick::abi::Side::SERVER,
                |_| {
                    assert_eq!(handle(), Some(ModHandle(1)), "init runs as the pack");
                    Ok(())
                },
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            handle(),
            Some(ModHandle(9)),
            "the loader's again after init"
        );
        let nested = || {
            // SAFETY: the fake host's table lives for the process.
            let nested =
                unsafe { enshwick_base::__internal::as_pack(loader.api(), ModHandle(3), handle) };
            (handle(), nested)
        };
        // SAFETY: the fake host's table lives for the process.
        let inner =
            unsafe { enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), nested) };
        assert_eq!(inner, (Some(ModHandle(2)), Some(ModHandle(3))));
        assert_eq!(handle(), Some(ModHandle(9)));
    }

    #[test]
    fn a_pack_s_panic_leaves_the_loader_as_the_host() {
        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        let panicked = std::panic::catch_unwind(|| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                    panic!("the pack's builder")
                })
            }
        });
        assert!(panicked.is_err());
        assert_eq!(handle(), Some(ModHandle(9)));
    }

    #[test]
    fn a_pack_s_log_line_goes_out_under_the_pack_s_handle() {
        use enshwick::abi::Level;

        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        unsafe {
            enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                enshwick::warn!("from the pack");
            });
        };
        enshwick::warn!("from the loader");
        assert_eq!(
            loader.sent(),
            [
                (ModHandle(2), Level::WARN, "from the pack".to_owned()),
                (ModHandle(9), Level::WARN, "from the loader".to_owned()),
            ]
        );
    }

    #[test]
    fn a_pack_draws_its_window_and_hud_under_its_own_handle() {
        use enshwick::abi::Side;
        use enshwick::test::Drawn;
        use enshwick::ui::Anchor;

        let loader = FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: the fake host's table lives for the process; the loader
        // makes its own handle the host of the copy that runs the packs.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: `init_pack` needs a host table that lives for the process,
        // which the installed fake host's is.
        let status = unsafe {
            enshwick::__private::init_pack(loader.api(), ModHandle(1), Side::SERVER, |load| {
                load.on_frame(|frame| {
                    frame.window("pack", "Pack", |w| w.label("framed"));
                })?;
                load.on_hud(|hud| hud.text(Anchor::TopRight, "hud line"))?;
                Ok(())
            })
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let drawn = loader.draw_frame();
        assert!(
            drawn
                .iter()
                .any(|d| matches!(d, Drawn::Label { text, .. } if text == "framed")),
            "the window's label, drawn as the pack: {drawn:?}"
        );
        let hud = loader.draw_hud();
        assert!(
            hud.iter()
                .any(|(_, d)| matches!(d, Drawn::Label { text, .. } if text == "hud line")),
            "the HUD line, drawn as the pack: {hud:?}"
        );
    }

    #[test]
    fn a_pack_s_hud_callback_runs_as_the_pack() {
        use enshwick::abi::Side;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static SEEN: AtomicUsize = AtomicUsize::new(0);
        let loader = FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(loader.api(), ModHandle(1), Side::SERVER, |load| {
                load.on_hud(|_| SEEN.store(handle().map_or(0, |h| h.0), Ordering::Relaxed))?;
                Ok(())
            })
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let _drawn = loader.draw_hud();
        assert_eq!(SEEN.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_pack_s_debug_line_passes_at_the_pack_s_level_while_the_loader_keeps_info() {
        use enshwick::abi::Level;

        let loader = FakeHost::new()
            .pack_log_level(ModHandle(2), Level::DEBUG)
            .install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        unsafe {
            enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                enshwick::debug!("from the pack");
            });
        };
        enshwick::debug!("from the loader");
        assert_eq!(
            loader.sent(),
            [(ModHandle(2), Level::DEBUG, "from the pack".to_owned())]
        );
    }

    #[test]
    fn a_pack_s_debug_line_is_dropped_at_the_pack_s_level_while_the_loader_keeps_debug() {
        use enshwick::abi::Level;

        let loader = FakeHost::new()
            .log_level(Level::DEBUG)
            .pack_log_level(ModHandle(2), Level::DEBUG)
            .pack_log_level(ModHandle(2), Level::WARN)
            .pack_log_level(ModHandle(3), Level::DEBUG)
            .install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        unsafe {
            enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                enshwick::debug!("from the pack");
            });
        };
        enshwick::debug!("from the loader");
        assert_eq!(
            loader.sent(),
            [(ModHandle(9), Level::DEBUG, "from the loader".to_owned())]
        );
    }

    #[test]
    fn a_pack_keeps_its_level_until_the_loader_s_settings_change() {
        use enshwick::abi::Level;
        use enshwick::events::LoaderSettingsChanged;

        let loader = FakeHost::new()
            .pack_log_level(ModHandle(2), Level::WARN)
            .install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(
                loader.api(),
                ModHandle(2),
                enshwick::abi::Side::SERVER,
                |_| Ok(()),
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let debug = |line: &'static str| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                    enshwick::debug!("{line}");
                });
            }
        };
        debug("at warn");
        loader.set_pack_log_level(ModHandle(2), Level::DEBUG);
        debug("kept at warn");
        assert_eq!(
            loader.raise::<LoaderSettingsChanged>(&()),
            0,
            "the loader refreshes the pack; the pack subscribes to nothing"
        );
        debug("at debug");
        assert_eq!(
            loader.sent(),
            [(ModHandle(2), Level::DEBUG, "at debug".to_owned())],
            "the level is read once, and again after the change"
        );
    }

    #[test]
    fn the_loader_s_own_level_follows_its_settings_also_inside_a_pack_s_call() {
        use enshwick::abi::Level;
        use enshwick::events::LoaderSettingsChanged;

        let loader = FakeHost::new()
            .pack_log_level(ModHandle(2), Level::INFO)
            .install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        let as_loader_in_pack = |line: &'static str| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                    enshwick_base::__internal::as_loader(|| enshwick::debug!("{line}"));
                });
            }
        };
        as_loader_in_pack("at info");
        loader.set_log_level(Level::DEBUG);
        assert_eq!(loader.raise::<LoaderSettingsChanged>(&()), 0);
        as_loader_in_pack("at debug");
        assert_eq!(
            loader.sent(),
            [(ModHandle(9), Level::DEBUG, "at debug".to_owned())]
        );
    }

    #[test]
    fn a_poisoned_lock_in_a_pack_s_call_is_logged_as_the_pack_not_to_the_loader_s_sink() {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, Ordering};

        use enshwick::Relock as _;
        use enshwick::abi::Level;

        static SUNK: AtomicBool = AtomicBool::new(false);
        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        enshwick_base::__internal::report_poison_to(|_| SUNK.store(true, Ordering::Relaxed));
        let lock = Mutex::new(0);
        let _panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = lock.lock();
            panic!("while holding");
        }));
        // SAFETY: the fake host's table lives for the process.
        unsafe {
            enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || drop(lock.relock()));
        }
        assert!(!SUNK.load(Ordering::Relaxed), "the loader's sink heard it");
        let sent = loader.sent();
        assert!(
            sent.iter().any(|(me, level, line)| *me == ModHandle(2)
                && *level == Level::WARN
                && line.contains("a thread panicked holding this lock")),
            "{sent:?}"
        );
    }

    #[test]
    fn a_pack_s_level_read_in_one_fake_life_is_read_again_in_the_next() {
        use enshwick::abi::Level;

        let debug_in = |level: Level| {
            let loader = FakeHost::new()
                .pack_log_level(ModHandle(2), level)
                .install();
            // SAFETY: the fake host's table lives for the process.
            unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                    enshwick::debug!("from the pack");
                });
            };
            loader.sent().len()
        };
        assert_eq!(debug_in(Level::DEBUG), 1);
        assert_eq!(debug_in(Level::WARN), 0, "the last life's level is gone");
    }

    #[test]
    fn a_once_and_an_every_site_log_for_each_pack_that_reaches_them() {
        use std::time::Duration;

        fn sites() {
            enshwick::info_once!("first time");
            enshwick::warn_every!(Duration::from_secs(3600), "now and then");
        }

        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        for pack in [ModHandle(2), ModHandle(2), ModHandle(3)] {
            // SAFETY: the fake host's table lives for the process.
            unsafe { enshwick_base::__internal::as_pack(loader.api(), pack, sites) };
        }
        sites();
        sites();
        let by: Vec<ModHandle> = loader.sent().into_iter().map(|(by, ..)| by).collect();
        assert_eq!(
            by,
            [
                ModHandle(2),
                ModHandle(2),
                ModHandle(3),
                ModHandle(3),
                ModHandle(9),
                ModHandle(9),
            ],
            "each pack and the loader once per site: {:?}",
            loader.sent()
        );
    }

    #[test]
    fn a_pack_s_event_and_tick_handlers_run_as_the_pack() {
        use enshwick::abi::Side;
        use std::time::Duration;

        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(loader.api(), ModHandle(1), Side::SERVER, |load| {
                load.on::<WorldLoaded>(|()| assert_eq!(handle(), Some(ModHandle(1))))?;
                load.on::<Ticked>(|_| assert_eq!(handle(), Some(ModHandle(1))))?;
                Ok(())
            })
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);
        assert_eq!(loader.tick(Duration::from_millis(50)), 1);
        assert_eq!(handle(), Some(ModHandle(9)), "the loader's after the calls");
    }

    #[test]
    fn a_pack_s_panicking_handler_faults_the_pack_and_not_the_loader() {
        use enshwick::abi::Level;

        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(
                loader.api(),
                ModHandle(2),
                enshwick::abi::Side::SERVER,
                |load| {
                    load.on::<WorldLoaded>(|()| panic!("the pack's"))?;
                    Ok(())
                },
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let before = loader.sent().len();
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);
        assert_eq!(
            loader.sent()[before..],
            [(
                ModHandle(2),
                Level::ERROR,
                "off: panic in an event handler: the pack's".to_owned()
            )]
        );
    }

    #[test]
    fn a_thread_spawned_in_a_pack_s_scope_runs_as_the_pack() {
        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        let (sender, seen) = std::sync::mpsc::channel();
        // SAFETY: the fake host's table lives for the process.
        let spawned = unsafe {
            enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                enshwick_core::__fake::spawn("pack-thread", move || {
                    let _sent = sender.send(handle());
                })
            })
        };
        spawned.expect("a thread").join().expect("no panic");
        assert_eq!(seen.recv().ok(), Some(Some(ModHandle(2))));
    }

    /// A site state whose making panics, as a panic in `Box::new` would.
    #[derive(Debug)]
    struct Refused;

    impl Default for Refused {
        fn default() -> Self {
            panic!("no state")
        }
    }

    #[test]
    fn a_pack_s_site_after_a_poisoned_lock_logs_the_lock_and_works() {
        use enshwick::__private::Once;
        use enshwick_core::__fake::for_pack;

        static BROKEN: Refused = Refused;
        static SITE: Once = Once::new();
        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        let panicked = std::panic::catch_unwind(|| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || {
                    let _state = for_pack(&BROKEN);
                });
            }
        });
        assert!(panicked.is_err());
        // SAFETY: the fake host's table lives for the process.
        let first = unsafe {
            enshwick_base::__internal::as_pack(loader.api(), ModHandle(2), || SITE.first())
        };
        assert!(first);
        assert!(
            loader
                .sent()
                .iter()
                .any(|(by, _, line)| *by == ModHandle(2) && line.contains("panicked holding")),
            "{:?}",
            loader.sent()
        );
    }

    /// The site a [`Reentering`] state asks for while it is made.
    static INNER: enshwick::__private::Once = enshwick::__private::Once::new();
    /// Whether that inner ask got the shared site back.
    static INNER_SHARED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

    /// A site state whose making reaches the pack's sites again, as a line
    /// logged while the states' lock is held does.
    #[derive(Debug)]
    struct Reentering;

    impl Default for Reentering {
        fn default() -> Self {
            let inner = enshwick_core::__fake::for_pack(&INNER);
            INNER_SHARED.store(
                u8::from(core::ptr::eq(inner, &raw const INNER)),
                std::sync::atomic::Ordering::Relaxed,
            );
            Self
        }
    }

    #[test]
    fn a_site_reached_again_while_the_states_lock_is_held_takes_the_shared_state() {
        use enshwick_core::__fake::for_pack;

        static OUTER: Reentering = Reentering;
        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        let outer = unsafe {
            enshwick_base::__internal::as_pack(loader.api(), ModHandle(3), || for_pack(&OUTER))
        };
        assert!(
            !core::ptr::eq(outer, &raw const OUTER),
            "the pack's own state"
        );
        assert_eq!(INNER_SHARED.load(std::sync::atomic::Ordering::Relaxed), 1);
        // SAFETY: the fake host's table lives for the process.
        let later = unsafe {
            enshwick_base::__internal::as_pack(loader.api(), ModHandle(3), || for_pack(&INNER))
        };
        assert!(
            !core::ptr::eq(later, &raw const INNER),
            "the lock is free again"
        );
    }
}
