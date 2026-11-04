//! Scripts written for another script host, ported to Luau from what they do:
//! each runs on the fake host over a small archive, and the test checks the
//! bytes the game would read through the served path.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::Guid;
    use enshwick::resources::{Constant, Field, Kind, Registry, Type, Value, fnv1a32};
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
}
