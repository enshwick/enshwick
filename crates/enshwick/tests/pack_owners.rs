//! Each kind of callback a data pack registers runs as the pack, as the
//! loader runs it: inside the callback the pack's handle answers the host,
//! and the loader's copy of the API answers again after it.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use enshwick::abi::{
        BuildCheck, CraftCheck, DamageCheck, Header, ItemAction, ModHandle, Registers, Side, Status,
    };
    use enshwick::chat::ChatVerdict;
    use enshwick::content::Guid;
    use enshwick::controls::{Control, Key};
    use enshwick::points::{Order, Verdict};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::ui::{Dialog, Edge, HudScreenType};
    use enshwick::{Entity, ItemId, Load, Off, PlayerKey};

    /// The data pack's handle.
    const PACK: ModHandle = ModHandle(1);
    /// The loader's own handle, the host of the copy that runs packs.
    const LOADER: ModHandle = ModHandle(9);
    const SITE: u64 = 0x1_4009_0a6c;

    #[enshwick::point(name = "you.other:limit", mode = Override, output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Limit {
        pub(crate) level: u32,
    }

    #[enshwick::call(name = "you.pack:price-of", output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct PriceOf {
        pub(crate) item: u32,
    }

    /// A pack's settings, whose check notes its host.
    #[enshwick::config(validate = Settings::check)]
    #[derive(Default)]
    struct Settings {
        /// Altars at the first level.
        first: u32,
    }

    impl Settings {
        #[expect(
            clippy::unused_self,
            clippy::unnecessary_wraps,
            reason = "the shape `validate` takes; this check only notes its host"
        )]
        fn check(&self) -> Result<(), enshwick::config::ConfigError> {
            note();
            Ok(())
        }
    }

    /// The handle the last callback saw as its host.
    static SEEN: AtomicUsize = AtomicUsize::new(0);

    fn handle() -> Option<ModHandle> {
        enshwick::host().map(|host| host.handle())
    }

    fn note() {
        SEEN.store(handle().map_or(0, |h| h.0), Ordering::Relaxed);
    }

    /// The fake from `host`, with the loader's copy installed and the pack's
    /// `init` run.
    fn installed(host: FakeHost, init: impl FnOnce(&mut Load<'_>) -> Result<(), Off>) -> Installed {
        let loader = host.install();
        // SAFETY: the fake host's table lives for the process; the loader
        // makes its own handle the host of the copy that runs the packs.
        unsafe { enshwick_base::__internal::install_host(loader.api(), LOADER) };
        // SAFETY: the fake host's table lives for the process.
        let status =
            unsafe { enshwick::__private::init_pack(loader.api(), PACK, Side::SERVER, init) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        SEEN.store(0, Ordering::Relaxed);
        loader
    }

    fn ran_as_the_pack() {
        assert_eq!(
            SEEN.load(Ordering::Relaxed),
            PACK.0,
            "the callback ran as the pack"
        );
        assert_eq!(handle(), Some(LOADER), "the loader's after the call");
    }

    #[test]
    fn a_pack_s_served_resource_builds_as_the_pack() {
        let key = Guid::parse("2f04a05f-697d-489b-a32f-76bb3439f5bc").key_by_hash(1);
        let loader = installed(FakeHost::new(), |load| {
            load.serve(key, key, |bytes| {
                note();
                Some(bytes.to_vec())
            })
        });
        assert_eq!(loader.build_resource(&key, &[1, 2, 3]), Ok(vec![1, 2, 3]));
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_screen_extension_draws_as_the_pack() {
        let loader = installed(FakeHost::new().side(Side::CLIENT), |load| {
            load.extend_screen(HudScreenType::StorageChest, Edge::Right, |_| note())
        });
        let _drawn = loader.draw_screen(HudScreenType::StorageChest);
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_dialog_answer_runs_as_the_pack() {
        let loader = installed(FakeHost::new().side(Side::CLIENT), |load| {
            let shown = load.ui()?.dialog(&Dialog::new("Pack", "Asked"), |_| note());
            assert!(shown.is_ok(), "{shown:?}");
            Ok(())
        });
        assert!(loader.answer(None));
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_damage_filter_runs_as_the_pack() {
        let loader = installed(FakeHost::new(), |load| load.on_damage(|_| note()));
        let _after = loader.damage(DamageCheck {
            header: Header::of::<DamageCheck>(),
            cause: DamageCheck::FALL_DAMAGE,
            target: Entity(2),
            source: Entity(0),
            attacker: Entity(0),
            amounts: [0.0; enshwick::abi::DAMAGE_TYPES],
            total: 10.0,
        });
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_build_filter_runs_as_the_pack() {
        let loader = installed(FakeHost::new(), |load| {
            load.on_build(|_| {
                note();
                true
            })
        });
        let placed = loader.build(&BuildCheck {
            header: Header::of::<BuildCheck>(),
            player: Entity(2),
            item: ItemId::new(7),
            position: [0; 3],
            reason: core::ptr::null_mut(),
        });
        assert_eq!(placed, Ok(()));
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_craft_filter_runs_as_the_pack() {
        let loader = installed(FakeHost::new(), |load| {
            load.on_craft(|_| {
                note();
                true
            })
        });
        assert!(loader.craft(&CraftCheck {
            header: Header::of::<CraftCheck>(),
            crafting_operator_id: Entity(2),
            crafting_station_id: Entity(0),
            recipe_id: enshwick::RecipeId::new(5),
            recipe_amount: 1,
            reserved: 0,
        }));
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_item_filter_runs_as_the_pack() {
        let loader = installed(FakeHost::new(), |load| {
            load.on_item_action(|_| {
                note();
                true
            })
        });
        assert!(loader.item_action(&ItemAction {
            header: Header::of::<ItemAction>(),
            kind: ItemAction::PICKUP,
            player: Entity(1),
            item: ItemId::new(0x5e01),
            count: 1,
            inventory: Entity(2),
            slot: 0,
        }));
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_chat_filter_runs_as_the_pack() {
        let loader = installed(FakeHost::new(), |load| {
            load.on_chat(|_| {
                note();
                ChatVerdict::Keep
            })
        });
        assert_eq!(
            loader.say(PlayerKey::new(7), "hello").as_deref(),
            Some("hello")
        );
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_control_fires_as_the_pack() {
        let loader = installed(FakeHost::new().side(Side::CLIENT), |load| {
            load.control(Control::new("horn", "Horn").key(Key::parse("H")?), |_| {
                note();
            })
        });
        let horn = Key::parse("H").unwrap();
        assert_eq!(loader.keys(&[horn], Duration::from_millis(16)), 1);
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_served_call_answers_as_the_pack() {
        let loader = installed(FakeHost::new(), |load| {
            load.serve_call(|input: &PriceOf| {
                note();
                input.item * 10
            })
        });
        assert_eq!(loader.call(&PriceOf { item: 2 }), Ok(20));
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_point_link_answers_as_the_pack() {
        let loader = installed(FakeHost::new(), |load| {
            load.extend_point(&Order::new(), |_: &Limit, so_far: &u32| {
                note();
                Verdict::Set(*so_far + 1)
            })
        });
        assert_eq!(
            loader.ask_point(&Limit { level: 1 }, 3),
            enshwick::points::Decision::Changed(4)
        );
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_settings_check_runs_as_the_pack() {
        let loader = installed(FakeHost::new().settings("first = 1\n"), |load| {
            load.on_settings(|_: &Settings| {})
        });
        assert_eq!(loader.check_settings("settings", "first = 2\n"), Ok(()));
        ran_as_the_pack();
    }

    #[test]
    fn a_pack_s_mid_hook_runs_as_the_pack() {
        let loader = installed(FakeHost::new().symbol("site", SITE), |load| {
            let site = load.symbol("site")?;
            // SAFETY: the fake runs the handler on its own registers.
            unsafe { load.hook_at(site, |_| note()) }
        });
        let mut registers = Registers::default();
        assert_eq!(loader.run_at(SITE, &mut registers), 1);
        ran_as_the_pack();
    }
}
