use enshwick_core::config::{Field, Schema};
use proptest::prelude::*;

use super::*;

const TEMPLATE: &str = "# Lockers.\n\n# How many.\nlimit = 4\n\n# The mode.\nmode = \"auto\"\n\n# Comfort.\n[comfort]\n# The factor.\nmultiplier = 1.0\n";

fn sync_of(existing: &str) -> Synced {
    sync(TEMPLATE, existing, "settings", "you.lockers 1.2.0", &[]).unwrap()
}

#[test]
fn a_file_like_the_template_stays_as_it_is() {
    assert_eq!(sync_of(TEMPLATE), Synced::default());
}

#[test]
fn new_keys_come_with_their_comments_and_old_ones_go_with_a_line_each() {
    let existing = "# Lockers.\n\n# How many.\nlimit = 9 # mine\n\n# Old.\ncolour = \"red\"\n";
    let synced = sync_of(existing);
    let text = synced.text.unwrap();
    assert!(text.contains("# How many.\nlimit = 9 # mine\n"), "{text}");
    assert!(text.contains("# The mode.\nmode = \"auto\"\n"), "{text}");
    assert!(
        text.contains("# Comfort.\n[comfort]\n# The factor.\nmultiplier = 1.0\n"),
        "{text}"
    );
    assert!(!text.contains("colour"), "{text}");
    assert_eq!(
        synced.lines,
        [
            "settings.toml: removed colour, which you.lockers 1.2.0 does not have",
            "settings.toml: added mode = \"auto\"",
            "settings.toml: added comfort.multiplier = 1.0",
        ]
    );
}

#[test]
fn the_loader_s_comments_are_refreshed_and_a_person_s_own_lines_stay() {
    let existing = "# Lockers.\n\n# My note, apart.\n\n# How many, said long ago.\nlimit = 2\nmode = \"on\"\n\n[comfort]\nmultiplier = 3.0\n";
    let text = sync_of(existing).text.unwrap();
    assert!(
        text.contains("# My note, apart.\n\n# How many.\nlimit = 2\n"),
        "{text}"
    );
    assert!(text.contains("# The mode.\nmode = \"on\"\n"), "{text}");
    assert!(
        text.contains("# Comfort.\n[comfort]\n# The factor.\nmultiplier = 3.0\n"),
        "{text}"
    );
    assert!(!text.contains("said long ago"), "{text}");
}

#[test]
fn a_secret_s_default_never_reaches_a_line() {
    let template = "# The pin.\npin = 4815\n";
    let synced = sync(template, "", "vault", "you.vault 1.0.0", &["pin"]).unwrap();
    assert_eq!(synced.lines, ["vault.toml: added pin"]);
}

#[test]
fn a_file_that_is_no_toml_is_left_alone() {
    assert!(sync(TEMPLATE, "limit = [", "settings", "x", &[]).is_err());
}

#[test]
fn a_value_where_the_mod_now_has_a_table_takes_the_table() {
    let synced = sync_of("limit = 1\nmode = \"on\"\ncomfort = 2\n");
    let text = synced.text.unwrap();
    assert!(
        text.contains("[comfort]\n# The factor.\nmultiplier = 1.0"),
        "{text}"
    );
    assert!(synced.lines.iter().any(|l| l.contains("removed comfort,")));
}

#[test]
fn files_go_beside_the_save_with_one_backup_each() {
    let file = world_file("you.trading-post", "altars", "save/world1");
    assert!(file.ends_with("save/world1-mod-settings/you.trading-post/altars.toml"));
    assert!(backup_of(&file).ends_with("you.trading-post/altars.toml.bak"));
}

fn files() -> Vec<SettingsFile> {
    let file = |name: &str, fields| SettingsFile {
        file: name.to_owned(),
        schema: Schema::new(&format!("{name} doc"), "config", fields),
        template: String::new(),
    };
    vec![
        file(DEFAULT_FILE, vec![Field::new("limit", &4)]),
        file(
            "altars",
            vec![Field::new("per_level", &[2, 4]).with_secret(true)],
        ),
    ]
}

#[test]
fn a_mod_s_files_are_one_view_and_split_back() {
    let schema = view_schema(&files()).unwrap();
    let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, ["limit", "altars.per_level"]);
    assert_eq!(schema.groups[0].key, "altars");
    assert_eq!(
        schema.fields[1].text, "config.per_level",
        "the author's text keys"
    );
    assert_eq!(
        split_key("altars.per_level", &files()),
        ("altars", "per_level")
    );
    assert_eq!(split_key("limit", &files()), (DEFAULT_FILE, "limit"));
    assert_eq!(
        split_key("comfort.x", &files()),
        (DEFAULT_FILE, "comfort.x")
    );
    let view = view_text(&[
        (DEFAULT_FILE, "limit = 9\n"),
        ("altars", "per_level = [1]\n"),
    ]);
    assert_eq!(view, "limit = 9\n\n[altars]\nper_level = [1]\n");
    assert_eq!(part(&view, DEFAULT_FILE, &["altars"]), "limit = 9\n");
    assert_eq!(part(&view, "altars", &["altars"]), "per_level = [1]\n");
    assert!(view_schema(&[]).is_none());
}

/// A settings text of `keys`, each `key = value` under a doc line.
fn text_of(keys: &[(String, i64)]) -> String {
    keys.iter()
        .map(|(key, value)| format!("# Doc of {key}.\n{key} = {value}\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn keys() -> impl Strategy<Value = Vec<(String, i64)>> {
    proptest::collection::btree_map("[a-h]{1,3}", any::<i64>(), 0..8)
        .prop_map(|keys| keys.into_iter().collect())
}

proptest! {
    #[test]
    fn a_synced_file_has_the_template_s_keys_the_file_s_values_and_syncs_to_itself(
        template in keys(),
        existing in keys(),
    ) {
        let (template_text, existing_text) = (text_of(&template), text_of(&existing));
        let synced = sync(&template_text, &existing_text, "settings", "x 1", &[]).unwrap();
        let text = synced.text.clone().unwrap_or_else(|| existing_text.clone());
        let read: toml::Table = text.parse().unwrap();
        let mut found: Vec<&String> = read.keys().collect();
        found.sort();
        let wanted: Vec<&String> = template.iter().map(|(k, _)| k).collect();
        prop_assert_eq!(found, wanted);
        for (key, value) in &existing {
            if template.iter().any(|(k, _)| k == key) {
                prop_assert_eq!(read[key].as_integer(), Some(*value));
            }
        }
        let added = template.iter().filter(|(k, _)| !existing.iter().any(|(e, _)| e == k)).count();
        let removed = existing.iter().filter(|(k, _)| !template.iter().any(|(t, _)| t == k)).count();
        prop_assert_eq!(synced.lines.len(), added + removed);
        let again = sync(&template_text, &text, "settings", "x 1", &[]).unwrap();
        prop_assert_eq!(again, Synced::default());
    }
}
