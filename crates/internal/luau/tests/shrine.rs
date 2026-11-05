//! The content example `examples/you.shrine`, loaded as the loader loads a
//! pack with scripts: its item's bindings, its reward and its settings.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Header, ItemAction, PlayerInfo, Status, TickData};
    use enshwick::events::{SettingsReplaced, Ticked, WorldLoaded};
    use enshwick::packs::{Pack, PackFiles, PackManifest};
    use enshwick::progress::Experience;
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, ItemId, Load, Mod, Off, PlayerKey};
    use enshwick_luau::{Limits, Scripts};

    const ID: &str = "you.shrine";

    static RUNTIME: Mutex<Option<enshwick_luau::Runtime>> = Mutex::new(None);

    fn read(name: &str) -> String {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/you.shrine");
        std::fs::read_to_string(dir.join(name)).unwrap()
    }

    struct Shrine;

    impl Mod for Shrine {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let mut files = PackFiles::new();
            for name in [
                "pack.toml",
                "loot.toml",
                "rewards.toml",
                "texts.toml",
                "main.luau",
            ] {
                files = files.with(name, read(name));
            }
            for name in ["items.toml", "recipes.toml"] {
                files = files.with(name, read(name));
            }
            let pack = Pack::parse_with(ID, &files, None).map_err(|e| Off::new(e.to_string()))?;
            load.add_pack(&pack)?;
            let manifest =
                PackManifest::parse(&read("pack.toml")).map_err(|e| Off::new(e.to_string()))?;
            let scripts = Scripts::new()
                .with("main.luau", read("main.luau"))
                .with_items(&pack.item_bindings());
            let runtime = enshwick_luau::start_pack(load, &manifest, scripts, Limits::default())?;
            *RUNTIME.lock().unwrap() = Some(runtime);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Shrine,
        r#"
        id = "you.shrine"
        side = "server"
        "#
    );

    fn action(kind: u32) -> ItemAction {
        ItemAction {
            header: Header::of::<ItemAction>(),
            kind,
            player: Entity(2),
            item: ItemId::named("you.shrine:tonic"),
            count: 1,
            inventory: Entity(40),
            slot: 0,
        }
    }

    fn gained(loader: &Installed) -> u32 {
        let _ran = loader.raise::<Ticked>(&TickData::new(1));
        let mut rows = [Experience {
            entity: Entity(2),
            level: 1,
            progress: 0,
            need: 10_000,
            gained: 0,
        }];
        loader.experience(&mut rows, 25, |level| level * 10_000);
        rows[0].gained
    }

    #[test]
    fn the_shrine_tonic_counts_each_drink_keeps_itself_and_rewards_the_world_s_experience_once() {
        let data = std::env::temp_dir().join(format!("enshwick-shrine-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let mut name = [0u8; 64];
        name[..3].copy_from_slice(b"Ann");
        let ann = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(2),
            key: PlayerKey::new(7),
            permissions: 0,
            name_len: 3,
            name,
        };
        let loader = FakeHost::new()
            .items()
            .recipes()
            .ui_images()
            .loot()
            .inventory()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .symbol("inventory.give", 0x1_4030_0000)
            .symbol("loot.roll", 0x1_4031_0000)
            .symbol("loot.roll_one", 0x1_4032_0000)
            .constant("loot.txn_context", 0x10)
            .constant("loot.txn_entity", 0x18)
            .constant("loot.request_node", 0x08)
            .constant("loot.request_labels", 0x0c)
            .world()
            .player(ann)
            .data_dir(data.to_str().unwrap())
            .world_key("world0")
            .settings("first_drink_experience = 250\n")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Shrine)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            loader.raise::<WorldLoaded>(&()),
            3,
            "the settings', the script's and the rewards'"
        );
        let _replaced = loader.raise::<SettingsReplaced>(&());

        assert!(loader.item_action(&action(ItemAction::CONSUME)));
        assert_eq!(
            gained(&loader),
            250,
            "the world's experience, not the default"
        );
        assert!(loader.item_action(&action(ItemAction::CONSUME)));
        assert_eq!(gained(&loader), 0, "the first drink only");
        let logged = |text: &str| loader.logs().iter().any(|(_, line)| line.contains(text));
        assert!(
            logged("Ann drank shrine tonic number 2"),
            "{:?}",
            loader.logs()
        );
        assert!(
            !loader.item_action(&action(ItemAction::TRASH)),
            "a tonic is kept"
        );
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
        let _gone = std::fs::remove_dir_all(&data);
    }
}
