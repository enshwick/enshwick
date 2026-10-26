use std::path::PathBuf;

use enshwick::abi::MigrationStep;

use super::*;

#[test]
fn renames_follow_their_chain_and_a_cycle_ends() {
    let direct: rustc_hash::FxHashMap<[u8; 16], [u8; 16]> = [
        ([1; 16], [2; 16]),
        ([2; 16], [3; 16]),
        ([7; 16], [8; 16]),
        ([8; 16], [7; 16]),
    ]
    .into_iter()
    .collect();
    let followed = follow_renames(&direct);
    assert_eq!(followed[&[1; 16]], [3; 16]);
    assert_eq!(followed[&[2; 16]], [3; 16]);
    assert_eq!(followed[&[7; 16]], [8; 16]);
}

fn step(kind: u32, from: &str, to: &str) -> Step {
    Step {
        kind,
        from: from.to_owned(),
        to: to.to_owned(),
        from_guid: [0; 16],
        to_guid: [0; 16],
    }
}

fn migration(id: &str, steps: Vec<Step>) -> Migration {
    Migration {
        id: id.to_owned(),
        steps,
    }
}

fn double(_: usize, _: usize, input: &str) -> Result<String, MapError> {
    let n: i64 = input
        .parse()
        .map_err(|_| MapError::Said(format!("not a number: {input}")))?;
    Ok((n * 2).to_string())
}

fn none(_: &str) -> Option<String> {
    None
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("enshwick-migrations-{name}-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn settings_steps_keep_comments() {
    let text = "# Cost.\ncost = 3 # each\n# Limit.\nlimit = 4\nold = true\n";
    let m = migration(
        "2",
        vec![
            step(MigrationStep::RENAME_SETTING, "cost", "cost_multiplier"),
            step(MigrationStep::MAP_SETTING, "limit", ""),
            step(MigrationStep::REMOVE_SETTING, "old", ""),
            step(MigrationStep::MAP_SETTING, "missing", ""),
        ],
    );
    let new = apply_settings(text, 0, &m, &double, &none).unwrap();
    assert_eq!(
        new,
        "# Cost.\ncost_multiplier = 3 # each\n# Limit.\nlimit = 8\n"
    );
}

#[test]
fn a_rename_never_overwrites() {
    let m = migration("2", vec![step(MigrationStep::RENAME_SETTING, "a", "b")]);
    let new = apply_settings("a = 1\nb = 2\n", 0, &m, &double, &none).unwrap();
    assert_eq!(new, "a = 1\nb = 2\n");
}

#[test]
fn a_refused_map_names_the_setting() {
    let m = migration("2", vec![step(MigrationStep::MAP_SETTING, "limit", "")]);
    let error = apply_settings("limit = \"x\"\n", 0, &m, &double, &none).unwrap_err();
    assert!(
        error.to_string().starts_with("setting limit: not a number"),
        "{error}"
    );
}

#[test]
fn the_plan_says_what_will_happen() {
    let all = [
        migration("2", vec![step(MigrationStep::RENAME_SETTING, "a", "b")]),
        migration("3", vec![step(MigrationStep::RENAME_STORE, "x", "y")]),
    ];
    let pending = pending(&all, &["2".to_owned()]);
    assert_eq!(
        plan("you.trading-post", &pending, false),
        "you.trading-post 3: rename store x to y"
    );
    assert_eq!(
        plan("you.trading-post", &pending, true),
        "you.trading-post 3: nothing here"
    );
}

#[test]
fn a_world_runs_its_pending_migrations_once() {
    let dir = scratch("world");
    let world = dir.join("you.trading-post/worlds/w1");
    std::fs::create_dir_all(&world).unwrap();
    std::fs::write(world.join("settings.toml"), "limit = 2\n").unwrap();
    std::fs::write(world.join("owners.toml"), "count = 1\n").unwrap();
    let all = [
        migration("2", vec![step(MigrationStep::MAP_SETTING, "limit", "")]),
        migration(
            "3",
            vec![
                step(MigrationStep::RENAME_STORE, "owners", "keepers"),
                step(MigrationStep::MAP_STORE, "keepers", ""),
            ],
        ),
    ];
    let map = |_: usize, step: usize, input: &str| {
        if step == 1 {
            Ok(input.replace('1', "5"))
        } else {
            double(0, 0, input)
        }
    };
    let mut record = Record::default();
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    let line = run(&mut record, target, "you.trading-post", &all, &map, false).unwrap();
    assert_eq!(
        line.as_deref(),
        Some(
            "you.trading-post 2: change setting limit; you.trading-post 3: rename store owners to keepers, change store keepers"
        )
    );
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 4\n"
    );
    assert_eq!(
        std::fs::read_to_string(world.join("keepers.toml")).unwrap(),
        "count = 5\n"
    );
    assert_eq!(record.worlds["w1"]["you.trading-post"], ["2", "3"]);
    assert_eq!(
        run(&mut record, target, "you.trading-post", &all, &map, false).unwrap(),
        None
    );
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 4\n"
    );
    record.write(&dir).unwrap();
    assert_eq!(Record::read(&dir).unwrap(), record);
    std::fs::write(dir.join(RECORD), "").unwrap();
    assert!(matches!(Record::read(&dir), Err(RecordError::Empty)));
    std::fs::write(dir.join(RECORD), "settings = [").unwrap();
    assert!(matches!(Record::read(&dir), Err(RecordError::Malformed(_))));
    std::fs::remove_file(dir.join(RECORD)).unwrap();
    assert_eq!(Record::read(&dir).unwrap(), Record::default());
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_migration_that_fails_midway_changes_nothing() {
    let dir = scratch("midway");
    let world = dir.join("you.trading-post/worlds/w1");
    std::fs::create_dir_all(&world).unwrap();
    std::fs::write(
        world.join("settings.toml"),
        "limit = 2
",
    )
    .unwrap();
    std::fs::write(
        world.join("owners.toml"),
        "count = 1
",
    )
    .unwrap();
    let all = [migration(
        "2",
        vec![
            step(MigrationStep::MAP_SETTING, "limit", ""),
            step(MigrationStep::RENAME_STORE, "owners", "keepers"),
            step(MigrationStep::MAP_STORE, "keepers", ""),
        ],
    )];
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    let refuse = |_: usize, step: usize, input: &str| {
        if step == 2 {
            Err(MapError::Said("refused".to_owned()))
        } else {
            double(0, 0, input)
        }
    };
    let mut record = Record::default();
    let (_, reason) = run(
        &mut record,
        target,
        "you.trading-post",
        &all,
        &refuse,
        false,
    )
    .unwrap_err();
    assert!(reason.contains("refused"), "{reason}");
    assert!(record.worlds["w1"]["you.trading-post"].is_empty());
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 2
"
    );
    assert!(world.join("owners.toml").exists() && !world.join("keepers.toml").exists());
    // Run again once the map takes it: whole, once.
    let take = |_: usize, step: usize, input: &str| {
        if step == 2 {
            Ok(input.replace('1', "5"))
        } else {
            double(0, 0, input)
        }
    };
    assert!(run(&mut record, target, "you.trading-post", &all, &take, false).is_ok());
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 4
"
    );
    assert_eq!(
        std::fs::read_to_string(world.join("keepers.toml")).unwrap(),
        "count = 5
"
    );
    assert!(!world.join("owners.toml").exists());
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_world_the_mod_never_saw_starts_current() {
    let dir = scratch("fresh");
    let world = dir.join("you.trading-post/worlds/new");
    let all = [migration(
        "2",
        vec![step(MigrationStep::MAP_SETTING, "limit", "")],
    )];
    let mut record = Record::default();
    let target = Target::World {
        key: "new",
        dir: &world,
    };
    assert_eq!(
        run(
            &mut record,
            target,
            "you.trading-post",
            &all,
            &double,
            false
        ),
        Ok(None)
    );
    assert_eq!(record.worlds["new"]["you.trading-post"], ["2"]);
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dry_run_changes_and_records_nothing() {
    let dir = scratch("dry");
    let file = dir.join("you.trading-post.toml");
    std::fs::write(&file, "limit = 2\n").unwrap();
    let all = [migration(
        "2",
        vec![step(MigrationStep::MAP_SETTING, "limit", "")],
    )];
    let mut record = Record::default();
    let line = run(
        &mut record,
        Target::Settings {
            path: &file,
            born: false,
        },
        "you.trading-post",
        &all,
        &double,
        true,
    )
    .unwrap();
    assert_eq!(
        line.as_deref(),
        Some("you.trading-post 2: change setting limit")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "limit = 2\n");
    assert!(record.settings["you.trading-post"].is_empty());
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failure_records_the_migrations_before_it() {
    let dir = scratch("fail");
    let file = dir.join("you.trading-post.toml");
    std::fs::write(&file, "a = 1\nlimit = \"x\"\n").unwrap();
    let all = [
        migration("2", vec![step(MigrationStep::RENAME_SETTING, "a", "b")]),
        migration("3", vec![step(MigrationStep::MAP_SETTING, "limit", "")]),
    ];
    let mut record = Record::default();
    let (_, reason) = run(
        &mut record,
        Target::Settings {
            path: &file,
            born: false,
        },
        "you.trading-post",
        &all,
        &double,
        false,
    )
    .unwrap_err();
    assert!(reason.starts_with("migration 3: setting limit"), "{reason}");
    assert_eq!(record.settings["you.trading-post"], ["2"]);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "b = 1\nlimit = \"x\"\n"
    );
    let _gone = std::fs::remove_dir_all(&dir);
}
#[test]
fn a_taken_setting_comes_from_the_other_mod_s_file_with_its_value() {
    let m = migration(
        "2",
        vec![
            step(
                MigrationStep::TAKE_SETTING,
                "comfort:multiplier",
                "comfort_multiplier",
            ),
            step(MigrationStep::TAKE_SETTING, "comfort:missing", "nothing"),
            step(MigrationStep::TAKE_SETTING, "gone:limit", "limit"),
        ],
    );
    let source = |id: &str| (id == "comfort").then(|| "multiplier = 2.5\n".to_owned());
    let text = "# Comfort per piece.\ncomfort_multiplier = 1.0\nlimit = 3\n";
    let new = apply_settings(text, 0, &m, &double, &source).unwrap();
    assert_eq!(
        new,
        "# Comfort per piece.\ncomfort_multiplier = 2.5\nlimit = 3\n"
    );
    let nested = migration(
        "3",
        vec![
            step(
                MigrationStep::TAKE_SETTING,
                "comfort:multiplier",
                "comfort.multiplier",
            ),
            step(MigrationStep::TAKE_SETTING, "rates:recipes.cost", "cost"),
        ],
    );
    let source = |id: &str| match id {
        "comfort" => Some("multiplier = 2.5\n".to_owned()),
        _ => Some("[recipes]\ncost = 3\n".to_owned()),
    };
    let new = apply_settings("a = 1\n", 0, &nested, &double, &source).unwrap();
    assert_eq!(new, "a = 1\ncost = 3\n\n[comfort]\nmultiplier = 2.5\n");
    let bad = migration("4", vec![step(MigrationStep::TAKE_SETTING, "no-key", "a")]);
    assert!(apply_settings("", 0, &bad, &double, &source).is_err());
}

#[test]
fn a_settings_file_waits_until_written_then_starts_current_with_taken_values() {
    let dir = scratch("born");
    let file = dir.join("rates.toml");
    std::fs::write(dir.join("comfort.toml"), "multiplier = 2.5\n").unwrap();
    let all = [
        migration("1", vec![step(MigrationStep::MAP_SETTING, "comfort", "")]),
        migration(
            "2",
            vec![step(
                MigrationStep::TAKE_SETTING,
                "comfort:multiplier",
                "comfort",
            )],
        ),
    ];
    let mut record = Record::default();
    let target = |born| Target::Settings { path: &file, born };
    assert_eq!(
        run(&mut record, target(false), "rates", &all, &double, false),
        Ok(None)
    );
    assert!(
        record.settings.is_empty(),
        "nothing recorded before the file"
    );

    std::fs::write(&file, "comfort = 1.0\n").unwrap();
    let line = run(&mut record, target(true), "rates", &all, &double, false).unwrap();
    assert_eq!(
        line.as_deref(),
        Some("rates 2: take setting comfort from comfort:multiplier")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "comfort = 2.5\n");
    assert_eq!(record.settings["rates"], ["1", "2"]);
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_world_takes_the_other_mod_s_world_values() {
    let data = scratch("take-world");
    let old = data.join("comfort/worlds/w-1");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("settings.toml"), "multiplier = 4.0\n").unwrap();
    let dir = data.join("rates/worlds/w-1");
    let all = [migration(
        "2",
        vec![step(
            MigrationStep::TAKE_SETTING,
            "comfort:multiplier",
            "comfort",
        )],
    )];
    let mut record = Record::default();
    let target = Target::World {
        key: "w-1",
        dir: &dir,
    };
    let outcome = run(&mut record, target, "rates", &all, &double, false);
    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(
        std::fs::read_to_string(dir.join("settings.toml")).unwrap(),
        "comfort = 4.0\n"
    );
    assert_eq!(record.worlds["w-1"]["rates"], ["2"]);
    let other = Target::World {
        key: "w-2",
        dir: &data.join("rates/worlds/w-2"),
    };
    assert_eq!(
        run(&mut record, other, "rates", &all, &double, false),
        Ok(None)
    );
    assert!(!data.join("rates/worlds/w-2").exists());
    let _gone = std::fs::remove_dir_all(&data);
}

#[test]
fn taking_over_a_former_id_counts_its_migrations() {
    let mut record = Record::default();
    record.ran(None, "old").push("2".into());
    record
        .ran(Some("w-1"), "old")
        .extend(["2".into(), "3".into()]);
    record.ran(Some("w-1"), "new").push("3".into());
    record.take_over("new", "old");
    record.take_over("new", "old");
    assert_eq!(record.settings["new"], ["2"]);
    assert_eq!(record.worlds["w-1"]["new"], ["3", "2"]);
    assert_eq!(record.carried["new"], ["old"]);
}

#[test]
fn a_setting_renamed_into_or_out_of_a_table_moves_there() {
    let m = migration(
        "2",
        vec![
            step(
                MigrationStep::RENAME_SETTING,
                "per_level",
                "count.per_level",
            ),
            step(MigrationStep::RENAME_SETTING, "old.radius", "radius"),
            step(MigrationStep::REMOVE_SETTING, "old.legacy", ""),
        ],
    );
    let text = "per_level = [2, 4]\n\n[old]\nradius = 3\nlegacy = true\n";
    let new = apply_settings(text, 0, &m, &double, &none).unwrap();
    let table: toml::Table = new.parse().unwrap();
    assert_eq!(
        enshwick::config::value_at(&table, "count.per_level")
            .map(ToString::to_string)
            .as_deref(),
        Some("[2, 4]")
    );
    assert_eq!(
        enshwick::config::value_at(&table, "radius")
            .map(ToString::to_string)
            .as_deref(),
        Some("3")
    );
    assert!(enshwick::config::value_at(&table, "per_level").is_none());
    assert!(enshwick::config::value_at(&table, "old.legacy").is_none());
    assert!(!new.contains("\"count.per_level\""), "{new}");
}

#[test]
fn a_migration_never_leaves_a_settings_file_that_does_not_read_back() {
    let deep = vec!["level"; 300].join(".");
    let rename = migration(
        "deep",
        vec![step(MigrationStep::RENAME_SETTING, "radius", &deep)],
    );
    let migrated = apply_settings("radius = 8\n", 0, &rename, &double, &none);
    assert!(
        migrated.is_err_and(|e| matches!(e, MigrationError::Unreadable(_))),
        "a file deeper than the reader takes is refused"
    );
}
