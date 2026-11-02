//! A Luau mod calls another mod's call through the description `cargo
//! enshwick deps --luau` writes, checked against the server's types.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::Entity;
    use enshwick::abi::Status;
    use enshwick::events::WorldSaved;
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Runtime, Scripts};

    /// The server's call, the one the fixture module describes.
    #[enshwick::call(name = "you.lockers:fee", output = u64, version = 1)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Fee {
        pub(crate) character: Entity,
        pub(crate) days: u32,
        pub(crate) place: u16,
        pub(crate) own: bool,
        pub(crate) row: u8,
    }

    const FEE: &str = include_str!("../../../cargo-enshwick/tests/fixtures/lockers_fee.luau");

    static CASE: Mutex<Option<Scripts>> = Mutex::new(None);
    static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scripts = CASE.lock().unwrap().take().unwrap();
            let runtime = enshwick_luau::start(load, "you.luau", scripts, None, Limits::default())?;
            *RUNTIME.lock().unwrap() = Some(runtime);
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

    fn run(main: &str, fee: &str) -> (Installed, Status) {
        let scripts = Scripts::new()
            .with("main.luau", main)
            .with("calls/fee.luau", fee);
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

    /// Starts `main` with a module edited by `edit` and asserts the start
    /// fails with `why` in the log.
    fn refused(edit: impl FnOnce(&str) -> String, why: &str) {
        let (loader, status) = run(CALLING, &edit(FEE));
        assert_ne!(status, Status::OK);
        assert!(logged(&loader, why), "{:?}", loader.logs());
    }

    const CALLING: &str = r#"local fee = enshwick.caller(require("./calls/fee"))
        local early, why = fee({ character = 1, days = 0, place = 0, own = false, row = 0 })
        print(`early {early} {why}`)
        enshwick.on("WorldSaved", function()
            local coins, err = fee({ character = 5, days = 4, place = 3, own = true, row = 2 })
            print(`fee {coins} {err}`)
        end)"#;

    #[test]
    fn a_script_calls_a_rust_server() {
        let (loader, status) = run(CALLING, FEE);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(&loader, "early nil unmatched"));
        loader.serve_call(|fee: &Fee| {
            let days = u64::from(fee.days) * 100;
            let place = u64::from(fee.row) * 10 + u64::from(fee.place);
            u64::from(fee.character.0) * 1000 + days + place + u64::from(fee.own)
        });
        saved(&loader);
        assert!(logged(&loader, "fee 5424 nil"), "{:?}", loader.logs());
    }

    #[test]
    fn an_unmatched_call_answers_nil_and_why() {
        let (loader, status) = run(CALLING, FEE);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        saved(&loader);
        assert!(logged(&loader, "fee nil unmatched"), "{:?}", loader.logs());
    }

    #[test]
    fn a_description_claiming_another_hash_is_refused() {
        refused(
            |fee| fee.replacen("read = \"u32\", hash = \"", "read = \"u32\", hash = \"1", 1),
            "days: its hash is not the one its read and offset give",
        );
    }

    #[test]
    fn a_value_type_in_the_input_is_refused() {
        refused(
            |fee| {
                fee.replacen(
                    "read = \"u32\", hash = \"5981015992674252258\"",
                    "read = \"u32\", hash = \"5981015992674252258\", shape = { name = \"Days\", kind = \"values\", repr = \"u32\", values = {} }",
                    1,
                )
            },
            "days: a script calls only with numbers, booleans, entities and GUIDs",
        );
    }

    #[test]
    fn an_enum_in_the_input_is_refused() {
        refused(
            |fee| {
                fee.replacen(
                    "read = \"u8\"",
                    "read = { repr = \"u8\", variants = { 1, 2 } }",
                    1,
                )
            },
            "row: a script calls only with numbers, booleans, entities and GUIDs",
        );
    }

    #[test]
    fn a_field_past_the_input_s_size_is_refused() {
        refused(
            |fee| fee.replacen("size = 12", "size = 11", 1),
            "row lies past the input's size",
        );
    }

    #[test]
    fn a_call_asked_for_after_the_start_raises_an_error() {
        let main = r#"enshwick.on("WorldSaved", function()
                local ok, err = pcall(enshwick.caller, require("./calls/fee"))
                print(`late {ok} {err}`)
            end)"#;
        let (loader, status) = run(main, FEE);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        saved(&loader);
        assert!(
            logged(&loader, "late false")
                && logged(
                    &loader,
                    "you.lockers:fee v1 is asked for from the next start"
                ),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_reload_cannot_change_an_asked_call() {
        let (loader, status) = run(CALLING, FEE);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let runtime = RUNTIME.lock().unwrap().clone().unwrap();
        let changed = FEE.replacen("read = \"u64\"", "read = \"i64\"", 1);
        let failed = runtime
            .reload(
                Scripts::new()
                    .with("main.luau", CALLING)
                    .with("calls/fee.luau", changed),
            )
            .unwrap_err();
        assert!(
            failed
                .to_string()
                .contains("you.lockers:fee v1 is described differently than at the start"),
            "{failed}"
        );
        assert!(
            runtime
                .reload(
                    Scripts::new()
                        .with("main.luau", CALLING)
                        .with("calls/fee.luau", FEE)
                )
                .is_ok()
        );
    }
}
