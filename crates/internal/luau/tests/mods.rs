//! Luau mods as a pack makes them: the example mod, and a mod without Rust
//! or data files that adds content and sets rules from its settings.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, PlayerData, PlayerInfo, Status};
    use enshwick::config::{Field, Schema};
    use enshwick::events::{PlayerJoined, SettingsReplaced};
    use enshwick::rates::{ExperienceGroup, Rate};
    use enshwick::test::FakeHost;
    use enshwick_luau::Scripts;

    use crate::harness::{logged, start_with};

    /// The address of random loot's roll in [`loot_host`].
    const ROLL: u64 = 0x1_4010_0000;

    /// A build with random loot's roll, which `add_loot` hooks.
    fn loot_host() -> FakeHost {
        FakeHost::new()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.give", 0x1_4030_0000)
            .symbol("loot.roll", ROLL)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .constant("loot.txn_context", 0x10)
            .constant("loot.txn_entity", 0x18)
            .constant("loot.request_node", 0x08)
            .constant("loot.request_labels", 0x0c)
            .world()
            .loot()
    }

    const DROP: &str = r#"enshwick.loot.add_loot({
        items = { 7 },
        chance = enshwick.settings().chance,
        count = { 1, 1 },
        from = { kinds = { "CORPSES" } },
    })
    enshwick.log.info(`drop at {enshwick.settings().chance}`)"#;

    fn chance_schema(chance: f64) -> Schema {
        Schema::new(
            "",
            "config",
            vec![Field::new("chance", &chance).with_doc("The drop's chance.", "")],
        )
    }

    fn hooked(loader: &enshwick::test::Installed) -> Vec<u64> {
        loader.hooks().iter().map(|hook| hook.target).collect()
    }

    #[test]
    fn the_example_mod_greets_each_player_who_joins_with_its_settings() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/you.welcome");
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
        let schema = enshwick::packs::PackManifest::parse(&read("pack.toml"))
            .unwrap()
            .settings_schema();
        let mut name = [0u8; 64];
        name[..3].copy_from_slice(b"Ann");
        let ann = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(10),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 3,
            name,
        };
        let host = FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .player(ann);
        let (loader, status) = start_with(
            host,
            &read("main.luau"),
            Scripts::new().with("lib/count.luau", read("lib/count.luau")),
            schema,
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let _handled = loader.raise::<SettingsReplaced>(&());
        assert!(
            logged(&loader, "this world greets with \"Welcome\""),
            "{:?}",
            loader.logs()
        );
        let joined = PlayerData {
            header: Header::of::<PlayerData>(),
            slot: 0,
            entity: enshwick::Entity(10),
            key: enshwick::PlayerKey::new(7),
        };
        assert_eq!(loader.raise::<PlayerJoined>(&joined), 1);
        assert!(
            loader
                .chat()
                .iter()
                .any(|(_, line)| line.starts_with("Welcome, Ann: visit 1")),
            "{:?}",
            loader.chat()
        );
        let listed: Vec<String> = loader.controls().into_iter().map(|c| c.0).collect();
        assert_eq!(listed, ["count"]);
        let run = enshwick::abi::ActionCall::RUN;
        let refused = loader.ask_actions(run, &ann, 0, "count", "");
        assert!(
            refused
                .as_deref()
                .is_some_and(|reply| reply.starts_with('0')),
            "{refused:?}"
        );
        let admin = PlayerInfo {
            permissions: PlayerInfo::CAN_KICK_BAN,
            ..ann
        };
        let done = loader.ask_actions(run, &admin, 0, "count", "");
        assert_eq!(done.as_deref(), Some("11 player online"));
    }

    #[test]
    fn a_script_adds_a_loot_drop_with_the_chance_its_settings_give() {
        let (loader, status) =
            start_with(loot_host(), DROP, Scripts::new(), Some(chance_schema(0.25)));
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(&loader, "drop at 0.25"), "{:?}", loader.logs());
        assert_eq!(hooked(&loader), [ROLL]);
    }

    #[test]
    fn a_loot_chance_past_one_refuses_the_start_naming_the_call() {
        let (loader, status) =
            start_with(loot_host(), DROP, Scripts::new(), Some(chance_schema(2.0)));
        assert_ne!(status, Status::OK);
        assert!(
            logged(&loader, "enshwick.loot.add_loot"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_luau_only_mod_adds_an_item_a_station_a_recipe_and_a_drop_and_changes_a_stack() {
        let chest = "6943053a-32c1-4718-ae22-e61427ddf54d";
        let bench = enshwick::content::workshops::stations::WORKBENCH;
        let main = format!(
            r#"
            enshwick.content.add_item({{ id = "you.luau:locker", like = "{chest}" }})
            local bench = enshwick.content.add_workshop({{
                id = "you.luau:bench",
                like = {{ item = "{item}", template = "{template}" }},
                named = false,
                described = false,
                debug_name = "Craftingstation_LockerBench",
            }})
            enshwick.content.add_recipe({{
                id = "you.luau:locker.recipe",
                like = "{chest}",
                output = enshwick.content.guid("you.luau:locker"),
                output_count = 1,
                cost = 2,
                workshop = bench,
            }})
            enshwick.loot.add_loot({{
                items = {{ enshwick.resources.guid_hash(enshwick.content.guid("you.luau:locker")) }},
                chance = enshwick.settings().chance,
                count = {{ 1, 1 }},
                from = {{ kinds = {{ "CHESTS" }} }},
            }})
            enshwick.content.change({{ entry = "{chest}", field = "MaxStackSize", value = {{ Set = 99 }} }}, "stacks")
            "#,
            item = bench.item,
            template = bench.template,
        );
        let (loader, status) = start_with(
            loot_host().items().recipes().workshops(),
            &main,
            Scripts::new(),
            Some(chance_schema(0.25)),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let ids = loader.content_ids();
        for id in [
            "you.luau:locker",
            "you.luau:locker.recipe",
            "you.luau:bench",
        ] {
            assert!(ids.iter().any(|i| i == id), "{id} in {ids:?}");
        }
        assert_eq!(loader.changes().len(), 1, "{:?}", loader.logs());
        assert_eq!(hooked(&loader), [ROLL]);
    }

    #[test]
    fn a_luau_mod_copies_a_template_and_changes_a_vanilla_one_by_component() {
        use enshwick::resources::{Registry, Value, fnv1a32};
        use enshwick::test::CREATURE;

        let main = format!(
            r#"
            local tame = enshwick.content.add_template({{
                id = "you.luau:tame-wolf",
                like = "{CREATURE}",
                changes = {{
                    {{ component = "ecs.Faction", edit = {{ faction = "Vukah" }} }},
                    {{ component = "ecs.Aggro" }},
                }},
            }})
            enshwick.log.info(`tame {{tame}}`)
            enshwick.content.change_template("{CREATURE}", {{
                {{ component = "ecs.Aggro", set = {{ base = {{}}, apply_enemy_value = false, sight_check_slot = "Root" }} }},
            }})
            local ok = pcall(enshwick.content.change_template, "{CREATURE}", {{ {{ component = "ecs.Boss" }} }})
            enshwick.log.info(`no boss to remove: {{not ok}}`)
            "#
        );
        let (loader, status) = start_with(FakeHost::new().templates(), &main, Scripts::new(), None);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let tame = enshwick::content::Guid::named("you.luau:tame-wolf");
        assert!(
            logged(&loader, &format!("tame {tame}")),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "no boss to remove: true"),
            "{:?}",
            loader.logs()
        );

        let ty = "demo::ecs::TemplateResource";
        let key = |guid: enshwick::content::Guid| guid.key_by_hash(fnv1a32(ty.as_bytes()));
        let base = enshwick::host()
            .unwrap()
            .game_resource(key(CREATURE))
            .unwrap();
        let registry = Registry::game().unwrap();
        let components = |built: Vec<u8>| {
            let value = registry.decode(ty, &built).unwrap();
            let Some(Value::List(components)) = value.field("components").cloned() else {
                panic!("{value:?}")
            };
            components
        };
        let copied = components(loader.build_resource(&key(tame), &base).unwrap());
        assert_eq!(copied.len(), 1, "aggro removed: {copied:?}");
        let Value::Variant(faction) = &copied[0] else {
            panic!("{copied:?}")
        };
        assert_eq!(
            faction.value().field("faction"),
            Some(&Value::Name("Vukah".into()))
        );
        let vanilla = components(loader.build_resource(&key(CREATURE), &base).unwrap());
        let Value::Variant(aggro) = &vanilla[1] else {
            panic!("{vanilla:?}")
        };
        assert_eq!(
            aggro.value().field("sightCheckSlot"),
            Some(&Value::Name("Root".into()))
        );
    }

    #[test]
    fn a_luau_mod_adds_a_prop_whose_item_places_its_template_and_a_crafting_prop() {
        use enshwick::content::{EngineFacts, Guid, types};
        use enshwick::test::{CRAFTING_PROP_ITEM, PROP_ITEM};

        let main = format!(
            r#"
            local crate = enshwick.content.add_prop({{ item = {{ id = "you.luau:crate", like = "{PROP_ITEM}" }} }})
            local anvil = enshwick.content.add_prop({{
                item = {{ id = "you.luau:anvil", like = "{CRAFTING_PROP_ITEM}" }},
                crafting_prop = {{ recipes = {{}} }},
            }})
            enshwick.log.info(`crate {{crate.template}} {{crate.crafting_prop}}`)
            enshwick.log.info(`anvil {{anvil.crafting_prop}}`)
            "#
        );
        let (loader, status) = start_with(FakeHost::new().props(), &main, Scripts::new(), None);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let named = |id: &str| Guid::named(id);
        for line in [
            format!("crate {} nil", named("you.luau:crate.template")),
            format!("anvil {}", named("you.luau:anvil.crafting-prop")),
        ] {
            assert!(logged(&loader, &line), "{line}: {:?}", loader.logs());
        }

        let host = enshwick::host().unwrap();
        let at = host.offset("item_info.placed_entity").unwrap();
        let vanilla = host
            .game_resource(PROP_ITEM.key(types::ITEM_INFO).unwrap())
            .unwrap();
        let item = loader
            .build_resource(
                &named("you.luau:crate").key(types::ITEM_INFO).unwrap(),
                &vanilla,
            )
            .unwrap();
        assert_eq!(item[at..at + 16], named("you.luau:crate.template").0);
    }

    #[test]
    fn a_luau_mod_adds_an_arsenal_and_an_enemy_that_uses_it() {
        use enshwick::content::{EngineFacts, Guid};
        use enshwick::resources::{Registry, Value};
        use enshwick::test::{BITE, ENEMY, ENEMY_ARSENAL};

        let main = format!(
            r#"
            local pack = enshwick.content.add_arsenal({{
                id = "you.luau:pack",
                like = "{ENEMY_ARSENAL}",
                change = {{ attacks = {{ "{BITE}" }} }},
            }})
            local elder = enshwick.content.add_enemy({{
                id = "you.luau:elder", like = "{ENEMY}", level = 12, faction = "Vukah", arsenal = pack,
            }})
            enshwick.log.info(`elder {{elder}} uses {{pack}}`)
            "#
        );
        let (loader, status) = start_with(FakeHost::new().enemies(), &main, Scripts::new(), None);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let (elder, pack) = (Guid::named("you.luau:elder"), Guid::named("you.luau:pack"));
        let line = format!("elder {elder} uses {pack}");
        assert!(logged(&loader, &line), "{line}: {:?}", loader.logs());

        let host = enshwick::host().unwrap();
        let hash = host.hash("ecs::TemplateResource").unwrap();
        let base = host.game_resource(ENEMY.key_by_hash(hash)).unwrap();
        let built = loader
            .build_resource(&elder.key_by_hash(hash), &base)
            .unwrap();
        let value = Registry::game()
            .unwrap()
            .decode("demo::ecs::TemplateResource", &built)
            .unwrap();
        let Some(Value::List(components)) = value.field("components") else {
            panic!("{value:?}")
        };
        let held = |name: &str| {
            components.iter().find_map(|held| match held {
                Value::Variant(held) if held.type_name() == name => Some(held.value().clone()),
                _ => None,
            })
        };
        assert_eq!(
            held("demo::ecs::Faction").and_then(|f| f.field("faction").cloned()),
            Some(Value::Name("Vukah".into()))
        );
        assert_eq!(
            held("demo::ecs::EnemyArsenalReference").and_then(|a| a.field("arsenal").cloned()),
            Some(Value::Guid(pack))
        );
    }

    #[test]
    fn a_luau_mod_adds_a_spawn_entry_changes_a_spawner_and_adds_an_altar_kind() {
        use enshwick::content::Guid;
        use enshwick::test::{ENEMY, NEST, PROP_ITEM};

        let main = format!(
            r#"
            enshwick.content.add_spawn_entry({{ spawner = {{ Listing = "{ENEMY}" }}, template = "{NEST}", weight = 3 }})
            enshwick.content.change_spawner({{ Spawner = "{NEST}" }}, {{ max_entities_alive = 2 }})
            local wide = enshwick.altars.add_flame_altar({{
                item = {{ id = "you.luau:wide", like = "{PROP_ITEM}" }},
                zones = {{ {{ grow = 4, height = 20 }} }},
                max_level = 1,
            }})
            enshwick.log.info(`altars of {{wide.template}}`)
            "#
        );
        let kind = Guid::named("you.luau:wide.template");
        let upgrade = Guid([0xc1; 16]);
        let host = FakeHost::new()
            .enemies()
            .props()
            .upgrade_recipes(&[(upgrade, 1)])
            .template(enshwick::Entity(10), kind.0);
        let (loader, status) = start_with(host, &main, Scripts::new(), None);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let line = format!("altars of {kind}");
        assert!(logged(&loader, &line), "{line}: {:?}", loader.logs());
        assert_eq!(loader.altar_zone(enshwick::Entity(10), 1, 30), (4, 20));
        let craft = enshwick::abi::CraftCheck {
            header: Header::of::<enshwick::abi::CraftCheck>(),
            crafting_operator_id: enshwick::Entity(2),
            crafting_station_id: enshwick::Entity(10),
            recipe_id: enshwick::RecipeId(upgrade.fnv1a()),
            recipe_amount: 1,
            reserved: 0,
        };
        assert!(!loader.craft(&craft), "an upgrade past the kind's maximum");
    }

    #[test]
    fn on_settings_hands_the_settings_over_now_and_at_each_world_load() {
        let (loader, status) = start_with(
            FakeHost::new(),
            r"
            local seen = 0
            enshwick.on_settings(function(settings)
                seen += 1
                enshwick.log.info(`settings {seen} {type(settings)}`)
            end)
            ",
            Scripts::new(),
            None,
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(&loader, "settings 1 table"), "{:?}", loader.logs());
        let _handled = loader.raise::<SettingsReplaced>(&());
        assert!(logged(&loader, "settings 2 table"), "{:?}", loader.logs());
    }

    const EXPORT: &str = r"
        local ok, dir = pcall(enshwick.export_dir)
        enshwick.log.info(`export {ok} {dir}`)
    ";

    #[test]
    fn the_export_folder_sits_beside_mods_named_by_the_mod_id() {
        let script = EXPORT;
        let listed = enshwick::abi::ModInfo {
            id: enshwick::abi::Str::new("you.export"),
            file: enshwick::abi::Str::new("mods/Export Pack/pack.toml"),
            ..enshwick::abi::ModInfo::EMPTY
        };
        let (installed, status) = start_with(
            FakeHost::new()
                .mod_dir("game/mods/Export Pack")
                .loaded(listed),
            script,
            Scripts::new(),
            None,
        );
        assert_eq!(status, Status::OK, "{:?}", installed.logs());
        let beside = std::path::Path::new("game")
            .join("export")
            .join("you.export");
        assert!(
            logged(&installed, &format!("export true {}", beside.display())),
            "{:?}",
            installed.logs()
        );
    }

    #[test]
    fn a_mod_folder_outside_mods_has_no_export_folder() {
        let script = EXPORT;
        let (elsewhere, status) = start_with(
            FakeHost::new().mod_dir("dev/you.export"),
            script,
            Scripts::new(),
            None,
        );
        assert_eq!(status, Status::OK, "{:?}", elsewhere.logs());
        assert!(logged(&elsewhere, "export false"), "{:?}", elsewhere.logs());
    }

    #[test]
    fn a_script_sets_a_rate_and_a_factor_past_its_range_raises_an_error() {
        let (loader, status) = start_with(
            FakeHost::new(),
            r#"
            local combat = enshwick.rates.rate({ Experience = "Combat" })
            local ok, why = pcall(combat.set, combat, 250)
            enshwick.log.info(`refused {not ok} {why}`)
            combat:set(3)
            "#,
            Scripts::new(),
            None,
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(&loader, "refused true"), "{:?}", loader.logs());
        assert!(logged(&loader, "RateHandle:set"));
        assert_eq!(
            loader.rate(Rate::Experience(ExperienceGroup::Combat)),
            Some(3.0)
        );
    }
}
