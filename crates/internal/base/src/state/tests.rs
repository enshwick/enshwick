use super::*;

#[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
struct Count {
    n: u32,
}

/// The data folder of the mod `you.safes` in the world `key` under `dir`.
fn world(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}-mod-data")).join("you.safes")
}

#[test]
fn holds_are_read_from_the_folders_in_one_folder() {
    let data = std::env::temp_dir().join(format!("enshwick-world-hold-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&data);
    Hold::new(&data.join("world0-mod-data/you.lanterns"))
        .set("20 lit lanterns")
        .unwrap();
    Hold::new(&data.join("mods/stacks"))
        .set("raised stacks")
        .unwrap();
    assert_eq!(
        holds(&data.join("world0-mod-data")),
        vec![("you.lanterns".to_owned(), "20 lit lanterns".to_owned())]
    );
    assert!(holds(&data.join("world1-mod-data")).is_empty());
    assert_eq!(
        holds(&data.join("mods")),
        vec![("stacks".to_owned(), "raised stacks".to_owned())]
    );
    let _gone = std::fs::remove_dir_all(&data);
}

#[test]
fn a_text_hold_keeps_its_line_for_readers_without_the_mod_s_texts() {
    let data = std::env::temp_dir().join(format!("enshwick-text-hold-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&data);
    let hold = Hold::new(&data.join("you.altars"));
    hold.set_text("17 altars\nand more", "hold.altars", &[("altars", "17")])
        .unwrap();
    assert_eq!(hold.reason().as_deref(), Some("17 altars"));
    assert_eq!(hold.text().as_deref(), Some("hold.altars\taltars=17"));
    assert_eq!(
        holds(&data),
        vec![("you.altars".to_owned(), "17 altars".to_owned())]
    );
    hold.set("17 altars").unwrap();
    assert_eq!(hold.text(), None);
    let _gone = std::fs::remove_dir_all(&data);
}

#[test]
fn a_hold_file_of_one_plain_line_still_holds() {
    let data = std::env::temp_dir().join(format!("enshwick-old-hold-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&data);
    let dir = data.join("you.stacks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(HOLD_FILE), "raised stacks\n").unwrap();
    let hold = Hold::new(&dir);
    assert_eq!(hold.reason().as_deref(), Some("raised stacks"));
    assert_eq!(hold.text(), None);
    assert_eq!(
        holds(&data),
        vec![("you.stacks".to_owned(), "raised stacks".to_owned())]
    );
    let _gone = std::fs::remove_dir_all(&data);
}

#[test]
fn a_world_store_writes_its_world_s_file_and_switches_worlds() {
    let dir = std::env::temp_dir().join(format!("enshwick-world-store-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&dir);
    let store: WorldStore<Count> = WorldStore::new("count").unwrap();
    assert_eq!(store.update(|c| c.n += 1), None);
    store.open("world0", &world(&dir, "world0")).unwrap();
    store.update(|c| c.n = 5);
    store.flush().unwrap();
    store.open("world1", &world(&dir, "world1")).unwrap();
    assert_eq!(store.with(|c| c.n), Some(0));
    store.open("world0", &world(&dir, "world0")).unwrap();
    assert_eq!(store.with(|c| c.n), Some(5));
    assert_eq!(store.key().as_deref(), Some("world0"));
    let file = dir.join("world0-mod-data/you.safes/count.toml");
    assert!(file.exists());
    store.update(|c| c.n = 6);
    store.flush_in_background();
    store.close().unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains('6'), "{text}");
    store.open("world0", &world(&dir, "world0")).unwrap();
    store.close().unwrap();
    assert_eq!(store.key(), None);
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_world_store_name_is_a_plain_file_name() {
    assert!(WorldStore::<Count>::new("a/b").is_err());
    assert!(WorldStore::<Count>::new("").is_err());
    assert!(WorldStore::<Count>::new("settings").is_ok());
}

#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
struct Grown {
    n: u32,
    limit: u32,
    lock: Lock,
    names: Vec<String>,
}

#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
struct Lock {
    on: bool,
    tries: u8,
}

impl Default for Grown {
    fn default() -> Self {
        Self {
            n: 0,
            limit: 3,
            lock: Lock { on: true, tries: 5 },
            names: vec!["a".to_owned()],
        }
    }
}

#[test]
fn a_store_whose_type_gained_fields_reads_them_from_the_default() {
    let dir = std::env::temp_dir().join(format!("enshwick-store-grown-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("grown.toml"),
        "n = 7\nnames = []\n[lock]\non = false\n",
    )
    .unwrap();
    let store: Store<Grown> = Store::new(&dir, "grown");
    assert_eq!(
        store.load().unwrap(),
        Grown {
            n: 7,
            limit: 3,
            lock: Lock {
                on: false,
                tries: 5
            },
            names: Vec::new(),
        }
    );
    std::fs::write(dir.join("grown.toml"), "n = \"seven\"\n").unwrap();
    assert!(store.load().is_err(), "a wrong type still fails");
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_flush_lands_after_a_background_write() {
    let dir = std::env::temp_dir().join(format!("enshwick-world-flush-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&dir);
    let store: WorldStore<Count> = WorldStore::new("count").unwrap();
    store.open("world0", &world(&dir, "world0")).unwrap();
    let file = world(&dir, "world0").join("count.toml");
    for n in 0..20 {
        store.update(|c| c.n = n);
        store.flush_in_background();
        store.update(|c| c.n = n + 100);
        store.flush().unwrap();
        let saved: Count = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(saved.n, n + 100);
    }
    store.close().unwrap();
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_background_write_alone_writes_the_changed_value() {
    let dir = std::env::temp_dir().join(format!("enshwick-world-alone-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&dir);
    let store: WorldStore<Count> = WorldStore::new("count").unwrap();
    store.open("world0", &world(&dir, "world0")).unwrap();
    store.update(|c| c.n = 42);
    store.flush_in_background();
    drop(store.wait_for_writes());
    let file = world(&dir, "world0").join("count.toml");
    let saved: Count = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(saved.n, 42);
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_background_write_is_retried_at_the_next_save() {
    let dir = std::env::temp_dir().join(format!("enshwick-world-retry-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&dir);
    let store: WorldStore<Count> = WorldStore::new("count").unwrap();
    store.open("world0", &world(&dir, "world0")).unwrap();
    store.update(|c| c.n = 7);
    // A folder in the file's place: the rename fails.
    let file = world(&dir, "world0").join("count.toml");
    std::fs::create_dir_all(&file).unwrap();
    store.flush_in_background();
    drop(store.wait_for_writes());
    std::fs::remove_dir(&file).unwrap();
    store.flush().unwrap();
    let saved: Count = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(
        saved.n, 7,
        "the next save writes what the failed one did not"
    );
    let _gone = std::fs::remove_dir_all(&dir);
}
