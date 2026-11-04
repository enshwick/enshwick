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
}
