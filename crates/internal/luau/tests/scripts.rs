//! Luau mods through the fake host: the API they reach, the sandbox, the
//! budgets, and faults that turn one mod off.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use enshwick::abi::{Header, PlayerData, PlayerInfo, Status, TickData};
    use enshwick::events::{PlayerJoined, Ticked, WorldSaved};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Runtime, Scripts};

    static CASE: Mutex<Option<(Scripts, Limits)>> = Mutex::new(None);
    static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.luau", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let (scripts, limits) = CASE.lock().unwrap().take().unwrap();
            let runtime = enshwick_luau::start(load, "you.luau", scripts, limits)?;
            *RUNTIME.lock().unwrap() = Some(runtime);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Luau);

    fn player(entity: u32, key: u64, name: &str) -> PlayerInfo {
        let mut bytes = [0u8; 64];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity,
            key,
            permissions: 0,
            name_len: u8::try_from(name.len()).unwrap(),
            name: bytes,
        }
    }

    fn run_with(main: &str, limits: Limits, scripts: Scripts) -> (Installed, Status) {
        *CASE.lock().unwrap() = Some((scripts.with("main.luau", main), limits));
        let loader = FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .player(player(10, 7, "Ann"))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        (loader, status)
    }

    fn run(main: &str) -> Installed {
        let (loader, status) = run_with(main, Limits::default(), Scripts::new());
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn logged(loader: &Installed, text: &str) -> bool {
        loader.logs().iter().any(|(_, line)| line.contains(text))
    }

    fn runtime() -> Runtime {
        RUNTIME.lock().unwrap().clone().unwrap()
    }

    fn joined(entity: u32, key: u64) -> PlayerData {
        PlayerData {
            header: Header::of::<PlayerData>(),
            slot: 0,
            entity,
            key,
        }
    }

    fn tick(loader: &Installed) {
        let _handled = loader.raise::<Ticked>(&TickData {
            header: Header::of::<TickData>(),
            step: 1,
        });
    }

    #[test]
    fn a_handler_gets_the_event_s_data_as_a_table() {
        let loader = run(r#"enshwick.on("PlayerJoined", function(player)
                enshwick.log.info(`joined {player.entity} as {player.key}`)
            end)"#);
        assert_eq!(loader.raise::<PlayerJoined>(&joined(9, u64::MAX)), 1);
        assert!(
            logged(&loader, &format!("joined 9 as {}", u64::MAX)),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_reaches_the_world_and_its_players() {
        let loader = run(r#"local players = enshwick.world.players()
            enshwick.world.broadcast(`{#players} online, first {players[1].name}`)
            enshwick.world.tell(players[1], "hello")"#);
        let chat = loader.chat();
        assert!(
            chat.iter()
                .any(|(to, line)| *to == 0 && line == "1 online, first Ann"),
            "{chat:?}"
        );
        assert!(
            chat.iter().any(|(to, line)| *to == 7 && line == "hello"),
            "{chat:?}"
        );
    }

    #[test]
    fn a_task_runs_on_ticks_until_it_answers_false() {
        let loader = run(r"local runs = 0
            enshwick.scheduler.every(0, function()
                runs += 1
                enshwick.log.info(`run {runs}`)
                return runs < 2
            end)");
        for _ in 0..4 {
            tick(&loader);
        }
        assert!(logged(&loader, "run 2"));
        assert!(!logged(&loader, "run 3"));
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn an_error_in_a_handler_turns_the_mod_off_with_its_line() {
        let loader = run(r#"enshwick.on("WorldSaved", function()
                error("boom")
            end)"#);
        let _handled = loader.raise::<WorldSaved>(&());
        let faults = loader.faults();
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert!(
            faults[0].contains("main.luau:2") && faults[0].contains("boom"),
            "{faults:?}"
        );
        assert!(runtime().is_off());
        let _again = loader.raise::<WorldSaved>(&());
        assert_eq!(loader.faults().len(), 1);
    }

    #[test]
    fn an_endless_handler_stops_at_its_budget() {
        let limits = Limits::default().with_call(Duration::from_millis(20));
        let (loader, status) = run_with(
            r#"enshwick.on("WorldSaved", function() while true do end end)"#,
            limits,
            Scripts::new(),
        );
        assert_eq!(status, Status::OK);
        let started = Instant::now();
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(
            loader.faults()[0].contains("time budget"),
            "{:?}",
            loader.faults()
        );
        assert_eq!(runtime().usage().overruns, 1);
    }

    #[test]
    fn a_pcall_does_not_outlast_the_budget() {
        let limits = Limits::default().with_call(Duration::from_millis(20));
        let (loader, _) = run_with(
            r#"enshwick.on("WorldSaved", function()
                while true do pcall(function() while true do end end) end
            end)"#,
            limits,
            Scripts::new(),
        );
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(
            loader.faults()[0].contains("time budget"),
            "{:?}",
            loader.faults()
        );
    }

    #[test]
    fn an_endless_start_refuses_the_mod() {
        *CASE.lock().unwrap() = None;
        let mut limits = Limits::default();
        limits.start = Duration::from_millis(20);
        let (loader, status) = run_with("while true do end", limits, Scripts::new());
        assert_ne!(status, Status::OK);
        assert!(logged(&loader, "time budget"), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_past_its_memory_stops() {
        let limits = Limits::default().with_memory(4 << 20);
        let (loader, _) = run_with(
            r#"enshwick.on("WorldSaved", function()
                local t = {}
                for i = 1, 10000000 do t[i] = i end
            end)"#,
            limits,
            Scripts::new(),
        );
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(
            loader.faults()[0].contains("memory"),
            "{:?}",
            loader.faults()
        );
    }

    #[test]
    fn a_syntax_error_refuses_the_mod_with_its_line() {
        let (loader, status) = run_with("local x = ", Limits::default(), Scripts::new());
        assert_ne!(status, Status::OK);
        assert!(logged(&loader, "main.luau:1"), "{:?}", loader.logs());
    }

    #[test]
    fn the_sandbox_leaves_out_files_processes_and_the_debugger() {
        let loader = run(
            r#"enshwick.log.info(`io {io == nil} execute {os.execute == nil} debug {debug == nil} getfenv {pcall(getfenv) == false or getfenv == nil}`)
            local ok = pcall(function() enshwick.world = nil end)
            enshwick.log.info(`api frozen {not ok}`)
            local outside = pcall(require, "../outside")
            enshwick.log.info(`outside {not outside}`)"#,
        );
        assert!(
            logged(&loader, "io true execute true debug true"),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "api frozen true"), "{:?}", loader.logs());
        assert!(logged(&loader, "outside true"), "{:?}", loader.logs());
    }

    /// Bytes Luau would take for compiled bytecode: a version byte, a types
    /// version byte, then anything.
    const BYTECODE: &str = "\u{6}\u{3}\u{0}\u{0}\u{1}\u{0}";

    #[test]
    fn a_main_script_of_bytecode_is_refused() {
        let (loader, status) = run_with(BYTECODE, Limits::default(), Scripts::new());
        assert_ne!(status, Status::OK, "{:?}", loader.logs());
    }

    #[test]
    fn a_required_script_of_bytecode_is_refused() {
        let (loader, status) = run_with(
            r#"local ok = pcall(require, "./compiled")
            enshwick.log.info(`compiled loaded {ok}`)"#,
            Limits::default(),
            Scripts::new().with("compiled.luau", BYTECODE),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(
            logged(&loader, "compiled loaded false"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_requires_its_siblings() {
        let (loader, status) = run_with(
            r#"local greet = require("./lib/greet")
            enshwick.log.info(greet("Ann"))"#,
            Limits::default(),
            Scripts::new()
                .with("lib/greet.luau", r#"local suffix = require("./suffix") return function(n) return "hi " .. n .. suffix end"#)
                .with("lib/suffix.luau", r#"return "!""#),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(&loader, "hi Ann!"), "{:?}", loader.logs());
    }

    #[test]
    fn an_unknown_event_refuses_the_mod() {
        let (loader, status) = run_with(
            r#"enshwick.on("PlayerJumped", function() end)"#,
            Limits::default(),
            Scripts::new(),
        );
        assert_ne!(status, Status::OK);
        assert!(
            logged(&loader, "no event is named \"PlayerJumped\""),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_subscription_after_start_raises_in_the_script() {
        let loader = run(r#"enshwick.on("WorldSaved", function()
                local ok, why = pcall(enshwick.on, "WorldSaved", function() end)
                enshwick.log.info(`late {ok} {why}`)
            end)"#);
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(logged(&loader, "late false"), "{:?}", loader.logs());
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_wrong_argument_names_the_function_and_parameter() {
        let loader = run(r"local ok, why = pcall(enshwick.world.broadcast, 5)
            enshwick.log.info(`{ok} {why}`)");
        assert!(
            logged(
                &loader,
                "enshwick.world.broadcast: text: expected a string, got a number"
            ),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn print_writes_to_the_mod_s_log() {
        let loader = run(r#"print("one", 2)"#);
        assert!(logged(&loader, "one\t2"), "{:?}", loader.logs());
    }
}
