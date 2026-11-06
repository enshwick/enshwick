//! Mods add skill nodes linked from vanilla ones, within the room a
//! player's save keeps, and perks copied from vanilla ones; a node on
//! another, beyond the room or from nowhere is refused.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::skills::{
        NewPerk, NewSkillNode, PERK_COLLECTION, SKILL_TREE, SkillNode, SkillNodeId,
    };
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{ATTRIBUTE_NODE, FakeHost, ROOT_NODE, VANILLA_PERK};
    use enshwick::typed::SkillNodeType;
    use enshwick::{Load, Mod, Off};

    static LISTED: Mutex<Vec<SkillNode>> = Mutex::new(Vec::new());
    static ADDED: Mutex<Vec<SkillNodeId>> = Mutex::new(Vec::new());
    static PERK: Mutex<Option<Guid>> = Mutex::new(None);
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn refused(result: Result<impl Sized, Off>) {
        if let Err(off) = result {
            REFUSED.lock().unwrap().push(off.to_string());
        }
    }

    fn node(name: &'static str, y: f32) -> NewSkillNode {
        NewSkillNode {
            links_from: vec![SkillNodeId::new(ROOT_NODE)],
            cost: Some(2),
            kind: Some(SkillNodeType::PassiveSkill),
            ..NewSkillNode::new(
                Id::new(name),
                SkillNodeId::new(ATTRIBUTE_NODE),
                0.0,
                y,
                Guid::named("you.skills:name"),
                Guid::named("you.skills:text"),
            )
        }
    }

    struct Skills;

    impl Mod for Skills {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *LISTED.lock().unwrap() = load.skill_nodes()?;
            let first = load.add_skill_node(node("you.skills:first", 300.0))?;
            load.add_skill_node(NewSkillNode {
                links_from: vec![SkillNodeId::new(9)],
                ..node("you.skills:nowhere", 900.0)
            })?;
            load.add_skill_node(NewSkillNode {
                links_from: vec![first],
                ..node("you.skills:second", 600.0)
            })?;
            refused(load.add_skill_node(node("you.skills:near", 50.0)));
            // Passes at init against the vanilla tree; the room is gone once
            // the first two are in.
            let third = load.add_skill_node(node("you.skills:third", -300.0))?;
            ADDED.lock().unwrap().extend([first, third]);
            *PERK.lock().unwrap() = Some(load.add_perk(NewPerk {
                id: Id::new("you.skills:thirst"),
                like: VANILLA_PERK,
                name: Guid::named("you.skills:thirst.name"),
                description: Guid::named("you.skills:thirst.text"),
            })?);
            refused(load.add_perk(NewPerk {
                id: Id::new("you.skills:nothing"),
                like: Guid([9; 16]),
                name: Guid::named("you.skills:n"),
                description: Guid::named("you.skills:n.text"),
            }));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Skills,
        r#"
        id = "you.skills"
        side = "both"
        "#
    );

    fn start() -> enshwick::test::Installed {
        let loader = FakeHost::new().progression().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Skills)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn built(
        loader: &enshwick::test::Installed,
        key: enshwick::abi::ResourceKey,
        ty: &str,
    ) -> Value {
        let base = enshwick::host().unwrap().game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        Registry::game().unwrap().decode(ty, &bytes).unwrap()
    }

    fn ids(list: Option<&Value>) -> Vec<u64> {
        match list {
            Some(Value::List(items)) => items
                .iter()
                .filter_map(
                    |item| match item.field("id").and_then(|id| id.field("value")) {
                        Some(Value::Uint(id)) => Some(*id),
                        _ => None,
                    },
                )
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn nodes_join_the_tree_linked_from_vanilla_ones_until_the_saves_room_is_gone() {
        let loader = start();
        let listed = LISTED.lock().unwrap().clone();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].kind, SkillNodeType::Root);
        let host = enshwick::host().unwrap();
        let key = SKILL_TREE.key_by_hash(host.hash("SkillTreeResource").unwrap());
        let tree = built(&loader, key, "demo::SkillTreeResource");
        let added = ADDED.lock().unwrap().clone();
        let nodes = ids(tree.field("nodes"));
        assert_eq!(
            nodes,
            [
                u64::from(ROOT_NODE),
                u64::from(ATTRIBUTE_NODE),
                u64::from(added[0].get()),
                u64::from(Id::new("you.skills:second").guid().fnv1a()),
            ],
            "the third is past the room of 4"
        );
        let Some(Value::List(all)) = tree.field("nodes") else {
            panic!("{tree:?}")
        };
        assert_eq!(
            all[2].field("type"),
            Some(&Value::Name("PassiveSkill".into()))
        );
        assert_eq!(all[2].field("costs"), Some(&Value::Uint(2)));
        assert_eq!(
            ids(tree.field("links")).len(),
            3,
            "the vanilla link and one each"
        );
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("you.skills:third")
                    && line.contains("the most a player's save keeps")),
            "{:?}",
            loader.logs()
        );

        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("you.skills:nowhere") && line.contains("no node 9")),
            "a link from a node no mod adds: {:?}",
            loader.logs()
        );
        let features = loader.features();
        let off = |id: &str| {
            features
                .iter()
                .find(|(name, _)| name == &format!("skill node {id}"))
                .map(|(_, off)| off.clone())
        };
        assert_eq!(off("you.skills:first"), Some(None), "{features:?}");
        for (id, why) in [
            ("you.skills:third", "the most a player's save keeps"),
            ("you.skills:near", "lies within"),
            ("you.skills:nowhere", "no node 9"),
        ] {
            assert!(
                off(id).flatten().is_some_and(|off| off.contains(why)),
                "{id} shows why it is refused: {features:?}"
            );
        }
        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 2, "{refused:?}");
        assert!(refused[0].contains("lies within"), "{}", refused[0]);
        assert!(refused[1].contains("no vanilla perk"), "{}", refused[1]);
    }

    #[test]
    fn a_perk_copies_a_vanilla_one_with_its_own_texts_and_joins_the_collection() {
        let loader = start();
        let host = enshwick::host().unwrap();
        let perk = PERK.lock().unwrap().unwrap();
        assert_eq!(perk, Id::new("you.skills:thirst").guid());
        let hash = host.hash("Perk").unwrap();
        let key = perk.key_by_hash(hash);
        let base = host.game_resource(VANILLA_PERK.key_by_hash(hash)).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        let value = Registry::game()
            .unwrap()
            .decode("demo::Perk", &bytes)
            .unwrap();
        assert_eq!(
            value.field("name"),
            Some(&Value::Guid(Guid::named("you.skills:thirst.name")))
        );
        let collection = PERK_COLLECTION.key_by_hash(host.hash("PerkCollectionResource").unwrap());
        let perks = built(&loader, collection, "demo::PerkCollectionResource");
        assert_eq!(
            perks.field("perks"),
            Some(&Value::List(vec![
                Value::Guid(VANILLA_PERK),
                Value::Guid(perk)
            ]))
        );
    }
}
