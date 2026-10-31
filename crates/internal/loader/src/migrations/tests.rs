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

#[expect(
    clippy::unnecessary_wraps,
    reason = "a source of other mods' settings reads a file"
)]
fn none(_: &str) -> std::io::Result<Option<String>> {
    Ok(None)
}

fn ran_on(world: &Path) -> Vec<String> {
    WorldRecord::read(world).unwrap().unwrap().ran
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
    assert_eq!(ran_on(&world), ["2", "3"]);
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
    assert_eq!(WorldRecord::read(&world).unwrap(), None);
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
    assert_eq!(ran_on(&world), ["2"]);
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
    let source = |id: &str| Ok((id == "comfort").then(|| "multiplier = 2.5\n".to_owned()));
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
        "comfort" => Ok(Some("multiplier = 2.5\n".to_owned())),
        _ => Ok(Some("[recipes]\ncost = 3\n".to_owned())),
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
    assert_eq!(ran_on(&dir), ["2"]);
    let other = Target::World {
        key: "w-2",
        dir: &data.join("rates/worlds/w-2"),
    };
    assert_eq!(
        run(&mut record, other, "rates", &all, &double, false),
        Ok(None)
    );
    assert_eq!(ran_on(&data.join("rates/worlds/w-2")), ["2"]);
    assert!(!data.join("rates/worlds/w-2/settings.toml").exists());
    let _gone = std::fs::remove_dir_all(&data);
}

#[test]
fn an_entry_set_to_nothing_leaves_the_record() {
    let mut record = Record::default();
    assert!(!record.set_entry(Some("w1"), "a", Some(Vec::new())));
    assert!(record.set_entry(Some("w1"), "a", Some(vec!["2".to_owned()])));
    assert!(!record.set_entry(Some("w1"), "a", Some(vec!["2".to_owned()])));
    assert_eq!(record.entry(Some("w1"), "a"), Some(&vec!["2".to_owned()]));
    assert!(record.set_entry(Some("w1"), "a", None));
    assert!(record.worlds.is_empty(), "{record:?}");
    assert!(record.set_entry(None, "a", Some(vec!["1".to_owned()])));
    assert!(record.set_entry(None, "a", Some(Vec::new())));
    assert_eq!(record, Record::default());
}

#[test]
fn a_moved_world_takes_its_shared_entries_and_a_copy_shares_them() {
    let mut record = Record::default();
    record.ran(Some("w0-aaaaaaaa"), "a").push("2".to_owned());
    record.world_arrived("w0-aaaaaaaa", "w0-cccccccc", true);
    assert_eq!(record.worlds["w0-aaaaaaaa"]["a"], ["2"]);
    assert_eq!(record.worlds["w0-cccccccc"]["a"], ["2"]);
    record.world_arrived("w0-aaaaaaaa", "w0-bbbbbbbb", false);
    assert!(!record.worlds.contains_key("w0-aaaaaaaa"));
    assert_eq!(record.worlds["w0-bbbbbbbb"]["a"], ["2"]);
    assert!(!record.world_arrived("gone", "w0-dddddddd", false));
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

#[test]
fn a_removed_setting_goes_from_an_inline_table_too() {
    let m = migration(
        "2",
        vec![
            step(MigrationStep::REMOVE_SETTING, "old.legacy", ""),
            step(MigrationStep::REMOVE_SETTING, "old.inner.gone", ""),
        ],
    );
    let text = "old = { radius = 3, legacy = true, inner = { gone = 1, kept = 2 } }\n";
    let new = apply_settings(text, 0, &m, &double, &none).unwrap();
    let table: toml::Table = new.parse().unwrap();
    let at = |key| enshwick::config::value_at(&table, key).map(ToString::to_string);
    assert_eq!(at("old.legacy"), None, "{new}");
    assert_eq!(at("old.inner.gone"), None, "{new}");
    assert_eq!(at("old.radius").as_deref(), Some("3"), "{new}");
    assert_eq!(at("old.inner.kept").as_deref(), Some("2"), "{new}");
}

/// The error's Display and Debug, checked to hold nothing of `secret`.
fn shown_without(secret: &str, error: &MigrationError) -> String {
    let text = format!("{error} {error:?}");
    assert!(!text.contains(secret), "{text}");
    text
}

#[test]
fn a_broken_settings_line_is_named_without_its_value() {
    let remove = migration("2", vec![step(MigrationStep::REMOVE_SETTING, "old", "")]);
    for broken in [
        "limit = 4\ntoken = \"hunter2\n",
        "limit = 4\ntoken = hunter2\n",
    ] {
        let error = apply_settings(broken, 0, &remove, &double, &none).unwrap_err();
        let text = shown_without("hunter2", &error);
        assert!(
            text.contains("line 2") && text.contains("key token"),
            "{text}"
        );
    }
    for inside_a_string in [
        "token = \"\"\"\nmy secret hunter2 = x \\q\n\"\"\"\n",
        "token = '''\nhunter2 = abc\u{1}\n'''\n",
    ] {
        let error = apply_settings(inside_a_string, 0, &remove, &double, &none).unwrap_err();
        let text = shown_without("hunter2", &error);
        assert!(
            text.contains("line 2") && text.contains("key: None"),
            "{text}"
        );
    }

    let take = migration(
        "2",
        vec![step(MigrationStep::TAKE_SETTING, "vault:token", "token")],
    );
    let source = |_: &str| Ok(Some("# Vault.\ntoken = \"hunter2\n".to_owned()));
    let error = apply_settings("limit = 4\n", 0, &take, &double, &source).unwrap_err();
    let text = shown_without("hunter2", &error);
    assert!(text.contains("vault") && text.contains("line 2"), "{text}");

    let map = migration("2", vec![step(MigrationStep::MAP_SETTING, "pin", "")]);
    for (mapped, secret) in [
        ("\"hunter2", "hunter2"),
        ("aHVudGVyMg==", "aHVudGVyMg"),
        ("hunter2 = x", "hunter2"),
    ] {
        let leaky = |_: usize, _: usize, _: &str| Ok(mapped.to_owned());
        let error = apply_settings("pin = \"x\"\n", 0, &map, &leaky, &none).unwrap_err();
        let text = shown_without(secret, &error);
        assert!(
            text.contains("setting pin") && text.contains("key: None"),
            "{text}"
        );
    }
}

/// A world folder of `you.lockers` with a settings file and one store, and
/// two migrations: one on the settings, one renaming and changing the store.
fn lockers_world(name: &str) -> (PathBuf, PathBuf, [Migration; 2]) {
    let dir = scratch(name);
    let world = dir.join("you.lockers/worlds/w1");
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
    (dir, world, all)
}

fn fives(_: usize, step: usize, input: &str) -> Result<String, MapError> {
    if step == 1 {
        Ok(input.replace('1', "5"))
    } else {
        double(0, 0, input)
    }
}

#[test]
fn a_world_keeps_its_own_record_so_a_lost_shared_record_runs_nothing_twice() {
    let (dir, world, all) = lockers_world("own-record");
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    let mut record = Record::default();
    assert!(run(&mut record, target, "you.lockers", &all, &fives, false).is_ok());
    assert_eq!(ran_on(&world), ["2", "3"]);
    assert!(record.worlds.is_empty(), "{record:?}");

    let mut lost = Record::default();
    assert_eq!(
        run(&mut lost, target, "you.lockers", &all, &fives, false),
        Ok(None)
    );
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 4\n"
    );

    let copy = dir.join("other-install/you.lockers/worlds/w1");
    std::fs::create_dir_all(&copy).unwrap();
    for name in [WORLD_RECORD, "settings.toml", "keepers.toml"] {
        std::fs::copy(world.join(name), copy.join(name)).unwrap();
    }
    let copied = Target::World {
        key: "w1",
        dir: &copy,
    };
    assert_eq!(
        run(
            &mut Record::default(),
            copied,
            "you.lockers",
            &all,
            &fives,
            false
        ),
        Ok(None)
    );
    assert_eq!(
        std::fs::read_to_string(copy.join("keepers.toml")).unwrap(),
        "count = 5\n"
    );
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_shared_record_counts_for_a_world_until_it_has_its_own() {
    let (dir, world, all) = lockers_world("legacy");
    let mut record = Record::default();
    record.ran(Some("w1"), "you.lockers").push("2".to_owned());
    record.ran(Some("w1"), "other").push("9".to_owned());
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    let line = run(&mut record, target, "you.lockers", &all, &fives, false).unwrap();
    assert_eq!(
        line.as_deref(),
        Some("you.lockers 3: rename store owners to keepers, change store keepers")
    );
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 2\n",
        "migration 2 ran before, by the shared record"
    );
    assert_eq!(ran_on(&world), ["2", "3"]);
    assert!(!record.worlds["w1"].contains_key("you.lockers"));
    assert_eq!(record.worlds["w1"]["other"], ["9"]);
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_world_record_that_does_not_read_runs_nothing() {
    let (dir, world, all) = lockers_world("bad-record");
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    for broken in ["", "ran = ["] {
        std::fs::write(world.join(WORLD_RECORD), broken).unwrap();
        let (_, reason) = run(
            &mut Record::default(),
            target,
            "you.lockers",
            &all,
            &fives,
            false,
        )
        .unwrap_err();
        assert!(reason.contains(WORLD_RECORD), "{reason}");
        assert_eq!(
            std::fs::read_to_string(world.join("settings.toml")).unwrap(),
            "limit = 2\n"
        );
    }
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_store_that_does_not_read_fails_its_migration_instead_of_counting_as_absent() {
    let (dir, world, all) = lockers_world("unreadable-store");
    std::fs::write(world.join("owners.toml"), [0xff, 0xfe, b'\n']).unwrap();
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    let (_, reason) = run(
        &mut Record::default(),
        target,
        "you.lockers",
        &all,
        &fives,
        false,
    )
    .unwrap_err();
    assert!(reason.starts_with("migration 3: owners.toml"), "{reason}");
    assert_eq!(ran_on(&world), ["2"]);
    assert!(!world.join("keepers.toml").exists());
    assert_eq!(
        std::fs::read(world.join("owners.toml")).unwrap(),
        [0xff, 0xfe, b'\n']
    );
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn another_mod_s_settings_that_do_not_read_fail_the_take() {
    let m = migration(
        "2",
        vec![step(MigrationStep::TAKE_SETTING, "comfort:multiplier", "c")],
    );
    let held = |_: &str| Err(std::io::Error::other("held by another program"));
    let error = apply_settings("c = 1.0\n", 0, &m, &double, &held).unwrap_err();
    assert!(
        error.to_string().contains("held by another program"),
        "{error}"
    );
}

#[test]
fn a_migration_keeps_the_files_it_changed_as_they_were() {
    let (dir, world, all) = lockers_world("backup");
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    assert!(
        run(
            &mut Record::default(),
            target,
            "you.lockers",
            &all,
            &fives,
            false
        )
        .is_ok()
    );
    let backups = world.join(BACKUPS);
    assert_eq!(
        std::fs::read_to_string(backups.join("2/settings.toml")).unwrap(),
        "limit = 2\n"
    );
    assert_eq!(
        std::fs::read_to_string(backups.join("3/owners.toml")).unwrap(),
        "count = 1\n"
    );
    assert_eq!(
        WorldRecord::read(&backups.join("3")).unwrap().unwrap().ran,
        ["2"],
        "the record as it was before migration 3"
    );
    assert!(!backups.join("2").join(".under-way.toml").exists());
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_commit_that_fails_part_way_puts_every_file_back() {
    let (dir, world, _) = lockers_world("part-way");
    let mut staged = Staged::default();
    staged.put(world.join("settings.toml"), Some("limit = 4\n".to_owned()));
    staged.put(world.join("keepers.toml"), Some("count = 5\n".to_owned()));
    staged.put(world.join("owners.toml"), None);
    staged.record(
        &world,
        WorldRecord {
            ran: vec!["3".to_owned()],
        },
    );
    let keepers = world.join("keepers.toml");
    let write = |path: &Path, text: &str| {
        if path == keepers {
            Err(std::io::Error::other("disk full"))
        } else {
            enshwick::files::write_atomically(path, text)
        }
    };
    let backup = world.join(BACKUPS).join("3");
    let error = staged
        .commit_with(Some((&backup, "3")), &write)
        .unwrap_err();
    assert!(error.to_string().contains("disk full"), "{error}");
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 2\n"
    );
    assert_eq!(
        std::fs::read_to_string(world.join("owners.toml")).unwrap(),
        "count = 1\n"
    );
    assert!(!keepers.exists());
    assert_eq!(WorldRecord::read(&world).unwrap(), None);
    assert!(!backup.join(".under-way.toml").exists());
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_commit_a_crash_cut_short_is_put_back_before_its_migration_runs_again() {
    let (dir, world, all) = lockers_world("crash");
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    let mut staged = Staged::default();
    staged.put(world.join("settings.toml"), Some("limit = 4\n".to_owned()));
    staged.record(
        &world,
        WorldRecord {
            ran: vec!["2".to_owned()],
        },
    );
    let crash = |path: &Path, text: &str| {
        assert!(!path.ends_with(WORLD_RECORD), "the power goes out");
        enshwick::files::write_atomically(path, text)
    };
    let backup = world.join(BACKUPS).join("2");
    let cut_short = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        staged.commit_with(Some((&backup, "2")), &crash)
    }));
    assert!(cut_short.is_err());
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 4\n",
        "half a migration on disk, as a crash leaves it"
    );

    assert!(
        run(
            &mut Record::default(),
            target,
            "you.lockers",
            &all,
            &fives,
            false
        )
        .is_ok()
    );
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 4\n",
        "doubled once, not twice"
    );
    assert_eq!(ran_on(&world), ["2", "3"]);
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_finished_commit_whose_note_a_crash_left_is_not_put_back() {
    let (dir, world, all) = lockers_world("note-left");
    let target = Target::World {
        key: "w1",
        dir: &world,
    };
    assert!(
        run(
            &mut Record::default(),
            target,
            "you.lockers",
            &all[..1],
            &fives,
            false
        )
        .is_ok()
    );
    let backup = world.join(BACKUPS).join("2");
    std::fs::write(
        backup.join(".under-way.toml"),
        "migration = \"2\"\ncopied = [\"settings.toml\"]\n",
    )
    .unwrap();
    assert!(
        run(
            &mut Record::default(),
            target,
            "you.lockers",
            &all,
            &fives,
            false
        )
        .is_ok()
    );
    assert_eq!(
        std::fs::read_to_string(world.join("settings.toml")).unwrap(),
        "limit = 4\n"
    );
    assert!(!backup.join(".under-way.toml").exists());
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_backup_folder_is_named_safely_for_any_migration_id() {
    let dir = Path::new("w");
    assert_eq!(backup_of(dir, "2"), dir.join(BACKUPS).join("2"));
    assert_eq!(backup_of(dir, "../x"), dir.join(BACKUPS).join("_.._x"));
    assert_eq!(backup_of(dir, "a b/c"), dir.join(BACKUPS).join("a_b_c"));
    assert_eq!(backup_of(dir, ""), dir.join(BACKUPS).join("_"));
}

proptest::proptest! {
    #[test]
    fn no_secret_reaches_a_migration_error(secret in "zq[0-9]{6}", shape in 0usize..6) {
        let broken = match shape {
            0 => format!("token = \"{secret}\n"),
            1 => format!("token = {secret}\n"),
            2 => format!("token = \"\"\"\n{secret} = 1 \\q\n\"\"\"\n"),
            3 => format!("token = '''\n{secret} = 1\u{1}\n'''\n"),
            4 => format!("a = [\n{secret} = 1\n"),
            _ => format!("a = {{\n{secret} = \"x\\q\" }}\n"),
        };
        let remove = migration("2", vec![step(MigrationStep::REMOVE_SETTING, "old", "")]);
        let migrated = apply_settings(&broken, 0, &remove, &double, &none);
        proptest::prop_assert!(migrated.is_err());
        if let Err(error) = migrated {
            shown_without(&secret, &error);
        }
        let map = migration("2", vec![step(MigrationStep::MAP_SETTING, "pin", "")]);
        let leaky = |_: usize, _: usize, _: &str| Ok(broken.clone());
        if let Err(error) = apply_settings("pin = 1\n", 0, &map, &leaky, &none) {
            shown_without(&secret, &error);
        }
    }
}
