//! A Luau mod reads the game's types and resources as tables and serves an
//! edit through `enshwick.resources`, the same calls a Rust mod makes.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::Guid;
    use enshwick::resources::{Constant, Field, Kind, Type, fnv1a32};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};

    const TABLE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");

    static CASE: Mutex<Option<Scripts>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scripts = CASE.lock().unwrap().take().unwrap();
            let _runtime =
                enshwick_luau::start(load, "you.luau", scripts, None, Limits::default())?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Luau,
        r#"
        id = "you.luau"
        side = "both"
        "#
    );

    fn key() -> ResourceKey {
        TABLE.key_by_hash(fnv1a32(b"demo::balance::Table"))
    }

    fn bytes(damage: f32, count: u64) -> Vec<u8> {
        let mut bytes = damage.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[1, 0, 0, 0]);
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes
    }

    fn run(main: &str) -> Installed {
        *CASE.lock().unwrap() = Some(Scripts::new().with("main.luau", main));
        let loader = FakeHost::new()
            .types([
                Type::new("demo::float32", Kind::Float32, 4, 4),
                Type::new("demo::uint64", Kind::Uint64, 8, 8),
                Type::new("demo::Side", Kind::Enum, 1, 1)
                    .with_constants(vec![Constant::new("Left", 0), Constant::new("Right", 1)]),
                Type::new("demo::balance::Table", Kind::Struct, 16, 8).with_fields(vec![
                    Field::new("damage", 0, "demo::float32"),
                    Field::new("side", 4, "demo::Side"),
                    Field::new("count", 8, "demo::uint64"),
                ]),
            ])
            .resource(key(), bytes(2.5, u64::MAX))
            .archive([4, 1, 2, 3], vec![5, 6, 7, 8])
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn logged(loader: &Installed, text: &str) -> bool {
        loader.logs().iter().any(|(_, line)| line.contains(text))
    }

    #[test]
    fn a_script_reads_a_type_and_a_resource_as_tables() {
        let loader = run(r#"
            local table = enshwick.resources.named("demo::balance::Table")
            print(`type {table.name} in {table.namespace} fields {#table.fields} second {table.fields[2].name}`)
            local keys = enshwick.resources.resource_keys("demo::balance::Table")
            local value = enshwick.resources.game_value(keys[1])
            print(`keys {#keys} damage {value.damage} side {value.side} count {value.count}`)
            print(`missing {enshwick.resources.named("demo::Nothing") == nil}`)
        "#);
        assert!(
            logged(&loader, "type Table in demo::balance fields 3 second side"),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(
                &loader,
                "keys 1 damage 2.5 side Right count 18446744073709551615"
            ),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "missing true"), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_serves_an_edit_that_names_only_what_it_changes() {
        let loader = run(r#"
            for _, key in enshwick.resources.resource_keys("demo::balance::Table") do
                enshwick.resources.serve(key, key, function(value)
                    return { damage = value.damage * 2, side = "Left", count = "12345678901234567890" }
                end)
            end
        "#);
        let built = loader
            .build_resource(&key(), &bytes(2.5, u64::MAX))
            .unwrap();
        let mut expected = bytes(5.0, 12_345_678_901_234_567_890);
        expected[4] = 0;
        assert_eq!(built, expected, "{:?}", loader.logs());
    }

    #[test]
    fn a_patch_that_errs_or_returns_nil_leaves_the_resource_as_it_was() {
        let loader = run(r#"
            local key = enshwick.resources.resource_keys("demo::balance::Table")[1]
            enshwick.resources.serve(key, key, function(value)
                return nil
            end)
        "#);
        let base = bytes(2.5, 7);
        assert_eq!(loader.build_resource(&key(), &base).unwrap(), base);
    }

    #[test]
    fn a_script_reads_content_bytes_and_codes_a_resource_through_a_buffer() {
        let loader = run(r#"
            local data = enshwick.resources.game_content(enshwick.resources.content_hashes()[1])
            print(`content {buffer.len(data)} first {buffer.readu8(data, 0)}`)
            local key = enshwick.resources.resource_keys("demo::balance::Table")[1]
            local bytes = enshwick.resources.encode("demo::balance::Table", enshwick.resources.game_value(key))
            local again = enshwick.resources.decode("demo::balance::Table", bytes)
            print(`round {again.damage} {buffer.len(bytes)}`)
            print(`served {enshwick.resources.serve_content(buffer.fromstring("abcd"))[1]}`)
            print(`missing {enshwick.resources.game_content({4, 9, 9, 9}) == nil}`)
        "#);
        for line in [
            "content 4 first 5",
            "round 2.5 16",
            "served 4",
            "missing true",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_hashes_bytes_and_turns_content_hashes_into_guids_and_back() {
        let loader = run(r#"
            local r = enshwick.resources
            print(`crc32 {r.crc32("123456789")} crc64 {r.crc64("123456789")} fnv {r.fnv1a32("foobar")}`)
            local guid = r.guid_from_content_hash({1, 2, 3, 4})
            local back = r.content_hash_of_guid(guid)
            print(`back {back[1]} {back[4]} nil {r.nil_guid()}`)
            print(`hash {r.guid_hash("6943053a-32c1-4718-ae22-e61427ddf54d")}`)
        "#);
        let id = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d").fnv1a();
        for line in [
            "crc32 3421780262 crc64 7800480153909949255 fnv 3214735720".to_owned(),
            format!("back 1 4 nil {}", Guid::NIL),
            format!("hash {id}"),
        ] {
            assert!(logged(&loader, &line), "{line}: {:?}", loader.logs());
        }
    }
}
