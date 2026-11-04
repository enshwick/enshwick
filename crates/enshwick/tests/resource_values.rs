//! Game resources as typed values: the type registry, the listing of keys
//! and contents, and a value edit served to the game.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, ResourceKey, Status, TypeEntry};
    use enshwick::content::Guid;
    use enshwick::resources::{Attribute, Constant, Field, Kind, Registry, Type, Value};
    use enshwick::{Load, Mod, Off};

    const TABLE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const OTHER: Guid = Guid::parse("5afe5afe-0000-4000-8000-000000000001");

    static SERVED: OnceLock<Result<(), String>> = OnceLock::new();

    fn types() -> Vec<Type<'static>> {
        vec![
            Type::new("keen::float32", Kind::Float32, 4, 4),
            Type::new("keen::Side", Kind::Enum, 1, 1)
                .with_constants(vec![Constant::new("Left", 0), Constant::new("Right", 1)]),
            Type::new("keen::balance::Table", Kind::Struct, 8, 4)
                .with_names("Table", "balance.Table")
                .with_layout(4, 2, 0xfeed)
                .with_attributes(vec![Attribute::new("converter", "parse")])
                .with_fields(vec![
                    Field::new("damage", 0, "keen::float32").with_attributes(vec![
                        Attribute::new("tooltip", "Damage").with_value_type("keen::float32"),
                    ]),
                    Field::new("side", 4, "keen::Side"),
                    Field::new("pad", 5, "keen::Side").untyped(),
                ]),
        ]
    }

    fn table_key() -> ResourceKey {
        TABLE.key_by_hash(enshwick::resources::fnv1a32(b"keen::balance::Table"))
    }

    fn table_bytes(damage: f32) -> Vec<u8> {
        let mut bytes = damage.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[1, 0, 0, 0]);
        bytes
    }

    struct Doubler;

    impl Mod for Doubler {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let served = (|| {
                let registry = Registry::game()?;
                let table = registry
                    .named("keen::balance::Table")
                    .ok_or_else(|| Off::new("no table type"))?;
                let host = enshwick::host().ok_or_else(|| Off::new("no host"))?;
                for key in host.resource_keys(Some(table))? {
                    load.serve(key, key, move |base| {
                        let mut value = registry.decode("keen::balance::Table", base).ok()?;
                        if let Some(Value::Float(damage)) = value.field_mut("damage") {
                            *damage *= 2.0;
                        }
                        registry
                            .encode("keen::balance::Table", &value, Some(base))
                            .ok()
                    })?;
                }
                let new = OTHER.key_by_hash(enshwick::resources::fnv1a32(b"keen::balance::Table"));
                load.serve_new(
                    new,
                    &Value::Struct(vec![("damage".into(), Value::Float(4.0))]),
                )?;
                Ok::<(), Off>(())
            })();
            let _first = SERVED.set(served.map_err(|off| off.to_string()));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Doubler,
        r#"
        id = "you.doubler"
        side = "both"
        "#
    );

    fn host() -> enshwick::test::FakeHost {
        enshwick::test::FakeHost::new()
            .types(types())
            .resource(table_key(), table_bytes(2.5))
            .resource(OTHER.key_by_hash(7), vec![9])
            .archive([4, 1, 2, 3], vec![5, 6, 7, 8])
    }

    fn start(host: enshwick::test::FakeHost) -> enshwick::test::Installed {
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Doubler)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn every_name_layout_field_constant_and_attribute_crosses_the_abi() {
        let _loader = start(host());
        let registry = Registry::game().unwrap();
        assert_eq!(registry.len(), 3);
        let table = registry.named("keen::balance::Table").unwrap();
        assert_eq!(table, &types()[2]);
        assert_eq!(table.namespace(), "keen::balance");
        assert!(registry.impact_named("balance.Table").is_some());
        let side = registry.named("keen::Side").unwrap();
        assert_eq!(side.constants()[1], Constant::new("Right", 1));
    }

    #[test]
    fn keys_list_by_type_and_all_and_contents_read_by_their_listed_hash() {
        let _loader = start(host());
        let host = enshwick::host().unwrap();
        let registry = Registry::game().unwrap();
        let table = registry.named("keen::balance::Table").unwrap();
        assert_eq!(host.resource_keys(Some(table)).unwrap(), [table_key()]);
        assert_eq!(host.resource_keys(None).unwrap().len(), 2);
        let hashes = host.content_hashes().unwrap();
        assert_eq!(hashes, [[4, 1, 2, 3]]);
        assert_eq!(host.game_content(hashes[0]), Some(vec![5, 6, 7, 8]));
        assert_eq!(host.game_content([4, 9, 9, 9]), None);
    }

    #[test]
    fn content_reads_at_its_archive_length_whatever_its_hash_claims() {
        let _loader = start(host().archive([1, 0, 0, 0], vec![1, 2, 3, 4]));
        let host = enshwick::host().unwrap();
        assert_eq!(host.game_content([1, 0, 0, 0]), Some(vec![1, 2, 3, 4]));
        assert_eq!(host.game_content([u32::MAX, 0, 0, 0]), None);
    }

    #[test]
    fn a_resource_reads_as_its_value_and_a_served_edit_reaches_the_game() {
        let loader = start(host());
        assert_eq!(SERVED.get(), Some(&Ok(())));
        let host = enshwick::host().unwrap();
        let value = host.game_value(table_key()).unwrap();
        assert_eq!(value.field("damage"), Some(&Value::Float(2.5)));
        assert_eq!(value.field("side"), Some(&Value::Name("Right".into())));
        assert_eq!(value.field("pad"), None, "an untyped field is left out");
        let built = loader
            .build_resource(&table_key(), &table_bytes(2.5))
            .unwrap();
        assert_eq!(built, table_bytes(5.0));
        assert!(
            host.game_value(OTHER.key_by_hash(7)).is_err(),
            "a type the build lacks"
        );
    }

    #[test]
    fn without_types_the_registry_is_off_and_short_entries_are_refused() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Doubler)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(Registry::game().is_err());
        assert!(matches!(SERVED.get(), Some(Err(_))));
        drop(loader);
        let loader = start(host());
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let mut out = [TypeEntry::EMPTY; 3];
        let mut count = 0u32;
        let header = u32::try_from(size_of::<Header>()).unwrap();
        let refused = (api.types)(
            loader.handle(),
            out.as_mut_ptr(),
            header - 1,
            3,
            &raw mut count,
        );
        assert_eq!(refused, Status::BAD_ARGS);
        let listed = (api.types)(loader.handle(), out.as_mut_ptr(), header, 3, &raw mut count);
        assert_eq!((listed, count, out[0].header.size), (Status::OK, 3, header));
    }

    #[test]
    fn a_new_resource_is_served_from_a_value_alone_under_its_key() {
        let loader = start(host());
        assert_eq!(SERVED.get(), Some(&Ok(())));
        let new = OTHER.key_by_hash(enshwick::resources::fnv1a32(b"keen::balance::Table"));
        let mut expected = 4.0f32.to_le_bytes().to_vec();
        expected.extend_from_slice(&[0; 4]);
        assert_eq!(
            loader.build_resource(&new, &table_bytes(2.5)).unwrap(),
            expected
        );
    }
}
