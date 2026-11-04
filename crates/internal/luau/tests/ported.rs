//! Scripts written for another script host, ported to Luau from what they do:
//! each runs on the fake host over a small archive, and the test checks the
//! bytes the game would read through the served path.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::Guid;
    use enshwick::resources::{Constant, Field, Kind, Registry, Type, Value, Variant, fnv1a32};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};

    static CASE: Mutex<Option<Scripts>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scripts = CASE.lock().unwrap().take().unwrap();
            let _runtime =
                enshwick_luau::start(load, "you.ported", scripts, None, Limits::default())?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Luau,
        r#"
        id = "you.ported"
        side = "both"
        "#
    );

    const REGISTRY: Guid = Guid::parse("0b6b1f7e-3f43-4a2e-9a0d-5f2c1e7d8a90");

    /// A marker registry: a blob array of markers, each an id, a sorting
    /// group and two flags.
    fn marker_types() -> Vec<Type<'static>> {
        vec![
            Type::new("demo::bool", Kind::Bool, 1, 1),
            Type::new("demo::uint32", Kind::Uint32, 4, 4),
            Type::new("demo::Sorting", Kind::Enum, 1, 1).with_constants(vec![
                Constant::new("None", 0),
                Constant::new("FlameRelated", 1),
            ]),
            Type::new("demo::Marker", Kind::Struct, 8, 4).with_fields(vec![
                Field::new("markerId", 0, "demo::uint32"),
                Field::new("sortingCategory", 4, "demo::Sorting"),
                Field::new("isCustomMarkerType", 5, "demo::bool"),
                Field::new("isFastTravelDestination", 6, "demo::bool"),
            ]),
            Type::new("demo::BlobArray<demo::Marker>", Kind::BlobArray, 8, 4)
                .with_inner("demo::Marker"),
            Type::new("demo::MarkerRegistry", Kind::Struct, 8, 4).with_fields(vec![Field::new(
                "mapMarkers",
                0,
                "demo::BlobArray<demo::Marker>",
            )]),
        ]
    }

    fn marker(id: u32, flame: bool) -> Value {
        Value::Struct(vec![
            ("markerId".into(), Value::Uint(id.into())),
            (
                "sortingCategory".into(),
                Value::Name(if flame { "FlameRelated" } else { "None" }.into()),
            ),
            ("isCustomMarkerType".into(), Value::Bool(false)),
            ("isFastTravelDestination".into(), Value::Bool(false)),
        ])
    }

    fn registry_key() -> ResourceKey {
        REGISTRY.key_by_hash(fnv1a32(b"demo::MarkerRegistry"))
    }

    fn run(
        types: Vec<Type<'static>>,
        resources: Vec<(ResourceKey, Vec<u8>)>,
        main: &str,
    ) -> Installed {
        *CASE.lock().unwrap() = Some(Scripts::new().with("main.luau", main));
        let mut host = FakeHost::new().types(types);
        for (key, bytes) in resources {
            host = host.resource(key, bytes);
        }
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// Ported script 1: one map marker type, found by its id and checked for
    /// its group and flags, becomes a fast-travel destination; nothing else
    /// changes.
    #[test]
    fn ported_script_one_makes_one_marker_type_a_travel_destination() {
        let types = marker_types();
        let registry = Registry::new(types.clone());
        let vanilla = Value::Struct(vec![(
            "mapMarkers".into(),
            Value::List(vec![
                marker(7, false),
                marker(2_068_618_774, true),
                marker(9, true),
            ]),
        )]);
        let base = registry
            .encode("demo::MarkerRegistry", &vanilla, None)
            .unwrap();
        let loader = run(
            types,
            vec![(registry_key(), base.clone())],
            r#"
            local keys = enshwick.resources.resource_keys("demo::MarkerRegistry")
            assert(#keys == 1, "one marker registry")
            enshwick.resources.serve(keys[1], keys[1], function(registry)
                local found = 0
                for _, marker in registry.mapMarkers do
                    if marker.markerId == 2068618774 then
                        assert(marker.sortingCategory == "FlameRelated", "a flame marker")
                        assert(not marker.isCustomMarkerType and not marker.isFastTravelDestination)
                        marker.isFastTravelDestination = true
                        found += 1
                    end
                end
                assert(found == 1, "exactly one marker type")
                return registry
            end)
            print("travel set on 1 marker type")
            "#,
        );
        let built = loader.build_resource(&registry_key(), &base).unwrap();
        let mut expected = vanilla;
        let Some(Value::List(markers)) = expected.field_mut("mapMarkers") else {
            panic!("markers")
        };
        *markers[1].field_mut("isFastTravelDestination").unwrap() = Value::Bool(true);
        assert_eq!(
            registry.decode("demo::MarkerRegistry", &built).unwrap(),
            expected
        );
        assert_eq!(built.len(), base.len(), "a change in place");
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, l)| l.contains("travel set on 1")),
            "{:?}",
            loader.logs()
        );
    }

    /// Templates whose components are variants, and a list of steps.
    fn template_types() -> Vec<Type<'static>> {
        vec![
            Type::new("demo::uint16", Kind::Uint16, 2, 2),
            Type::new("demo::float32", Kind::Float32, 4, 4),
            Type::new("demo::BlobString", Kind::BlobString, 8, 4),
            Type::new("demo::Component", Kind::Struct, 4, 4),
            Type::new("demo::PickupItem", Kind::Struct, 4, 4)
                .with_inner("demo::Component")
                .with_fields(vec![Field::new("pickupAmount", 0, "demo::uint16")]),
            Type::new("demo::Weight", Kind::Struct, 4, 4)
                .with_inner("demo::Component")
                .with_fields(vec![Field::new("weight", 0, "demo::float32")]),
            Type::new(
                "demo::BlobVariant<demo::Component>",
                Kind::BlobVariant,
                12,
                4,
            )
            .with_inner("demo::Component"),
            Type::new(
                "demo::BlobArray<demo::BlobVariant<demo::Component>>",
                Kind::BlobArray,
                8,
                4,
            )
            .with_inner("demo::BlobVariant<demo::Component>"),
            Type::new("demo::Template", Kind::Struct, 16, 4).with_fields(vec![
                Field::new("name", 0, "demo::BlobString"),
                Field::new(
                    "components",
                    8,
                    "demo::BlobArray<demo::BlobVariant<demo::Component>>",
                ),
            ]),
            Type::new("demo::BlobArray<demo::float32>", Kind::BlobArray, 8, 4)
                .with_inner("demo::float32"),
            Type::new("demo::Steps", Kind::Struct, 8, 4).with_fields(vec![Field::new(
                "steps",
                0,
                "demo::BlobArray<demo::float32>",
            )]),
        ]
    }

    fn component(ty: &str, field: &str, value: Value) -> Value {
        Value::Variant(Box::new(Variant::new(
            ty,
            Value::Struct(vec![(field.into(), value)]),
        )))
    }

    fn template(name: &str, components: Vec<Value>) -> Value {
        Value::Struct(vec![
            ("name".into(), Value::Text(name.into())),
            ("components".into(), Value::List(components)),
        ])
    }

    /// Ported script 2: pickups of loot templates, matched by name, yield
    /// three times as much up to the field's limit, and a descending list of
    /// steps grows at its front.
    #[test]
    fn ported_script_two_triples_loot_pickups_and_grows_a_list_of_steps() {
        let types = template_types();
        let registry = Registry::new(types.clone());
        let wood = template(
            "LootPickup_Wood",
            vec![
                component("demo::PickupItem", "pickupAmount", Value::Uint(30_000)),
                component("demo::Weight", "weight", Value::Float(1.5)),
            ],
        );
        let tree = template(
            "Tree_Oak",
            vec![component(
                "demo::PickupItem",
                "pickupAmount",
                Value::Uint(5),
            )],
        );
        let steps = Value::Struct(vec![(
            "steps".into(),
            Value::List(vec![Value::Float(3.0), Value::Float(2.0)]),
        )]);
        let key = |name: &[u8], ty: &[u8]| {
            Guid::named(core::str::from_utf8(name).unwrap()).key_by_hash(fnv1a32(ty))
        };
        let (wood_key, tree_key, steps_key) = (
            key(b"you.ported:wood", b"demo::Template"),
            key(b"you.ported:tree", b"demo::Template"),
            key(b"you.ported:steps", b"demo::Steps"),
        );
        let encode = |ty: &str, value: &Value| registry.encode(ty, value, None).unwrap();
        let (wood_bytes, tree_bytes, steps_bytes) = (
            encode("demo::Template", &wood),
            encode("demo::Template", &tree),
            encode("demo::Steps", &steps),
        );
        let loader = run(
            types,
            vec![
                (wood_key, wood_bytes.clone()),
                (tree_key, tree_bytes.clone()),
                (steps_key, steps_bytes.clone()),
            ],
            r#"
            local r = enshwick.resources
            for _, key in r.resource_keys("demo::Template") do
                r.serve(key, key, function(t)
                    if not string.match(t.name, "^LootPickup_") then
                        return nil
                    end
                    for _, c in t.components do
                        if c.type == "demo::PickupItem" and c.value.pickupAmount > 0 then
                            c.value.pickupAmount = math.min(c.value.pickupAmount * 3, 65535)
                        end
                    end
                    return t
                end)
            end
            local steps = r.resource_keys("demo::Steps")[1]
            r.serve(steps, steps, function(s)
                local added = { 5.0, 4.0 }
                for i = #added, 1, -1 do
                    if added[i] > s.steps[1] then
                        table.insert(s.steps, 1, added[i])
                    end
                end
                return s
            end)
            "#,
        );
        let built = |key, base: &[u8], ty| {
            registry
                .decode(ty, &loader.build_resource(key, base).unwrap())
                .unwrap()
        };
        let mut tripled = wood;
        let Some(Value::List(parts)) = tripled.field_mut("components") else {
            panic!("components")
        };
        parts[0] = component("demo::PickupItem", "pickupAmount", Value::Uint(65_535));
        assert_eq!(built(&wood_key, &wood_bytes, "demo::Template"), tripled);
        assert_eq!(
            built(&tree_key, &tree_bytes, "demo::Template"),
            tree,
            "not loot"
        );
        let grown = Value::Struct(vec![(
            "steps".into(),
            Value::List([5.0, 4.0, 3.0, 2.0].map(Value::Float).to_vec()),
        )]);
        assert_eq!(built(&steps_key, &steps_bytes, "demo::Steps"), grown);
    }

    /// An item, the item registry, and a text table held in a content.
    fn item_types() -> Vec<Type<'static>> {
        vec![
            Type::new("demo::uint32", Kind::Uint32, 4, 4),
            Type::new("demo::Guid", Kind::Guid, 16, 4),
            Type::new("demo::BlobString", Kind::BlobString, 8, 4),
            Type::new("demo::Item", Kind::Struct, 20, 4).with_fields(vec![
                Field::new("id", 0, "demo::uint32"),
                Field::new("guid", 4, "demo::Guid"),
            ]),
            Type::new("demo::BlobArray<demo::Guid>", Kind::BlobArray, 8, 4)
                .with_inner("demo::Guid"),
            Type::new("demo::BlobArray<demo::BlobString>", Kind::BlobArray, 8, 4)
                .with_inner("demo::BlobString"),
            Type::new("demo::ItemRegistry", Kind::Struct, 16, 4).with_fields(vec![
                Field::new("itemRefs", 0, "demo::BlobArray<demo::Guid>"),
                Field::new("dbgNames", 8, "demo::BlobArray<demo::BlobString>"),
            ]),
            Type::new(
                "demo::StaticArray<demo::uint32,4>",
                Kind::StaticArray,
                16,
                4,
            )
            .with_inner("demo::uint32")
            .with_count(4),
            Type::new("demo::Texts", Kind::Struct, 16, 4).with_fields(vec![Field::new(
                "data",
                0,
                "demo::StaticArray<demo::uint32,4>",
            )]),
        ]
    }

    /// Ported script 3: a new item copies a vanilla one under a declared
    /// GUID, takes that GUID's hash as its id and joins the item registry,
    /// and its text joins the text table, served as new content.
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one ported script: its fixture, its run and the bytes the game reads"
    )]
    fn ported_script_three_clones_an_item_registers_it_and_adds_its_text() {
        let types = item_types();
        let registry = Registry::new(types.clone());
        let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
        let item = Value::Struct(vec![
            ("id".into(), Value::Uint(chest.fnv1a().into())),
            ("guid".into(), Value::Guid(chest)),
        ]);
        let items = Value::Struct(vec![
            ("itemRefs".into(), Value::List(vec![Value::Guid(chest)])),
            (
                "dbgNames".into(),
                Value::List(vec![Value::Text("chest".into())]),
            ),
        ]);
        let text_hash = [3u32, 1, 2, 3];
        let texts = Value::Struct(vec![(
            "data".into(),
            Value::List(text_hash.map(|w| Value::Uint(w.into())).to_vec()),
        )]);
        let key = |ty: &[u8]| Guid::named("you.ported:fixture").key_by_hash(fnv1a32(ty));
        let encode = |ty: &str, value: &Value| registry.encode(ty, value, None).unwrap();
        let (items_key, texts_key) = (key(b"demo::ItemRegistry"), key(b"demo::Texts"));
        let (items_bytes, texts_bytes) = (
            encode("demo::ItemRegistry", &items),
            encode("demo::Texts", &texts),
        );
        *CASE.lock().unwrap() = Some(Scripts::new().with(
            "main.luau",
            r#"
            local r = enshwick.resources
            local guid = r.declare("you.ported:lantern")
            local item = r.game_value(r.resource_keys("demo::Item")[1])
            item.id = r.guid_hash(guid)
            item.guid = guid
            r.serve_new({ guid = guid, type = "demo::Item" }, item)
            local items = r.resource_keys("demo::ItemRegistry")[1]
            r.serve(items, items, function(registry)
                table.insert(registry.itemRefs, guid)
                table.insert(registry.dbgNames, "lantern")
                return registry
            end)
            local texts = r.resource_keys("demo::Texts")[1]
            local old = buffer.tostring(r.game_content(r.game_value(texts).data))
            local hash = r.serve_content(buffer.fromstring(old .. "\nlantern=Lantern"))
            r.serve(texts, texts, function(table)
                table.data = hash
                return table
            end)
            "#,
        ));
        let loader = FakeHost::new()
            .types(types)
            .resource(
                chest.key_by_hash(fnv1a32(b"demo::Item")),
                encode("demo::Item", &item),
            )
            .resource(items_key, items_bytes.clone())
            .resource(texts_key, texts_bytes.clone())
            .archive(text_hash, b"a=A".to_vec())
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let built = |key, base: &[u8], ty| {
            registry
                .decode(ty, &loader.build_resource(key, base).unwrap())
                .unwrap()
        };
        let registered = built(&items_key, &items_bytes, "demo::ItemRegistry");
        let Some(Value::List(refs)) = registered.field("itemRefs") else {
            panic!("refs")
        };
        let Value::Guid(lantern) = refs[1] else {
            panic!("{refs:?}")
        };
        assert_eq!(
            registered.field("dbgNames"),
            Some(&Value::List(vec![
                Value::Text("chest".into()),
                Value::Text("lantern".into())
            ]))
        );
        let lantern_key = lantern.key_by_hash(fnv1a32(b"demo::Item"));
        let blank = [0u8; 20];
        let new = built(&lantern_key, &blank, "demo::Item");
        assert_eq!(new.field("id"), Some(&Value::Uint(lantern.fnv1a().into())));
        assert_eq!(new.field("guid"), Some(&Value::Guid(lantern)));
        let table = built(&texts_key, &texts_bytes, "demo::Texts");
        let Some(Value::List(words)) = table.field("data") else {
            panic!("data")
        };
        let hash: Vec<u32> = words
            .iter()
            .map(|w| match w {
                Value::Uint(w) => u32::try_from(*w).unwrap(),
                _ => 0,
            })
            .collect();
        let host = enshwick::host().unwrap();
        assert_eq!(
            host.game_content(hash.try_into().unwrap()),
            Some(b"a=A\nlantern=Lantern".to_vec())
        );
    }

    /// A skill tree: nodes with an id, a cost, a position and the ids they
    /// link to.
    fn skill_types() -> Vec<Type<'static>> {
        vec![
            Type::new("demo::uint32", Kind::Uint32, 4, 4),
            Type::new("demo::float32", Kind::Float32, 4, 4),
            Type::new("demo::Position", Kind::Struct, 8, 4).with_fields(vec![
                Field::new("x", 0, "demo::float32"),
                Field::new("y", 4, "demo::float32"),
            ]),
            Type::new("demo::BlobArray<demo::uint32>", Kind::BlobArray, 8, 4)
                .with_inner("demo::uint32"),
            Type::new("demo::SkillNode", Kind::Struct, 24, 4).with_fields(vec![
                Field::new("id", 0, "demo::uint32"),
                Field::new("cost", 4, "demo::uint32"),
                Field::new("position", 8, "demo::Position"),
                Field::new("links", 16, "demo::BlobArray<demo::uint32>"),
            ]),
            Type::new("demo::BlobArray<demo::SkillNode>", Kind::BlobArray, 8, 4)
                .with_inner("demo::SkillNode"),
            Type::new("demo::SkillTree", Kind::Struct, 8, 4).with_fields(vec![Field::new(
                "nodes",
                0,
                "demo::BlobArray<demo::SkillNode>",
            )]),
        ]
    }

    fn node(id: u32, cost: u32, x: f32, links: &[u32]) -> Value {
        Value::Struct(vec![
            ("id".into(), Value::Uint(id.into())),
            ("cost".into(), Value::Uint(cost.into())),
            (
                "position".into(),
                Value::Struct(vec![
                    ("x".into(), Value::Float(x.into())),
                    ("y".into(), Value::Float(0.0)),
                ]),
            ),
            (
                "links".into(),
                Value::List(links.iter().map(|l| Value::Uint((*l).into())).collect()),
            ),
        ])
    }

    /// Ported script 4: a skill costs less, one link moves from one node to
    /// another, and a node moves on the tree.
    #[test]
    fn ported_script_four_reprices_relinks_and_moves_skill_nodes() {
        let types = skill_types();
        let registry = Registry::new(types.clone());
        let tree = Value::Struct(vec![(
            "nodes".into(),
            Value::List(vec![
                node(1, 1, 0.0, &[2, 3]),
                node(2, 4, 10.0, &[1]),
                node(3, 1, 20.0, &[1]),
            ]),
        )]);
        let key = Guid::named("you.ported:tree").key_by_hash(fnv1a32(b"demo::SkillTree"));
        let base = registry.encode("demo::SkillTree", &tree, None).unwrap();
        let loader = run(
            types,
            vec![(key, base.clone())],
            r#"
            local function find(tree, id)
                for _, n in tree.nodes do
                    if n.id == id then
                        return n
                    end
                end
                error(`no skill {id}`)
            end
            local function unlink(tree, from, to)
                local links = find(tree, from).links
                table.remove(links, table.find(links, to))
            end
            local key = enshwick.resources.resource_keys("demo::SkillTree")[1]
            enshwick.resources.serve(key, key, function(tree)
                find(tree, 2).cost = 3
                unlink(tree, 1, 2)
                unlink(tree, 2, 1)
                table.insert(find(tree, 3).links, 2)
                find(tree, 2).position = { x = 15.5, y = 2.5 }
                return tree
            end)
            "#,
        );
        let built = registry
            .decode(
                "demo::SkillTree",
                &loader.build_resource(&key, &base).unwrap(),
            )
            .unwrap();
        let mut moved = node(2, 3, 15.5, &[]);
        *moved.field_mut("position").unwrap() = Value::Struct(vec![
            ("x".into(), Value::Float(15.5)),
            ("y".into(), Value::Float(2.5)),
        ]);
        let expected = Value::Struct(vec![(
            "nodes".into(),
            Value::List(vec![
                node(1, 1, 0.0, &[3]),
                moved,
                node(3, 1, 20.0, &[1, 2]),
            ]),
        )]);
        assert_eq!(built, expected);
    }

    /// A ported script on the real server archive beside `ENSH_SERVER_EXE`
    /// the balancing table's two-handed damage factor doubles.
    /// Without the executable the test passes trivially.
    #[test]
    fn a_ported_script_doubles_a_real_balancing_value_on_the_server_archive() {
        use enshwick_gamedata::archive::Archive;
        use enshwick_gamedata::reflect::{Image, Layout, Reflection};

        let Ok(exe) = std::env::var("ENSH_SERVER_EXE") else {
            return;
        };
        let exe = std::path::Path::new(&exe);
        let bytes: &'static [u8] = Box::leak(std::fs::read(exe).unwrap().into_boxed_slice());
        let image: &'static Image<'static> =
            Box::leak(Box::new(Image::parse(bytes, Layout::File, None).unwrap()));
        let reflection: &'static Reflection<'static> = Box::leak(Box::new(Reflection::scan(image)));
        let registry = reflection.registry();
        let archive = Archive::open(
            exe.parent().unwrap(),
            exe.file_stem().unwrap().to_str().unwrap(),
        )
        .unwrap();
        let table = registry.named("keen::BalancingTable").unwrap();
        let key = archive.keys_of(table.qualified_hash())[0];
        let base = archive.resource(&key).unwrap();
        let types: Vec<Type<'static>> = registry.iter().cloned().collect();
        let loader = run(
            types,
            vec![(key, base.clone())],
            r#"
            local key = enshwick.resources.resource_keys("keen::BalancingTable")[1]
            enshwick.resources.serve(key, key, function(table)
                return { damageMod2Handed = table.damageMod2Handed * 2 }
            end)
            "#,
        );
        let built = registry
            .decode(
                "keen::BalancingTable",
                &loader.build_resource(&key, &base).unwrap(),
            )
            .unwrap();
        let vanilla = registry.decode("keen::BalancingTable", &base).unwrap();
        let (Some(Value::Float(new)), Some(Value::Float(old))) = (
            built.field("damageMod2Handed"),
            vanilla.field("damageMod2Handed"),
        ) else {
            panic!("no damageMod2Handed")
        };
        assert!((new - old * 2.0).abs() < 1e-6, "{old} became {new}");
    }
}
