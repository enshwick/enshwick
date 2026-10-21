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
}
