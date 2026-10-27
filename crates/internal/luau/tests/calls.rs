//! A Luau mod calls another mod's call through the description `cargo
//! enshwick call --luau` writes, checked against the server's types.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::ecs::Entity;
    use enshwick::events::WorldSaved;
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Runtime, Scripts};

    /// What a player does at a stall, as the server declares it.
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, enshwick::Shape)]
    #[shape(values(BUY, SELL))]
    pub(crate) struct Deal(pub(crate) u32);

    impl Deal {
        pub(crate) const BUY: Self = Self(0);
        pub(crate) const SELL: Self = Self(1);
    }

    /// The server's call, the one the fixture module describes.
    #[enshwick::call(name = "you.stalls:rent", output = u64, version = 2)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Rent {
        pub(crate) character: Entity,
        pub(crate) deal: Deal,
        pub(crate) own: bool,
        pub(crate) row: u8,
        pub(crate) place: u16,
    }

    const RENT: &str = include_str!("../../../cargo-enshwick/tests/fixtures/stalls_rent.luau");

    static CASE: Mutex<Option<Scripts>> = Mutex::new(None);
    static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.luau", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scripts = CASE.lock().unwrap().take().unwrap();
            let runtime = enshwick_luau::start(load, "you.luau", scripts, None, Limits::default())?;
            *RUNTIME.lock().unwrap() = Some(runtime);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Luau);

    fn run(main: &str, rent: &str) -> (Installed, Status) {
        let scripts = Scripts::new()
            .with("main.luau", main)
            .with("calls/rent.luau", rent);
        *CASE.lock().unwrap() = Some(scripts);
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        (loader, status)
    }

    fn logged(loader: &Installed, text: &str) -> bool {
        loader.logs().iter().any(|(_, line)| line.contains(text))
    }

    fn saved(loader: &Installed) {
        let _handled = loader.raise::<WorldSaved>(&());
    }

    const CALLING: &str = r#"local rent = enshwick.caller(require("./calls/rent"))
        local early, why = rent({ character = 1, deal = 0, own = false, row = 0, place = 0 })
        print(`early {early} {why}`)
        enshwick.on("WorldSaved", function()
            local coins, err = rent({ character = 5, deal = 1, own = true, row = 2, place = 3 })
            print(`rent {coins} {err}`)
        end)"#;

    #[test]
    fn a_script_calls_a_rust_server_with_a_value_type_in_its_input() {
        let (loader, status) = run(CALLING, RENT);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(
            &loader,
            "early nil the call is asked for at the start"
        ));
        loader.serve_call(|rent: &Rent| {
            let base = u64::from(rent.character.0) * 100;
            let place = u64::from(rent.row) * 10 + u64::from(rent.place);
            if rent.deal == Deal::SELL && rent.own {
                base + place
            } else {
                base
            }
        });
        saved(&loader);
        assert!(logged(&loader, "rent 523 nil"), "{:?}", loader.logs());
    }

    #[test]
    fn an_unmatched_call_answers_nil_and_why() {
        let (loader, status) = run(CALLING, RENT);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        saved(&loader);
        assert!(logged(&loader, "rent nil unmatched"), "{:?}", loader.logs());
        assert_ne!(Deal::BUY, Deal::SELL);
    }

    #[test]
    fn a_description_claiming_another_hash_is_refused() {
        let tampered = RENT.replacen("read = \"u32\", hash = \"", "read = \"u32\", hash = \"1", 1);
        let (loader, status) = run(CALLING, &tampered);
        assert_ne!(status, Status::OK);
        assert!(
            logged(&loader, "its hash is not the one"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_value_type_with_other_values_is_refused() {
        let tampered = RENT.replacen(
            "name = \"SELL\", value = 1",
            "name = \"SELL\", value = 2",
            1,
        );
        let (loader, status) = run(CALLING, &tampered);
        assert_ne!(status, Status::OK);
        assert!(
            logged(&loader, "its hash is not the one"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_reload_cannot_change_an_asked_call() {
        let (loader, status) = run(CALLING, RENT);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let runtime = RUNTIME.lock().unwrap().clone().unwrap();
        let changed = RENT.replacen("major = 2", "major = 3", 1);
        let failed = runtime.reload(
            Scripts::new()
                .with("main.luau", CALLING)
                .with("calls/rent.luau", changed),
        );
        assert!(failed.is_err());
        assert!(
            runtime
                .reload(
                    Scripts::new()
                        .with("main.luau", CALLING)
                        .with("calls/rent.luau", RENT)
                )
                .is_ok()
        );
    }

    /// A stall kind, an enum the server's call takes.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, enshwick::Shape)]
    #[repr(u8)]
    pub(crate) enum Stall {
        Food = 1,
        Tools = 2,
    }

    /// A fee by stall kind.
    #[enshwick::call(name = "you.stalls:fee", output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Fee {
        pub(crate) stall: Stall,
        pub(crate) floor: u8,
        pub(crate) count: u16,
    }

    /// The module `cargo enshwick call --luau` writes for `Fee`.
    fn fee_module() -> String {
        use enshwick::services::Call;
        let hash = |i: usize| Fee::LAYOUT[i].hash;
        format!(
            r#"return {{
                name = "you.stalls:fee", major = 1,
                input = {{ size = 4, fields = {{
                    {{ name = "stall", offset = 0, read = {{ repr = "u8", variants = {{ 1, 2 }} }}, hash = "{}",
                       shape = {{ name = "Stall", kind = "enum", repr = "u8", values = {{ {{ name = "Food", value = 1 }}, {{ name = "Tools", value = 2 }} }} }} }},
                    {{ name = "floor", offset = 1, read = "u8", hash = "{}" }},
                    {{ name = "count", offset = 2, read = "u16", hash = "{}" }},
                }} }},
                output = {{ size = 4, hash = "{}", read = "u32" }},
            }}"#,
            hash(0),
            hash(1),
            hash(2),
            Fee::OUTPUT[0].hash
        )
    }

    #[test]
    fn a_script_calls_with_an_enum_and_writes_only_its_variants() {
        let main = r#"local fee = enshwick.caller(require("./calls/fee"))
            enshwick.on("WorldSaved", function()
                print(`fee {fee({ stall = 2, floor = 1, count = 3 })}`)
                local ok = pcall(fee, { stall = 3, floor = 1, count = 3 })
                print(`stray {ok}`)
            end)"#;
        let scripts = Scripts::new()
            .with("main.luau", main)
            .with("calls/fee.luau", fee_module());
        *CASE.lock().unwrap() = Some(scripts);
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader.serve_call(|fee: &Fee| {
            let base = if fee.stall == Stall::Food { 1 } else { 7 };
            base * u32::from(fee.count) + u32::from(fee.floor)
        });
        saved(&loader);
        assert!(logged(&loader, "fee 22"), "{:?}", loader.logs());
        assert!(logged(&loader, "stray false"), "{:?}", loader.logs());
    }
}
