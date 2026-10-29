use enshwick::config::Field;

use super::*;
use crate::configure::rows::{Widget, rows};

fn field(key: &str, stage: Stage, scope: Scope) -> Field {
    let mut field = Field::new(key, stage, &true);
    field.scope = scope;
    field
}

fn schema() -> Schema {
    Schema::new(
        "",
        "config",
        vec![
            field("lamps", Stage::Runtime, Scope::User),
            field("altars", Stage::Startup, Scope::World),
            field("radius", Stage::Sync, Scope::World),
            field("note", Stage::Client, Scope::User),
        ],
    )
}

fn keys(schema: &Schema) -> Vec<&str> {
    schema.fields.iter().map(|f| f.key.as_str()).collect()
}

fn row(stage: Stage, lock: Lock) -> Row {
    Row {
        key: "lamps".to_owned(),
        label: "Lamps".to_owned(),
        group: None,
        doc: String::new(),
        widget: Widget::Toggle(true),
        lock,
        stage,
    }
}

#[test]
fn the_screen_stays_while_its_menu_draws_every_frame() {
    assert!(!missed_frame(5, 5));
    assert!(!missed_frame(5, 6));
    assert!(missed_frame(5, 7));
    assert!(missed_frame(0, 2));
    assert!(!missed_frame(u64::MAX, u64::MAX));
}

#[test]
fn back_ends_the_typing_before_it_leaves_the_page() {
    assert_eq!(heard(0, false), Heard::Nothing);
    assert_eq!(heard(0, true), Heard::Nothing);
    assert_eq!(heard(BACK_PRESSED, false), Heard::Back);
    assert_eq!(heard(BACK_PRESSED, true), Heard::EndTyping);
    assert_eq!(heard(WORLD_SETTINGS_OPENED, false), Heard::WorldSettings);
    assert_eq!(heard(WORLD_SETTINGS_OPENED, true), Heard::WorldSettings);
}

#[test]
fn back_steps_from_each_page_towards_the_list_and_closes_from_it() {
    assert_eq!(back(Kind::List), Back::Close);
    assert_eq!(back(Kind::Settings), Back::List);
    assert_eq!(back(Kind::ServerList), Back::List);
    assert_eq!(back(Kind::ServerSettings), Back::ServerList);
    assert_eq!(back(Kind::Actions), Back::Inside);
}

#[test]
fn the_mods_screen_edits_only_each_players_own_settings() {
    let user = user_settings(schema()).unwrap();
    assert_eq!(keys(&user), ["lamps", "note"]);
    let world_only = Schema::new("", "config", vec![field("a", Stage::Runtime, Scope::World)]);
    assert!(user_settings(world_only).is_none());
}

#[test]
fn the_world_settings_show_only_join_values_when_joined_elsewhere() {
    assert_eq!(
        keys(&world_settings(schema(), false).unwrap()),
        ["altars", "radius"]
    );
    assert_eq!(keys(&world_settings(schema(), true).unwrap()), ["radius"]);
    let startup = Schema::new("", "config", vec![field("a", Stage::Startup, Scope::World)]);
    assert!(world_settings(startup, true).is_none());
}

#[test]
fn a_servers_rows_take_no_changes() {
    let mut shown = rows(&schema(), "", None, None);
    lock_to_server(&mut shown);
    assert!(shown.iter().all(|r| r.lock == Lock::Server));
}

#[test]
fn a_player_who_may_not_change_the_world_reads_open_rows_only() {
    assert_eq!(read_only(row(Stage::Runtime, Lock::Open)).lock, Lock::File);
    assert_eq!(
        read_only(row(Stage::Runtime, Lock::Server)).lock,
        Lock::Server
    );
    assert_eq!(read_only(row(Stage::Runtime, Lock::File)).lock, Lock::File);
}

#[test]
fn a_rows_label_names_when_it_applies_and_who_sets_it() {
    assert_eq!(row_label(&row(Stage::Runtime, Lock::Open)), "Lamps");
    let startup = texts::fill("row.startup", &[("label", "Lamps")]);
    assert_eq!(row_label(&row(Stage::Startup, Lock::Open)), startup);
    let restart = texts::fill("row.restart", &[("label", "Lamps")]);
    assert_eq!(row_label(&row(Stage::Restart, Lock::File)), restart);
    let both = texts::fill("row.server", &[("label", &startup)]);
    assert_eq!(row_label(&row(Stage::Startup, Lock::Server)), both);
}

#[test]
fn a_servers_join_values_read_as_a_table() {
    let table = server_table("{ radius = 4.0, mode = \"on\" }").unwrap();
    assert_eq!(table["radius"].as_float(), Some(4.0));
    assert!(server_table("nonsense").is_none());
    assert!(server_table("4").is_none(), "a value, not a table");
}
