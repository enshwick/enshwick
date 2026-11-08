//! A Luau mod reads the game's types and resources as tables and serves an
//! edit through `enshwick.resources`, the same calls a Rust mod makes.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ResourceKey, Side, Status};
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

    fn extra_key() -> ResourceKey {
        Guid::parse("5afe5afe-0000-4000-8000-000000000002").key_by_hash(fnv1a32(b"demo::Extra"))
    }

    /// `demo::Extra` with its bonus 1.5, its pair's first absent and its
    /// second 2.5; each optional's offset counts from itself.
    fn extra() -> Vec<u8> {
        let mut bytes = [12u32, 0, 8].map(u32::to_le_bytes).concat();
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        bytes.extend_from_slice(&2.5f32.to_le_bytes());
        bytes
    }

    fn run(main: &str) -> Installed {
        run_on(FakeHost::new(), main)
    }

    fn run_on(host: FakeHost, main: &str) -> Installed {
        *CASE.lock().unwrap() = Some(Scripts::new().with("main.luau", main));
        let loader = host
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
                Type::new("demo::Bonus", Kind::BlobOptional, 4, 4).with_inner("demo::float32"),
                Type::new("demo::Pair", Kind::StaticArray, 8, 4)
                    .with_inner("demo::Bonus")
                    .with_count(2),
                Type::new("demo::Extra", Kind::Struct, 12, 4).with_fields(vec![
                    Field::new("bonus", 0, "demo::Bonus"),
                    Field::new("pair", 4, "demo::Pair"),
                ]),
            ])
            .resource(key(), bytes(2.5, u64::MAX))
            .resource(extra_key(), extra())
            .archive([4, 1, 2, 3], vec![5, 6, 7, 8])
            .game_version("1283531|^/game38/branches/release_1.0|2026")
            .mod_dir(&folder())
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
            print(`build {string.split(enshwick.resources.game_version(), "|")[1]}`)
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
        assert!(logged(&loader, "build 1283531"), "{:?}", loader.logs());
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
    fn a_script_reads_another_resource_as_served_but_not_its_own_inside_its_build() {
        let loader = run(r#"
            local r = enshwick.resources
            local key = r.resource_keys("demo::balance::Table")[1]
            local extra = r.resource_keys("demo::Extra")[1]
            local early = pcall(r.served_value, key)
            print(`at init {early}`)
            r.serve(extra, extra, function()
                return { bonus = 4 }
            end)
            r.serve(key, key, function()
                local own = pcall(r.served_value, key)
                local bonus = r.served_value(extra).bonus
                print(`own {own} extra bonus {bonus}`)
                return { damage = bonus }
            end)
        "#);
        let built = loader.build_resource(&key(), &bytes(2.5, 7)).unwrap();
        assert_eq!(built, bytes(4.0, 7), "{:?}", loader.logs());
        for line in ["at init false", "own false extra bonus 4"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
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

    #[test]
    fn a_script_declares_an_id_and_serves_a_new_resource_from_a_value_alone() {
        let loader = run(r#"
            local guid = enshwick.resources.declare("you.luau:big_table")
            enshwick.resources.serve_new({ guid = guid, type = "demo::balance::Table" }, { damage = 4, side = "Right" })
            print(`declared {guid}`)
        "#);
        let line = loader
            .logs()
            .into_iter()
            .find_map(|(_, line)| line.split("declared ").nth(1).map(str::to_owned))
            .unwrap_or_else(|| panic!("{:?}", loader.logs()));
        let guid = Guid::try_parse(line.trim()).unwrap();
        let new = guid.key_by_hash(fnv1a32(b"demo::balance::Table"));
        let built = loader.build_resource(&new, &bytes(2.5, 9)).unwrap();
        let mut expected = bytes(4.0, 0);
        expected[4] = 1;
        assert_eq!(built, expected, "the value alone, its count left zero");
    }

    #[test]
    fn an_absent_optional_reads_as_none_keeps_its_place_and_clears_on_return() {
        let loader = run(r#"
            local r = enshwick.resources
            local value = r.game_value(r.resource_keys("demo::Extra")[1])
            print(`bonus {value.bonus} pair {#value.pair} first {value.pair[1] == r.none()} second {value.pair[2]}`)
            local empty = r.decode("demo::Extra", r.encode("demo::Extra", { pair = { r.none(), r.none() } }))
            print(`empty bonus {empty.bonus == nil}`)
            for _, key in r.resource_keys("demo::Extra") do
                r.serve(key, key, function()
                    return { bonus = r.none() }
                end)
            end
        "#);
        for line in ["bonus 1.5 pair 2 first true second 2.5", "empty bonus true"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
        let mut cleared = extra();
        cleared[..4].fill(0);
        assert_eq!(
            loader.build_resource(&extra_key(), &extra()).unwrap(),
            cleared
        );
    }

    #[test]
    fn a_patch_whose_value_does_not_fit_is_logged_and_serves_the_base() {
        let loader = run(r#"
            local r = enshwick.resources
            local key = r.resource_keys("demo::balance::Table")[1]
            r.serve(key, key, function()
                return { damage = "much" }
            end)
            r.serve(r.resource_keys("demo::Extra")[1], r.resource_keys("demo::Extra")[1], function()
                return { bonus = print }
            end)
        "#);
        let base = bytes(2.5, 7);
        assert_eq!(loader.build_resource(&key(), &base).unwrap(), base);
        assert_eq!(
            loader.build_resource(&extra_key(), &extra()).unwrap(),
            extra()
        );
        assert!(
            logged(&loader, "serve demo::balance::Table: damage"),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "serve demo::Extra: bonus"),
            "{:?}",
            loader.logs()
        );
    }

    /// The mod's folder: `data/names.txt` holds `ada`.
    fn folder() -> String {
        // Under the target directory, which `cargo clean` empties, not the system's.
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("enshwick-luau-mods-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("data").join("names.txt"), "ada").unwrap();
        dir.to_str().unwrap().to_owned()
    }

    #[test]
    fn a_script_reads_its_own_folder_knows_its_side_and_the_running_mods() {
        let loader = run(r#"
            local m = enshwick.mods
            print(`names {buffer.tostring(m.read("data/names.txt"))} listed {m.list("data")[1]}`)
            print(`side {m.side()} hosting {m.hosting()} other {m.is_running("you.other")}`)
            print(`escape {pcall(m.read, "../secret.txt")}`)
        "#);
        for line in [
            "names ada listed names.txt",
            "side server hosting true other false",
            "escape false",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_client_that_hosts_a_game_runs_the_server_and_one_that_joins_does_not() {
        for (hosting, line) in [
            (true, "side client hosting true"),
            (false, "side client hosting false"),
        ] {
            let loader = run_on(
                FakeHost::new().side(Side::CLIENT).hosting(hosting),
                "print(`side {enshwick.mods.side()} hosting {enshwick.mods.hosting()}`)",
            );
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_writes_pixels_as_png_and_reads_them_back() {
        let loader = run(r#"
            local c = enshwick.content
            local pixels = buffer.create(8)
            buffer.writeu32(pixels, 0, 0xff0000ff)
            buffer.writeu32(pixels, 4, 0xff00ff00)
            local image = c.decode_png(c.encode_png(pixels, 2, 1))
            print(`png {image.width}x{image.height} second {buffer.readu32(image.pixels, 4) == 0xff00ff00}`)
            print(`texture {buffer.len(c.decode(37, 1, 1, buffer.fromstring("abcd")))}`)
            print(`bad {pcall(c.encode_png, pixels, 3, 1)}`)
            local tinted = c.tint(pixels, { colour = { 0, 0, 255 }, amount = 100 })
            print(`tint {buffer.readu8(tinted, 2)}`)
        "#);
        let mut pixels = [0xff, 0, 0, 0xff, 0, 0xff, 0, 0xff];
        enshwick::content::icons::tint(
            &mut pixels,
            enshwick::content::icons::Tint {
                colour: [0, 0, 255],
                amount: 100,
            },
        );
        let tinted = format!("tint {}", pixels[2]);
        for line in [
            "png 2x1 second true",
            "texture 4",
            "bad false",
            tinted.as_str(),
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_list_with_a_hole_is_refused_rather_than_cut_short() {
        let loader = run(r#"
            local list = { 1, 2, 3 }
            list[2] = nil
            local ok, why = pcall(enshwick.resources.encode, "demo::balance::Table", { damage = list })
            print(`hole {ok} {string.find(tostring(why), "hole") ~= nil}`)
        "#);
        assert!(logged(&loader, "hole false true"), "{:?}", loader.logs());
    }
}
