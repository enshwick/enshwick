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
                enshwick::__private::init_pack(loader.api(), me, |load| {
                    load.add_pack(&pack)?;
                    let declared = enshwick::packs::declared_settings(load).map(|(_, v)| v);
                    let bonus = declared.and_then(|v| v.get("bonus")?.as_integer());
                    assert_eq!(
                        bonus,
                        setting.then_some(5),
                        "{id} sees only its own settings"
                    );
                    Ok(())
                })
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
        unsafe { enshwick::__private::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(loader.api(), ModHandle(1), |_| {
                assert_eq!(handle(), Some(ModHandle(1)), "init runs as the pack");
                Ok(())
            })
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
                unsafe { enshwick::__private::as_pack(loader.api(), ModHandle(3), handle) };
            (handle(), nested)
        };
        // SAFETY: the fake host's table lives for the process.
        let inner = unsafe { enshwick::__private::as_pack(loader.api(), ModHandle(2), nested) };
        assert_eq!(inner, (Some(ModHandle(2)), Some(ModHandle(3))));
        assert_eq!(handle(), Some(ModHandle(9)));
    }

    #[test]
    fn a_pack_s_panic_leaves_the_loader_as_the_host() {
        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick::__private::install_host(loader.api(), ModHandle(9)) };
        let panicked = std::panic::catch_unwind(|| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                enshwick::__private::as_pack(loader.api(), ModHandle(2), || {
                    panic!("the pack's builder")
                })
            }
        });
        assert!(panicked.is_err());
        assert_eq!(handle(), Some(ModHandle(9)));
    }
}
