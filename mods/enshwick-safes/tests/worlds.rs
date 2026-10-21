//! Owners per world: each world keeps its own file, and the mod-wide file
//! from before moves into the first world that loads without owners.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::events::{ShutdownStarted, WorldLoaded};
    use enshwick_loader_api::test::FakeHost;
    use enshwick_safes::access::ACCESS;

    /// A loader that knows the item and recipe layouts the Safe needs.
    fn host() -> FakeHost {
        let mut host = FakeHost::new()
            .type_size("keen::ImpactUiValue", 0x28)
            .type_size("keen::RecipeInfo", 0xa8)
            .type_size("keen::RecipeInputResource", 0x18)
            .type_size("keen::RecipeItemStackResource", 0x18)
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2);
        for (field, at) in [
            ("itemId", 0),
            ("objectId", 4),
            ("name", 0x48),
            ("iconImage", 0x58),
            ("description", 0x68),
            ("flags", 0x78),
            ("pidEntity", 0x80),
            ("uiValues", 0x90),
        ] {
            host = host.field_offset("keen::ItemInfo", field, at);
        }
        for (field, at) in [
            ("locaId", 0),
            ("valueFormat", 4),
            ("value", 8),
            ("isSigned", 0x24),
        ] {
            host = host.field_offset("keen::ImpactUiValue", field, at);
        }
        for (field, at) in [
            ("recipeGuid", 0),
            ("recipeId", 0x10),
            ("workshopGuid", 0x14),
            ("workshopId", 0x24),
            ("input", 0x28),
            ("output", 0x30),
            ("completionRequirementQuery", 0x5c),
            ("requiredProps", 0x38),
            ("debugName", 0x40),
        ] {
            host = host.field_offset("keen::RecipeInfo", field, at);
        }
        host.field_offset("keen::RecipeRegistryResource", "recipes", 0)
            .field_offset("keen::RecipeItemStackResource", "item", 0x10)
            .field_offset("keen::RecipeItemStackResource", "count", 0x14)
            .field_offset("keen::ItemRegistryResource", "itemRefs", 0)
            .field_offset("keen::ItemRegistryResource", "dbgNames", 0x38)
    }

    #[test]
    fn the_old_owners_file_moves_into_the_world_that_loads() {
        let dir =
            std::env::temp_dir().join(format!("enshwick-safes-worlds-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("owners.toml"),
            "[[safes]]\ncell = [1, 2, 3]\nowner = \"00000000000000ff\"\nname = \"Ann\"\n",
        )
        .unwrap();
        let loader = host()
            .settings("cost_multiplier = 2\nadmins_can_open = true\nlabel_language = \"en\"\n")
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-aa")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_safes::enshwick_mod_init,
                enshwick_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(ACCESS.owners.load().is_empty(), "no world yet");
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        let owners = ACCESS.owners.load();
        assert_eq!(owners.get(&(1, 2, 3)).map(|safe| safe.owner), Some(0xff));
        assert!(dir.join("worlds/world0-aa/owners.toml").exists());
        assert!(!dir.join("owners.toml").exists());
        assert!(dir.join("owners.toml.moved").exists());
        assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        assert_eq!(ACCESS.owners.load().len(), 1, "the world's own file");
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_removed_safe_keeps_its_owner_on_disk_until_the_world_saves() {
        use enshwick_loader_api::abi::{BuildingData, Header};
        use enshwick_loader_api::events::{BuildingPlaced, BuildingRemoved, WorldSaved};

        let dir =
            std::env::temp_dir().join(format!("enshwick-safes-removed-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        let world = dir.join("worlds/world0-cc");
        std::fs::create_dir_all(&world).unwrap();
        let file = world.join("owners.toml");
        std::fs::write(
            &file,
            "[[safes]]\ncell = [4, 0, 4]\nowner = \"00000000000000ff\"\nname = \"Ann\"\n",
        )
        .unwrap();
        let loader = host()
            .settings("cost_multiplier = 2\nadmins_can_open = true\nlabel_language = \"en\"\n")
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-cc")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_safes::enshwick_mod_init,
                enshwick_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        assert_eq!(ACCESS.seen(900, (4, 0, 4)), Some((0xff, (4, 0, 4))));
        let removed = BuildingData {
            header: Header::of::<BuildingData>(),
            entity: 900,
            player: 10,
            item: 0,
            kind: BuildingData::PICKED_UP,
            replay: 0,
            template: [0; 16],
            position: [0.0; 3],
        };
        assert!(loader.raise::<BuildingRemoved>(&removed) >= 1);
        assert!(ACCESS.owners.load().is_empty(), "gone from the checks");
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("00000000000000ff"),
            "kept on disk while the world has not saved"
        );
        // Placed again elsewhere, as when a Safe moves: written at once.
        let placed = BuildingData {
            kind: BuildingData::PLACED,
            entity: 901,
            item: enshwick_safes::content::SAFE.id(),
            position: [10.0, 0.0, 10.0],
            ..removed
        };
        let write = std::fs::metadata(&file).unwrap().modified().unwrap();
        assert!(loader.raise::<BuildingPlaced>(&placed) >= 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::fs::metadata(&file).unwrap().modified().unwrap() == write
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("00000000000000ff"),
            "a write before the world saves still keeps the removed Safe's owner"
        );
        assert!(loader.raise::<WorldSaved>(&()) >= 1);
        assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
        assert!(
            !std::fs::read_to_string(&file)
                .unwrap()
                .contains("00000000000000ff"),
            "written with the world"
        );
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_safe_nobody_looked_at_lends_its_stock_to_no_stranger() {
        use enshwick_loader_api::abi::TickData;
        use enshwick_loader_api::ecs::Entity;
        use enshwick_loader_api::events::Ticked;
        use enshwick_loader_api::interactions::{Interaction, Kind};
        use enshwick_safes::access::{Safe, Seen};

        let dir = std::env::temp_dir().join(format!("enshwick-safes-stock-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let item: &'static u32 = Box::leak(Box::new(enshwick_safes::content::SAFE.id()));
        let loader = host()
            .type_hash("keen::ecs::InventorySetup", 3)
            .placed(3, 700, [20.0, 1.0, 20.0])
            .component(700, 2, std::ptr::from_ref(item).cast())
            .settings(
                "cost_multiplier = 2
admins_can_open = true
label_language = \"en\"
",
            )
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-bb")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_safes::enshwick_mod_init,
                enshwick_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        ACCESS.load_owners(std::collections::HashMap::from([(
            (40, 2, 40),
            Safe::new(1, "Ann"),
        )]));
        let stranger = Seen {
            key: 3,
            admin: false,
        };
        ACCESS
            .players
            .store(std::sync::Arc::new(std::collections::HashMap::from([(
                10, stranger,
            )])));
        let stock = Interaction {
            kind: Kind::Stock,
            player: Entity(10),
            target: Entity(700),
            item: None,
            position: None,
        };
        assert!(enshwick_safes::access::filter(&stock), "not seen yet");
        std::thread::sleep(std::time::Duration::from_millis(2100));
        let _ticked = loader.raise::<Ticked>(&TickData {
            header: enshwick_loader_api::abi::Header::of::<TickData>(),
            step: 1,
        });
        assert!(
            !enshwick_safes::access::filter(&stock),
            "the sweep found the Safe: {:?}",
            loader.logs()
        );
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn other_mods_change_who_may_use_a_safe_at_its_point() {
        use std::collections::HashMap;
        use std::sync::Arc;

        use enshwick_loader_api::ecs::Entity;
        use enshwick_loader_api::points::Verdict;
        use enshwick_loader_api::points::safes::{Access, SafeAction};
        use enshwick_safes::access::{Action, Seen};

        let dir = std::env::temp_dir().join(format!("enshwick-safes-point-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let loader = host()
            .settings(
                "cost_multiplier = 2
admins_can_open = true
label_language = \"en\"
",
            )
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-dd")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_safes::enshwick_mod_init,
                enshwick_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.defined_points(), ["safes:access"]);
        let (owner, stranger) = (
            Seen {
                key: 1,
                admin: false,
            },
            Seen {
                key: 3,
                admin: false,
            },
        );
        ACCESS
            .players
            .store(Arc::new(HashMap::from([(10, owner), (12, stranger)])));
        ACCESS.placed(700, (40, 2, 40), owner.key, "Ann");
        assert!(ACCESS.refused_seen(700, Entity(12)), "vanilla Safes");
        assert!(ACCESS.may_manage((40, 2, 40), owner, Entity(10)));
        assert!(!ACCESS.may_manage((40, 2, 40), stranger, Entity(12)));
        assert!(loader.extend_point(|access: &Access, may: &bool| {
            match access.action {
                SafeAction::USE if access.player == 3 => Verdict::Set(true),
                SafeAction::REMOVE | SafeAction::MANAGE => Verdict::Refuse,
                _ => Verdict::Set(*may),
            }
        }));
        assert!(
            !ACCESS.refused_seen(700, Entity(12)),
            "the link lets them in"
        );
        assert!(
            !ACCESS.may(
                owner.key,
                (40, 2, 40),
                Entity(700),
                Entity(10),
                Action::Remove
            ),
            "the link refuses even the owner"
        );
        assert!(ACCESS.may(owner.key, (40, 2, 40), Entity(700), Entity(10), Action::Use));
        assert!(
            !ACCESS.may_manage((40, 2, 40), owner, Entity(10)),
            "`/safe allow` and `deny` hear the refusal too"
        );
        assert!(
            ACCESS.refused_seen(700, Entity(14)),
            "a player not registered yet stays refused"
        );
        assert!(ACCESS.removed(700).is_some());
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    fn player(entity: u32, key: u64, name: &str) -> enshwick_loader_api::abi::PlayerInfo {
        use enshwick_loader_api::abi::{Header, PlayerInfo};
        let mut bytes = [0u8; 64];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity,
            key,
            permissions: PlayerInfo::CAN_ACCESS_INVENTORIES,
            name_len: u8::try_from(name.len()).unwrap(),
            name: bytes,
        }
    }

    #[test]
    fn safe_commands_act_on_the_nearest_safe_the_player_may_manage() {
        use enshwick_loader_api::points::Verdict;
        use enshwick_loader_api::points::safes::{Access, SafeAction};

        let dir =
            std::env::temp_dir().join(format!("enshwick-safes-manage-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let at = |x: f64| -> *const core::ffi::c_void {
            let fixed: &'static [i64; 3] =
                Box::leak(Box::new(enshwick_loader_api::ecs::fixed([x, 0.0, 0.0])));
            std::ptr::from_ref(fixed).cast()
        };
        let (ann, bob, cat) = (
            player(10, 1, "Ann"),
            player(11, 2, "Bob"),
            player(12, 3, "Cat"),
        );
        let loader = host()
            .player(ann)
            .player(bob)
            .player(cat)
            .component(10, 1, at(0.0))
            .component(11, 1, at(100.0))
            .component(12, 1, at(20.0))
            .settings(
                "cost_multiplier = 2
admins_can_open = true
label_language = \"en\"
",
            )
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-ee")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_safes::enshwick_mod_init,
                enshwick_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        ACCESS.placed(700, (2, 0, 0), 1, "Ann");
        ACCESS.placed(701, (42, 0, 0), 2, "Bob");
        let safe = |who, args| loader.command("safe", who, args).unwrap();

        assert_eq!(
            safe(&ann, "allow Cat"),
            "Cat may now open your Safe at (1, 0, 0), the nearest you manage within 4 m."
        );
        assert_eq!(
            safe(&cat, "list"),
            "No Safe you manage within 4 m.",
            "Bob's Safe is not Cat's to manage"
        );
        assert!(loader.extend_point(|access: &Access, may: &bool| {
            match (access.action, access.player) {
                (SafeAction::MANAGE, 3) => Verdict::Set(true),
                (SafeAction::MANAGE, 1) => Verdict::Refuse,
                _ => Verdict::Set(*may),
            }
        }));
        assert_eq!(
            safe(&cat, "allow Ann"),
            "Ann may now open Bob's Safe at (21, 0, 0), the nearest you manage within 4 m."
        );
        assert_eq!(
            safe(&cat, "list"),
            "Allowed at Bob's Safe at (21, 0, 0): Ann."
        );
        assert_eq!(
            safe(&ann, "deny Cat"),
            "You may not change who opens your Safe at (1, 0, 0)."
        );
        let owners = ACCESS.owners.load();
        assert_eq!(owners[&(2, 0, 0)].allowed.len(), 1, "Cat stays allowed");
        assert_eq!(owners[&(42, 0, 0)].allowed[0].key, 1);
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
