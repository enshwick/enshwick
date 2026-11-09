//! `enshwick.content`: a Luau mod adds items and recipes and changes
//! vanilla values while it starts, through the same calls and checks as a
//! Rust mod.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::categories::ItemCategoryMask;
    use enshwick::content::changes::{Field, Items, Value};
    use enshwick::content::{EngineFacts as _, Guid, Id, types};
    use enshwick::resources::{Registry, Value as Data};
    use enshwick::test::{BALANCING_TABLE, FakeHost};

    use crate::harness::{logged, run, start};

    const CHEST: &str = "6943053a-32c1-4718-ae22-e61427ddf54d";

    #[test]
    fn a_script_adds_an_item_cloned_from_a_vanilla_one() {
        let loader = run(
            FakeHost::new().items(),
            &format!(
                r#"enshwick.content.add_item({{ id = "you.luau:locker", like = "{CHEST}" }})"#
            ),
        );
        assert_eq!(loader.content_ids(), ["you.luau:locker"]);
        let locker = Id::new("you.luau:locker").guid();
        let item = FakeHost::new().items().hash(types::ITEM_INFO.id()).unwrap();
        assert!(
            loader
                .resources()
                .iter()
                .any(|r| r.key == locker.key_by_hash(item))
        );
    }

    #[test]
    fn a_script_adds_a_player_s_knowledge_entry_and_gets_its_id() {
        let loader = run(
            FakeHost::new().progression(),
            r#"
            local id = enshwick.content.add_knowledge({
                id = "you.luau:shrine", scope = "Player", kind = "Bool", skill_points = 1,
            })
            enshwick.log.info(`knowledge {id}`)
            local ok = pcall(enshwick.content.add_knowledge, { id = "you.luau:w", scope = "World", kind = "Bool", skill_points = 1 })
            enshwick.log.info(`world points refused {not ok}`)
            "#,
        );
        let id = enshwick::knowledge::KnowledgeId::from_guid(Id::new("you.luau:shrine").guid());
        assert!(
            logged(&loader, &format!("knowledge {id}")),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "world points refused true"),
            "{:?}",
            loader.logs()
        );
        let hash = FakeHost::new()
            .progression()
            .hash("GameKnowledgeResource")
            .unwrap();
        let knowledge = enshwick::content::knowledge::GAME_KNOWLEDGE.key_by_hash(hash);
        assert!(loader.resources().iter().any(|r| r.key == knowledge));
    }

    #[test]
    fn a_script_lists_vanilla_experience_and_gives_a_value_of_its_own_to_knowledge() {
        let loader = run(
            FakeHost::new().progression(),
            r#"
            local vanilla = enshwick.content.experience_values()
            enshwick.log.info(`{#vanilla} vanilla, first {vanilla[1].gain} of {vanilla[1].source}`)
            local xp = enshwick.content.add_experience({ id = "you.luau:shrine-xp", source = "Exploration", gain = 250 })
            enshwick.log.info(`experience {xp}`)
            enshwick.content.add_knowledge({
                id = "you.luau:shrine", scope = "Player", kind = "Bool", experience = xp,
            })
            local ok = pcall(enshwick.content.add_experience, { id = "you.luau:bad", source = "Sleeping", gain = 1 })
            enshwick.log.info(`unknown source refused {not ok}`)
            "#,
        );
        let logs = format!("{:?}", loader.logs());
        assert!(logged(&loader, "3 vanilla, first 5 of Mining"), "{logs}");
        let xp = Id::new("you.luau:shrine-xp").guid().fnv1a();
        assert!(logged(&loader, &format!("experience {xp}")), "{logs}");
        assert!(logged(&loader, "unknown source refused true"), "{logs}");
        let host = enshwick::host().unwrap();
        let table = BALANCING_TABLE.key_by_hash(host.hash("balancing_table").unwrap());
        let base = host.game_resource(table).unwrap();
        let built = Registry::game()
            .unwrap()
            .decode(
                "demo::BalancingTable",
                &loader.build_resource(&table, &base).unwrap(),
            )
            .unwrap();
        let Some(Data::List(values)) = built.field("experienceValues") else {
            panic!("{built:?}")
        };
        let last = values.last().unwrap();
        assert_eq!(
            last.field("experienceValueId")
                .and_then(|id| id.field("value")),
            Some(&Data::Uint(xp.into())),
            "{built:?}"
        );
        assert_eq!(
            last.field("source"),
            Some(&Data::Name("Exploration".into()))
        );
        assert_eq!(last.field("experienceGain"), Some(&Data::Uint(250)));
    }

    #[test]
    fn a_script_adds_a_quest_and_a_page_and_changes_a_vanilla_quest() {
        let loader = run(
            FakeHost::new().progression(),
            &format!(
                r#"
                local given = enshwick.content.add_knowledge({{ id = "you.luau:given", scope = "Player", kind = "Bool" }})
                local is_given = {{ id = given, scope = "Player", test = "IsSet" }}
                local quest = enshwick.content.add_quest({{
                    id = "you.luau:trail",
                    name = enshwick.content.guid("you.luau:trail.name"),
                    source = "None",
                    kind = "PlayerQuest",
                    steps = {{ {{
                        name = enshwick.content.guid("you.luau:trail.1"),
                        text = enshwick.content.guid("you.luau:trail.1.text"),
                        shown_when = is_given,
                        done_when = {{ id = given, scope = "Player", test = {{ AtLeast = 1 }} }},
                    }} }},
                    rewards = {{ {{ Item = "{CHEST}" }} }},
                }})
                enshwick.log.info(`quest {{quest}}`)
                enshwick.content.add_journal_page({{
                    id = "you.luau:lore",
                    category = enshwick.content.guid("you.luau:lore.category"),
                    name = enshwick.content.guid("you.luau:lore.name"),
                    entries = {{ {{
                        name = enshwick.content.guid("you.luau:lore.1"),
                        text = enshwick.content.guid("you.luau:lore.1.text"),
                        shown_when = is_given,
                    }} }},
                }})
                enshwick.content.change_quest({VANILLA}, {{ add_rewards = {{ {{ Item = "{CHEST}" }} }} }})
                local ok = pcall(enshwick.content.change_quest, 1, {{}})
                enshwick.log.info(`unknown quest refused {{not ok}}`)
                "#,
                VANILLA = enshwick::test::VANILLA_QUEST,
            ),
        );
        let quest = enshwick::knowledge::KnowledgeId::from_guid(Id::new("you.luau:trail").guid());
        assert!(
            logged(&loader, &format!("quest {quest}")),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "unknown quest refused true"),
            "{:?}",
            loader.logs()
        );
        let hash = FakeHost::new()
            .progression()
            .hash("JournalRegistryResource")
            .unwrap();
        let journal = enshwick::content::journal::JOURNAL_REGISTRY.key_by_hash(hash);
        assert!(loader.resources().iter().any(|r| r.key == journal));
    }

    #[test]
    fn a_script_reads_and_sets_a_player_s_quest_progression() {
        let progression = enshwick::resources::fnv1a32(enshwick::test::PROGRESSION.as_bytes());
        let loader = run(
            FakeHost::new().progression().world().component(
                enshwick::Entity(4096),
                progression,
                &[0, 0, 0, 0, 4, 0, 0, 0],
            ),
            r"
            enshwick.log.info(`before {enshwick.knowledge.quest_progression(4096)}`)
            enshwick.knowledge.set_quest_progression(4096, true)
            enshwick.log.info(`after {enshwick.knowledge.quest_progression(4096)}`)
            enshwick.log.info(`nobody {enshwick.knowledge.quest_progression(1)}`)
            local ok = pcall(enshwick.knowledge.set_quest_progression, 1, true)
            enshwick.log.info(`nobody refused {not ok}`)
            ",
        );
        let logs = format!("{:?}", loader.logs());
        assert!(logged(&loader, "before false"), "{logs}");
        assert!(logged(&loader, "after true"), "{logs}");
        assert!(logged(&loader, "nobody nil"), "{logs}");
        assert!(logged(&loader, "nobody refused true"), "{logs}");
        assert_eq!(
            loader.component_bytes(enshwick::Entity(4096), progression),
            Some(vec![1, 0, 0, 0, 5, 0, 0, 0]),
            "set, its change counted"
        );
    }

    #[test]
    fn a_script_lists_the_tree_and_adds_a_node_and_a_perk() {
        let loader = run(
            FakeHost::new().progression(),
            &format!(
                r#"
                local nodes = enshwick.content.skill_nodes()
                local root = nodes[1]
                enshwick.log.info(`{{#nodes}} nodes, first a {{root.kind}}`)
                local node = enshwick.content.add_skill_node({{
                    id = "you.luau:endurance",
                    like = nodes[2].id,
                    x = root.x,
                    y = root.y + 300,
                    links_from = {{ root.id }},
                    name = enshwick.content.guid("you.luau:endurance.name"),
                    description = enshwick.content.guid("you.luau:endurance.text"),
                    cost = 1,
                }})
                enshwick.log.info(`node {{node}}`)
                local ok = pcall(enshwick.content.add_skill_node, {{
                    id = "you.luau:near", like = nodes[2].id, x = root.x, y = root.y,
                    name = enshwick.content.guid("you.luau:n"), description = enshwick.content.guid("you.luau:n.text"),
                }})
                enshwick.log.info(`near refused {{not ok}}`)
                local perk = enshwick.content.add_perk({{
                    id = "you.luau:thirst", like = "{PERK}",
                    name = enshwick.content.guid("you.luau:thirst.name"),
                    description = enshwick.content.guid("you.luau:thirst.text"),
                }})
                enshwick.log.info(`perk {{perk}}`)
                "#,
                PERK = enshwick::test::VANILLA_PERK,
            ),
        );
        let logs = format!("{:?}", loader.logs());
        assert!(logged(&loader, "2 nodes, first a Root"), "{logs}");
        let node = Id::new("you.luau:endurance").guid().fnv1a();
        assert!(logged(&loader, &format!("node {node}")), "{logs}");
        assert!(logged(&loader, "near refused true"), "{logs}");
        let perk = Id::new("you.luau:thirst").guid();
        assert!(logged(&loader, &format!("perk {perk}")), "{logs}");
    }

    #[test]
    fn a_script_removes_vanilla_content_and_rekeys_an_item() {
        use enshwick::abi::RecipeRuleKind;
        use enshwick::test::{ENTITY_ITEM, FILL_TABLE, LOOT_ITEM, OTHER_ITEM};

        let loader = run(
            FakeHost::new().removal(),
            &format!(
                r#"
                enshwick.content.remove_item("{LOOT_ITEM}")
                enshwick.content.remove_workshop("{ENTITY_ITEM}")
                enshwick.content.remove_loot_table("{FILL_TABLE}")
                enshwick.content.rekey_item("{OTHER_ITEM}", "{LOOT_ITEM}")
                local ok = pcall(enshwick.content.rekey_item, "{LOOT_ITEM}", "{ENTITY_ITEM}")
                enshwick.log.info(`entity re-key refused {{not ok}}`)
                "#
            ),
        );
        let rules = loader.recipe_rules();
        assert!(
            rules.contains(&(RecipeRuleKind::OUTPUT, LOOT_ITEM.to_string())),
            "{rules:?} {:?}",
            loader.logs()
        );
        assert!(rules.contains(&(RecipeRuleKind::WORKSHOP, ENTITY_ITEM.to_string())));
        assert!(
            logged(&loader, "entity re-key refused true"),
            "{:?}",
            loader.logs()
        );
        let item = FakeHost::new()
            .removal()
            .hash(types::ITEM_INFO.id())
            .unwrap();
        assert!(
            loader
                .resources()
                .iter()
                .any(|r| r.key == OTHER_ITEM.key_by_hash(item))
        );
    }

    #[test]
    fn a_script_adds_a_recipe_beside_a_vanilla_one() {
        let loader = run(
            FakeHost::new().items().recipes(),
            &format!(
                r#"
                enshwick.content.add_item({{ id = "you.luau:locker", like = "{CHEST}" }})
                enshwick.content.add_recipe({{
                    id = "you.luau:locker.recipe",
                    like = "{CHEST}",
                    output = enshwick.content.guid("you.luau:locker"),
                    output_count = 1,
                    cost = 2,
                }})
                "#
            ),
        );
        assert!(
            loader
                .content_ids()
                .contains(&"you.luau:locker.recipe".to_owned())
        );
    }

    #[test]
    fn a_recipe_a_function_makes_is_asked_for_while_the_mod_starts() {
        let loader = run(
            FakeHost::new().items().recipes(),
            &format!(
                r#"
                enshwick.content.add_recipe_with(function()
                    return {{
                        id = "you.luau:box.recipe",
                        like = "{CHEST}",
                        output = enshwick.content.guid("you.luau:box"),
                        output_count = 1,
                        cost = 2,
                    }}
                end)
                "#
            ),
        );
        assert!(
            loader
                .content_ids()
                .contains(&"you.luau:box.recipe".to_owned()),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_changes_a_vanilla_stack_size_and_a_category_of_items() {
        let loader = run(
            FakeHost::new(),
            &format!(
                r#"
                enshwick.content.change({{ entry = "{CHEST}", field = "MaxStackSize", value = {{ Set = 99 }} }}, "stacks")
                enshwick.content.change_items({{ items = {{ Category = {{ "Materials" }} }}, field = "MaxStackSize", value = {{ Times = 4 }} }}, "materials")
                "#
            ),
        );
        let changes = loader.changes();
        assert_eq!(changes.len(), 1, "{:?}", loader.logs());
        assert_eq!(changes[0].0.entry, Guid::parse(CHEST));
        assert_eq!(changes[0].0.field, Field::MaxStackSize);
        assert_eq!(changes[0].0.value, Value::Set(99.0));
        assert_eq!(changes[0].1, "stacks");
        let items = loader.item_changes();
        assert_eq!(
            items[0].0.items,
            Items::Category(ItemCategoryMask::MATERIALS)
        );
        assert_eq!(items[0].0.value, Value::Times(4.0));
    }

    #[test]
    fn a_change_the_api_refuses_raises_an_error_naming_the_call() {
        let loader = run(
            FakeHost::new(),
            r#"
            local ok, why = pcall(enshwick.content.change_items,
                { items = "Stackable", field = "DamageMod", value = { Times = 2 } }, "damage")
            enshwick.log.info(`refused {not ok} {why}`)
            local ok2, why2 = pcall(enshwick.content.change_items,
                { items = "Stackable", field = "Nope", value = { Times = 2 } }, "nope")
            enshwick.log.info(`unknown {not ok2} {why2}`)
            "#,
        );
        assert!(logged(&loader, "refused true"), "{:?}", loader.logs());
        assert!(logged(&loader, "enshwick.content.change_items"));
        assert!(logged(&loader, "unknown true"));
        assert!(logged(&loader, "\"Nope\" is not one of"));
        assert!(loader.item_changes().is_empty());
    }

    #[test]
    fn a_malformed_content_id_fails_the_start_with_the_reason() {
        let (loader, status) = start(
            FakeHost::new().items(),
            &format!(r#"enshwick.content.add_item({{ id = "Locker", like = "{CHEST}" }})"#),
        );
        assert_ne!(status, Status::OK);
        assert!(logged(&loader, "is no content id"), "{:?}", loader.logs());
    }

    #[test]
    fn content_calls_after_the_start_are_refused() {
        let loader = run(
            FakeHost::new(),
            r#"
            local declare = enshwick.content.declare
            enshwick.on("WorldSaved", function()
                local ok, why = pcall(declare, "you.luau:late")
                enshwick.log.info(`late {not ok} {why}`)
            end)
            "#,
        );
        let _handled = loader.raise::<enshwick::events::WorldSaved>(&());
        assert!(logged(&loader, "late true"));
        assert!(loader.content_ids().is_empty());
    }

    #[test]
    fn a_script_reads_vanilla_lookups_and_helpers() {
        let loader = run(
            FakeHost::new(),
            r#"
            enshwick.log.info(`fill {enshwick.content.fill(3, 4)}`)
            enshwick.log.info(`id {enshwick.content.is_id("you.luau:x")} {enshwick.content.is_id("x")}`)
            local ok = pcall(enshwick.content.item, "Chest_Wood")
            enshwick.log.info(`lookup outside the game {not ok}`)
            "#,
        );
        assert!(logged(&loader, "fill 0.75"));
        assert!(logged(&loader, "id true false"));
        assert!(logged(&loader, "lookup outside the game true"));
    }
}
