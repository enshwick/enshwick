//! Luau mods through the fake host: the API they reach, the sandbox, the
//! budgets, and faults that turn one mod off.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use enshwick::abi::{Header, PlayerData, PlayerInfo, Status, TickData};
    use enshwick::config::{Field, Schema, Stage};
    use enshwick::events::{PlayerJoined, SettingsChanged, Ticked, WorldSaved};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Runtime, Scripts};

    static CASE: Mutex<Option<(Scripts, Option<Schema>, Limits)>> = Mutex::new(None);
    static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.luau", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let (scripts, settings, limits) = CASE.lock().unwrap().take().unwrap();
            let runtime = enshwick_luau::start(load, "you.luau", scripts, settings, limits)?;
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
            entity: enshwick::Entity(entity),
            key: enshwick::PlayerKey::new(key),
            permissions: 0,
            name_len: u8::try_from(name.len()).unwrap(),
            name: bytes,
        }
    }

    fn run_with(main: &str, limits: Limits, scripts: Scripts) -> (Installed, Status) {
        run_full(main, limits, scripts, None, FakeHost::new())
    }

    fn run_full(
        main: &str,
        limits: Limits,
        scripts: Scripts,
        settings: Option<Schema>,
        host: FakeHost,
    ) -> (Installed, Status) {
        *CASE.lock().unwrap() = Some((scripts.with("main.luau", main), settings, limits));
        let loader = host
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
            entity: enshwick::Entity(entity),
            key: enshwick::PlayerKey::new(key),
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
                .any(|(to, line)| *to == enshwick::PlayerKey::new(0)
                    && line == "1 online, first Ann"),
            "{chat:?}"
        );
        assert!(
            chat.iter()
                .any(|(to, line)| *to == enshwick::PlayerKey::new(7) && line == "hello"),
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

    fn main(source: &str) -> Scripts {
        Scripts::new().with("main.luau", source)
    }

    fn count(loader: &Installed, text: &str) -> usize {
        loader
            .logs()
            .iter()
            .filter(|(_, line)| line.contains(text))
            .count()
    }

    #[test]
    fn a_reload_moves_the_handlers_to_the_new_scripts() {
        let loader = run(r#"enshwick.on("WorldSaved", function() print("first") end)"#);
        runtime()
            .reload(main(
                r#"enshwick.on("WorldSaved", function() print("second") end)"#,
            ))
            .unwrap();
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(logged(&loader, "second"), "{:?}", loader.logs());
        assert!(!logged(&loader, "first"));
    }

    #[test]
    fn a_failing_reload_keeps_the_old_scripts() {
        let loader = run(r#"enshwick.on("WorldSaved", function() print("first") end)"#);
        let error = runtime().reload(main("local x = ")).unwrap_err();
        assert!(error.to_string().contains("main.luau:1"), "{error}");
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(logged(&loader, "first"));
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_reload_stops_the_old_tasks_without_a_fault() {
        let loader =
            run(r#"enshwick.scheduler.every(0, function() print("old task") return true end)"#);
        tick(&loader);
        assert_eq!(count(&loader, "old task"), 1);
        runtime().reload(main("")).unwrap();
        tick(&loader);
        tick(&loader);
        assert_eq!(count(&loader, "old task"), 1);
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
        assert!(!runtime().is_off());
    }

    #[test]
    fn a_handler_of_a_new_event_waits_for_the_next_start() {
        let loader = run(r#"enshwick.on("WorldSaved", function() end)"#);
        runtime()
            .reload(main(r#"enshwick.on("PlayerJoined", function() end)"#))
            .unwrap();
        assert!(
            logged(&loader, "handlers of PlayerJoined run from the next start"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn the_status_shows_the_vm_s_cost() {
        let _loader = run("local t = {} for i = 1, 1000 do t[i] = i end");
        assert!(runtime().usage().memory > 0);
        assert_eq!(runtime().usage().calls, 1);
    }

    fn greeting_schema() -> Schema {
        Schema::new(
            "",
            "config",
            vec![Field::new("greeting", Stage::Runtime, "hi").with_doc("What to say.", "")],
        )
    }

    #[test]
    fn a_script_reads_its_settings_and_sees_them_change() {
        let (loader, status) = run_full(
            r#"enshwick.log.info(`greeting {enshwick.settings().greeting}`)
            enshwick.on("SettingsChanged", function()
                enshwick.log.info(`now {enshwick.settings().greeting}`)
            end)"#,
            Limits::default(),
            Scripts::new(),
            Some(greeting_schema()),
            FakeHost::new().settings("greeting = \"yo\"\n"),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(&loader, "greeting yo"), "{:?}", loader.logs());
        assert_eq!(loader.config_schema(), Some(greeting_schema()));
        loader.set_settings("greeting = \"hey\"\n");
        let _handled = loader.raise::<SettingsChanged>(&());
        assert!(logged(&loader, "now hey"), "{:?}", loader.logs());
    }

    #[test]
    fn a_mod_without_settings_reads_an_empty_table() {
        let loader = run("enshwick.log.info(`keys {next(enshwick.settings()) == nil}`)");
        assert!(logged(&loader, "keys true"), "{:?}", loader.logs());
    }

    const LEVEL_UP: u32 = 0x51;
    const DIED: u32 = 0x52;

    const XP: u32 = 0x53;
    const HIT: u32 = 0x54;
    const WIDE: u32 = 0x55;

    fn game_host() -> FakeHost {
        let hit_fields = [
            ("hit_event.root_source_id", 8),
            ("hit_event.target_id", 12),
            ("hit_event.source_id", 16),
            ("hit_event.root_target_id", 20),
            ("hit_event.weapon_pide_id", 24),
            ("hit_event.health_change", 28),
            ("hit_event.stamina_change", 32),
            ("hit_event.damage_types", 36),
            ("hit_event.attack_category", 38),
            ("hit_event.target_faction", 39),
            ("damage_type.magical_fire", 3),
            ("attack_category.melee", 0),
            ("xp_gained_event.player_entity_id", 8),
            ("xp_gained_event.amount", 12),
            ("xp_gained_event.source", 16),
            ("experience_source.combat", 4),
        ];
        let mut host = FakeHost::new();
        for (id, value) in hit_fields {
            host = host.constant(id, value);
        }
        host.type_hash("hit_event", HIT)
            .type_size("hit_event", 48)
            .type_hash("xp_gained_event", XP)
            .type_size("xp_gained_event", 24)
            .game_event_type("keen::ecs::WideEvent", WIDE, 32)
            .field_offset("keen::ecs::WideEvent", "account", 8)
            .field_offset("keen::ecs::WideEvent", "delta", 16)
            .type_hash("level_up_event", LEVEL_UP)
            .type_size("level_up_event", 16)
            .constant("level_up_event.player_entity_id", 8)
            .constant("level_up_event.level", 12)
            .game_event_type("keen::ecs::EntityDiedEvent", DIED, 16)
            .field_offset("keen::ecs::EntityDiedEvent", "deadEntityId", 8)
    }

    #[test]
    fn a_typed_game_event_reaches_its_handler_as_a_table() {
        let (loader, status) = run_full(
            r#"enshwick.on("LevelUpEvent", function(up)
                enshwick.log.info(`entity {up.player_entity_id} reached {up.level}`)
            end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            game_host(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let mut bytes = [0u8; 16];
        bytes[8..12].copy_from_slice(&7u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&5i32.to_le_bytes());
        assert_eq!(loader.raise_game_event(LEVEL_UP, &bytes), 1);
        assert!(logged(&loader, "entity 7 reached 5"), "{:?}", loader.logs());
    }

    #[test]
    fn a_game_event_by_type_reads_the_fields_the_script_names() {
        let (loader, status) = run_full(
            r#"local types = enshwick.game_event_types()
            local names = {}
            for _, t in types do table.insert(names, `{t.name}:{t.size}`) end
            table.sort(names)
            enshwick.log.info(table.concat(names, " "))
            enshwick.on_game_type("keen::ecs::EntityDiedEvent", { deadEntityId = "entity" }, function(event)
                enshwick.log.info(`died {event.deadEntityId}`)
            end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            game_host(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(
            logged(
                &loader,
                "keen::ecs::EntityDiedEvent:16 keen::ecs::WideEvent:32"
            ),
            "{:?}",
            loader.logs()
        );
        let mut bytes = [0u8; 16];
        bytes[8..12].copy_from_slice(&4096u32.to_le_bytes());
        assert_eq!(loader.raise_game_event(DIED, &bytes), 1);
        assert!(logged(&loader, "died 4096"), "{:?}", loader.logs());
    }

    #[test]
    fn a_field_of_an_unknown_kind_refuses_the_mod() {
        let (loader, status) = run_full(
            r#"enshwick.on_game_type("keen::ecs::EntityDiedEvent", { deadEntityId = "pointer" }, function() end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            game_host(),
        );
        assert_ne!(status, Status::OK);
        assert!(
            logged(&loader, "\"pointer\" is none of"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_field_the_type_lacks_refuses_the_mod() {
        let (loader, status) = run_full(
            r#"enshwick.on_game_type("keen::ecs::EntityDiedEvent", { nope = "u32" }, function() end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            game_host(),
        );
        assert_ne!(status, Status::OK);
        assert!(logged(&loader, "nope"), "{:?}", loader.logs());
    }

    #[test]
    fn a_mask_and_a_game_value_reach_luau_by_name() {
        let (loader, status) = run_full(
            r#"enshwick.on("HitEvent", function(hit)
                enshwick.log.info(`hit with {table.concat(hit.damage_types, ",")} by {table.concat(hit.attack_category, ",")}`)
            end)
            enshwick.on("XpGainedEvent", function(xp)
                enshwick.log.info(`xp from {xp.source}`)
            end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            game_host(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let mut hit = [0u8; 48];
        hit[12..16].copy_from_slice(&5u32.to_le_bytes());
        hit[36..38].copy_from_slice(&(1u16 << 3).to_le_bytes());
        hit[38] = 1;
        assert_eq!(loader.raise_game_event(HIT, &hit), 1);
        assert!(
            logged(&loader, "hit with MAGICAL_FIRE by MELEE"),
            "{:?}",
            loader.logs()
        );
        let mut xp = [0u8; 24];
        xp[16] = 4;
        assert_eq!(loader.raise_game_event(XP, &xp), 1);
        assert!(logged(&loader, "xp from Combat"), "{:?}", loader.logs());
        xp[16] = 9;
        let _handled = loader.raise_game_event(XP, &xp);
        assert!(logged(&loader, "xp from Other(9)"), "{:?}", loader.logs());
    }

    #[test]
    fn sixty_four_bit_fields_by_type_arrive_as_strings() {
        let (loader, status) = run_full(
            r#"enshwick.on_game_type("keen::ecs::WideEvent", { account = "u64", delta = "i64" }, function(e)
                enshwick.log.info(`account {e.account} delta {e.delta}`)
            end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            game_host(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let mut wide = [0u8; 32];
        wide[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        wide[16..24].copy_from_slice(&(-3i64).to_le_bytes());
        assert_eq!(loader.raise_game_event(WIDE, &wide), 1);
        assert!(
            logged(&loader, &format!("account {} delta -3", u64::MAX)),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_typed_game_event_this_build_lacks_a_field_of_refuses_the_mod_with_why() {
        let (loader, status) = run_full(
            r#"enshwick.on("LevelUpEvent", function() end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            FakeHost::new()
                .type_hash("level_up_event", LEVEL_UP)
                .type_size("level_up_event", 16)
                .constant("level_up_event.player_entity_id", 8),
        );
        assert_ne!(status, Status::OK);
        assert!(
            logged(&loader, "level_up_event.level"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_reload_moves_game_event_handlers_and_a_new_one_waits() {
        let (loader, status) = run_full(
            r#"enshwick.on("LevelUpEvent", function() print("first") end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            game_host(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        runtime()
            .reload(main(
                r#"enshwick.on("LevelUpEvent", function() print("second") end)
                enshwick.on("XpGainedEvent", function() end)"#,
            ))
            .unwrap();
        let _handled = loader.raise_game_event(LEVEL_UP, &[0u8; 16]);
        assert!(logged(&loader, "second"), "{:?}", loader.logs());
        assert!(!logged(&loader, "first"));
        assert!(
            logged(&loader, "handlers of XpGainedEvent run from the next start"),
            "{:?}",
            loader.logs()
        );
    }

    #[enshwick::point(name = "you.trading-post:price", mode = Override, output = u32, refusable)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Price {
        pub(crate) item: u32,
        pub(crate) count: u32,
        pub(crate) player: u64,
    }

    /// The description `cargo enshwick point --luau` writes for `Price`.
    fn price_module() -> String {
        use enshwick::points::Point;
        let hash = |i: usize| Price::LAYOUT[i].hash;
        format!(
            r#"return table.freeze({{
                name = "you.trading-post:price",
                mode = "override",
                refusable = true,
                input = {{ size = 16, fields = {{
                    {{ name = "item", offset = 0, read = "u32", hash = "{}" }},
                    {{ name = "count", offset = 4, read = "u32", hash = "{}" }},
                    {{ name = "player", offset = 8, read = "u64", hash = "{}" }},
                }} }},
                output = {{ size = 4, hash = "{}", read = "u32" }},
            }})"#,
            hash(0),
            hash(1),
            hash(2),
            Price::OUTPUT[0].hash
        )
    }

    #[test]
    fn a_script_extends_another_mod_s_point() {
        use enshwick::points::Decision;
        let (loader, status) = run_full(
            r#"local price = require("./points/price")
            enshwick.extend(price, function(input, answer)
                if input.item == 9 then return enshwick.refuse end
                if input.player == "18446744073709551615" then return answer * 2 end
                return nil
            end)"#,
            Limits::default(),
            Scripts::new().with("points/price.luau", price_module()),
            None,
            FakeHost::new(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.attached_points(), ["you.trading-post:price"]);
        let ask = |item, player| {
            loader.ask_point(
                &Price {
                    item,
                    count: 1,
                    player,
                },
                10,
            )
        };
        assert_eq!(ask(1, u64::MAX), Decision::Changed(20));
        assert_eq!(ask(9, 1), Decision::Refused);
        assert_eq!(ask(1, 1), Decision::Kept(10));
    }

    #[test]
    fn an_answer_of_the_wrong_type_turns_the_mod_off_and_keeps_the_answer() {
        use enshwick::points::Decision;
        let (loader, status) = run_full(
            r#"enshwick.extend(require("./points/price"), function() return "lots" end)"#,
            Limits::default(),
            Scripts::new().with("points/price.luau", price_module()),
            None,
            FakeHost::new(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            loader.ask_point(
                &Price {
                    item: 1,
                    count: 1,
                    player: 1
                },
                10
            ),
            Decision::Kept(10)
        );
        assert!(
            loader.faults()[0].contains("expected a number"),
            "{:?}",
            loader.faults()
        );
    }

    #[enshwick::event(name = "you.trading-post:sold")]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Sold {
        pub(crate) player: u64,
        pub(crate) item: u32,
        pub(crate) cheap: bool,
        pub(crate) tier: u8,
        pub(crate) count: u16,
    }

    /// A description a script writes for itself: no offsets and no hashes,
    /// which the runtime works out as Rust lays the struct out.
    const SOLD: &str = r#"{ name = "you.trading-post:sold", fields = {
        { name = "player", read = "u64" },
        { name = "item", read = "u32" },
        { name = "cheap", read = "bool" },
    } }"#;

    #[test]
    fn a_script_hears_another_mod_s_event_by_a_description_of_its_own() {
        let (loader, status) = run_full(
            &format!(
                r"enshwick.on_mod_event({SOLD}, function(sold)
                    enshwick.log.info(`sold {{sold.item}} to {{sold.player}} cheap {{sold.cheap}}`)
                end)"
            ),
            Limits::default(),
            Scripts::new(),
            None,
            FakeHost::new(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            loader.send_mod_event(&Sold {
                player: u64::MAX,
                item: 7,
                cheap: true,
                tier: 1,
                count: 2
            }),
            1
        );
        assert!(
            logged(&loader, &format!("sold 7 to {} cheap true", u64::MAX)),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_defines_and_sends_its_own_event() {
        let (loader, status) = run_full(
            r#"local greeted = enshwick.define_event({ name = "you.luau:greeted", fields = {
                { name = "player", read = "entity" },
                { name = "times", read = "u16" },
            } })
            enshwick.on("WorldSaved", function()
                greeted.send({ player = 7, times = 2 })
            end)"#,
            Limits::default(),
            Scripts::new(),
            None,
            FakeHost::new(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let _handled = loader.raise::<WorldSaved>(&());
        assert_eq!(loader.sent_events(), ["you.luau:greeted"]);
        assert_eq!(loader.sent_event_data()[0], [7, 0, 0, 0, 2, 0, 0, 0]);
    }

    #[test]
    fn a_reload_keeps_a_defined_event_and_a_new_one_waits() {
        let (loader, status) = run_full(
            r#"local greeted = enshwick.define_event({ name = "you.luau:greeted", fields = { { name = "n", read = "u32" } } })"#,
            Limits::default(),
            Scripts::new(),
            None,
            FakeHost::new(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        runtime()
            .reload(main(
                r#"local greeted = enshwick.define_event({ name = "you.luau:greeted", fields = { { name = "n", read = "u32" } } })
                greeted.send({ n = 4 })
                local ok = pcall(enshwick.define_event, { name = "you.luau:waved", fields = {} })
                print(`waved {ok}`)"#,
            ))
            .unwrap();
        assert_eq!(loader.sent_event_data(), [vec![4, 0, 0, 0]]);
        assert!(logged(&loader, "waved false"), "{:?}", loader.logs());
    }

    #[test]
    fn a_reload_cannot_change_a_defined_event_s_fields() {
        let (loader, status) = run_full(
            r#"local greeted = enshwick.define_event({ name = "you.luau:greeted", fields = { { name = "n", read = "u32" } } })"#,
            Limits::default(),
            Scripts::new(),
            None,
            FakeHost::new(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let failed = runtime().reload(main(
            r#"local greeted = enshwick.define_event({ name = "you.luau:greeted", fields = { { name = "n", read = "f32" } } })
            greeted.send({ n = 4 })"#,
        ));
        assert!(failed.is_err());
        assert!(loader.sent_event_data().is_empty());
    }

    #[test]
    fn a_definition_takes_no_hash_but_its_own() {
        let (loader, status) = run_full(
            r#"local ok, err = pcall(enshwick.define_event, { name = "you.luau:greeted", fields = {
                { name = "flag", read = "u8", offset = 0, hash = "12345" },
            } })
            print(`defined {ok} {err}`)
            local ok2, err2 = pcall(enshwick.on_mod_event, { name = "you.luau:mixed", fields = {
                { name = "a", read = "u8" },
                { name = "b", read = "u64", offset = 8, hash = "1" },
            } }, function() end)
            print(`mixed {ok2} {err2}`)"#,
            Limits::default(),
            Scripts::new(),
            None,
            FakeHost::new(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(&loader, "defined false"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "is not the one its read and offset give"),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "mixed false"), "{:?}", loader.logs());
    }

    #[test]
    fn one_event_described_twice_differently_is_an_error() {
        let (loader, status) = run_full(
            &format!(
                r#"enshwick.on_mod_event({SOLD}, function() end)
                local ok = pcall(enshwick.on_mod_event, {{ name = "you.trading-post:sold", fields = {{ {{ name = "item", read = "u64" }} }} }}, function() end)
                print(`twice {{ok}}`)"#
            ),
            Limits::default(),
            Scripts::new(),
            None,
            FakeHost::new(),
        );
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(logged(&loader, "twice false"), "{:?}", loader.logs());
    }
}
