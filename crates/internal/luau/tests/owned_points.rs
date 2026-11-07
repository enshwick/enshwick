//! A Luau mod's own extension points: declared in `pack.toml`, defined by
//! `enshwick.points.define_point`, and linked by a Rust mod through the
//! layout hashes `#[point]` gives the same fields.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::events::WorldSaved;
    use enshwick::packs::PackManifest;
    use enshwick::points::{Decision, Verdict};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};

    const MANIFEST: &str = r#"
        id = "you.luau"
        version = "1.0.0"
        exports = ["point:price"]

        [[point]]
        name = "price"
        mode = "override"
        refusable = true
        output = "u32"
        input = [
            { name = "player", type = "enshwick.player_key" },
            { name = "item", type = "enshwick.item_id" },
            { name = "count", type = "u16" },
            { name = "cheap", type = "bool" },
            { name = "tier", type = "u8" },
        ]
    "#;

    /// The definition another mod's author writes for the pack's point, or
    /// `cargo enshwick deps` writes for them.
    #[enshwick::point(name = "you.luau:price", mode = Override, output = u32, refusable)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Price {
        pub(crate) player: enshwick::PlayerKey,
        pub(crate) item: enshwick::ItemId,
        pub(crate) count: u16,
        pub(crate) cheap: bool,
        pub(crate) tier: u8,
    }

    /// A point a Rust mod owns, which the script links.
    #[enshwick::point(name = "you.stalls:rent", mode = Override, output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Rent {
        pub(crate) stall: u32,
    }

    static MAIN: Mutex<Option<String>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let main = MAIN.lock().unwrap().take().unwrap();
            let manifest = PackManifest::parse(MANIFEST).unwrap();
            let scripts = Scripts::new().with("main.luau", main);
            let _runtime = enshwick_luau::start_pack(load, &manifest, scripts, Limits::default())?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Luau,
        r#"
        id = "you.luau"
        side = "server"
        "#
    );

    /// Asks the pack's point each time a world saves, and logs what the
    /// links made of the answer 10.
    const ASK: &str = r#"
        local price = enshwick.points.define_point("price")
        enshwick.on("WorldSaved", function()
            local value, how = price:decide({ player = "18446744073709551615", item = 3, count = 2, cheap = true, tier = 1 }, 10)
            print(`price {value} {how} extended {price:is_extended()}`)
        end)
    "#;

    fn run(main: &str) -> Installed {
        *MAIN.lock().unwrap() = Some(main.to_owned());
        let loader = FakeHost::new().install();
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
    fn a_rust_link_changes_the_answer_of_a_point_the_script_owns() {
        let loader = run(ASK);
        assert_eq!(loader.defined_points(), ["you.luau:price"]);
        let _saved = loader.raise::<WorldSaved>(&());
        assert!(
            logged(&loader, "price 10 kept extended false"),
            "{:?}",
            loader.logs()
        );
        let linked = loader.extend_point(|price: &Price, answer: &u32| {
            if price.cheap && price.tier == 1 && price.player.0 == u64::MAX {
                Verdict::Set(answer * 2 + u32::from(price.count))
            } else {
                Verdict::Pass
            }
        });
        assert!(linked, "the Rust definition's hashes match the pack's");
        let _saved = loader.raise::<WorldSaved>(&());
        assert!(
            logged(&loader, "price 22 changed extended true"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_rust_link_reads_the_script_s_input_and_answer_aligned() {
        let loader = run(ASK);
        assert!(loader.extend_point(|price: &Price, answer: &u32| {
            let input = core::ptr::from_ref(price).addr();
            let answer_at = core::ptr::from_ref(answer).addr();
            if input.is_multiple_of(16) && answer_at.is_multiple_of(16) {
                Verdict::Set(answer + 1)
            } else {
                Verdict::Pass
            }
        }));
        let _saved = loader.raise::<WorldSaved>(&());
        assert!(
            logged(&loader, "price 11 changed extended true"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_point_defined_twice_in_a_start_is_one_point() {
        let loader = run(&format!(
            "local again = enshwick.points.define_point(\"price\")\n{ASK}"
        ));
        assert_eq!(loader.defined_points(), ["you.luau:price"]);
        assert!(loader.extend_point(|_: &Price, answer: &u32| Verdict::Set(answer + 1)));
        let _saved = loader.raise::<WorldSaved>(&());
        assert!(
            logged(&loader, "price 11 changed extended true"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_rust_link_refuses_a_point_the_script_owns() {
        let loader = run(ASK);
        assert!(loader.extend_point(|_: &Price, _: &u32| Verdict::Refuse));
        let _saved = loader.raise::<WorldSaved>(&());
        assert!(logged(&loader, "price nil refused"), "{:?}", loader.logs());
    }

    #[test]
    fn a_rust_definition_of_other_fields_does_not_link() {
        #[enshwick::point(name = "you.luau:price", mode = Override, output = u32, refusable)]
        #[derive(Clone, Copy, Debug)]
        struct Wider {
            player: u64,
            item: enshwick::ItemId,
            count: u16,
            cheap: bool,
            tier: u8,
        }
        let loader = run(ASK);
        assert!(!loader.extend_point(|_: &Wider, _: &u32| Verdict::Pass));
    }

    #[test]
    fn the_script_links_a_rust_point_beside_its_own() {
        use enshwick::points::Point;
        let rent = format!(
            r#"local rent = table.freeze({{
                name = "you.stalls:rent", mode = "override", refusable = false,
                input = {{ size = 4, fields = {{ {{ name = "stall", offset = 0, read = "u32", hash = "{}" }} }} }},
                output = {{ size = 4, hash = "{}", read = "u32" }},
            }})
            enshwick.extend(rent, function(input, answer) return answer + input.stall end)"#,
            Rent::LAYOUT[0].hash,
            Rent::OUTPUT[0].hash
        );
        let loader = run(&format!("{ASK}\n{rent}"));
        assert_eq!(loader.defined_points(), ["you.luau:price"]);
        assert_eq!(loader.attached_points(), ["you.stalls:rent"]);
        assert_eq!(
            loader.ask_point(&Rent { stall: 5 }, 10),
            Decision::Changed(15)
        );
    }

    #[test]
    fn a_point_pack_toml_does_not_declare_or_defined_late_is_an_error() {
        let loader = run(
            r#"local ok, err = pcall(enshwick.points.define_point, "rent")
            print(`undeclared {ok} {err}`)
            enshwick.on("WorldSaved", function()
                local late, why = pcall(enshwick.points.define_point, "price")
                print(`late {late} {why}`)
            end)"#,
        );
        assert!(logged(&loader, "undeclared false"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "declares no [[point]] named \"rent\""),
            "{:?}",
            loader.logs()
        );
        let _saved = loader.raise::<WorldSaved>(&());
        assert!(
            logged(&loader, "is defined from the next start"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn an_input_that_does_not_fit_its_fields_is_an_error() {
        let loader = run(r#"local price = enshwick.points.define_point("price")
            enshwick.on("WorldSaved", function()
                local ok, err = pcall(price.decide, price, { player = "1", item = -1, count = 0, cheap = false, tier = 0 }, 10)
                print(`decided {ok} {err}`)
            end)"#);
        let _saved = loader.raise::<WorldSaved>(&());
        assert!(logged(&loader, "decided false"), "{:?}", loader.logs());
        assert!(logged(&loader, "the input"), "{:?}", loader.logs());
    }
}
