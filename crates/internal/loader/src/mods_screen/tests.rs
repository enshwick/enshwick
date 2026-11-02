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
fn a_mods_page_shows_the_players_own_settings_first_then_the_worlds() {
    let page = page_settings(schema(), false).unwrap();
    assert_eq!(keys(&page), ["lamps", "note", "altars", "radius"]);
    let world_only = Schema::new("", "config", vec![field("a", Stage::Runtime, Scope::World)]);
    assert_eq!(keys(&page_settings(world_only, false).unwrap()), ["a"]);
    assert!(page_settings(Schema::new("", "config", Vec::new()), false).is_none());
}

#[test]
fn in_another_s_world_a_mods_page_shows_only_the_world_values_the_server_sends() {
    assert_eq!(
        keys(&page_settings(schema(), true).unwrap()),
        ["lamps", "note", "radius"]
    );
    let startup = Schema::new("", "config", vec![field("a", Stage::Startup, Scope::World)]);
    assert!(has_world_settings(&startup));
    assert!(page_settings(startup, true).is_none());
    let user_only = Schema::new("", "config", vec![field("b", Stage::Client, Scope::User)]);
    assert!(!has_world_settings(&user_only));
}

#[test]
fn in_another_s_world_only_the_world_rows_take_no_changes() {
    let schema = schema();
    for (sent, locked) in [(true, Lock::Server), (false, Lock::File)] {
        let mut shown = rows(&schema, "", None, None);
        lock_world_rows(&schema, &mut shown, sent);
        for row in &shown {
            let world = schema.field(&row.key).unwrap().scope == Scope::World;
            let want = if world { locked } else { Lock::Open };
            assert_eq!(row.lock, want, "{} sent {sent}", row.key);
        }
    }
    let synced = Schema::new(
        "",
        "config",
        vec![field("rule", Stage::Startup, Scope::World).with_synced(true)],
    );
    assert_eq!(keys(&page_settings(synced, true).unwrap()), ["rule"]);
}

#[test]
fn the_place_comes_from_the_menu_and_the_world_this_process_runs() {
    let main_menu = Place::new(false, false, false);
    assert_eq!(
        main_menu,
        Place {
            world: false,
            away: false,
            joined: false
        }
    );
    let own_world = Place::new(true, true, false);
    assert!(own_world.world && !own_world.away);
    let elsewhere = Place::new(true, false, false);
    assert!(elsewhere.away, "a vanilla server, or before the handshake");
    assert!(!elsewhere.joined);
    let modded = Place::new(true, false, true);
    assert!(modded.away && modded.joined);
    assert!(
        !Place::new(true, true, true).joined,
        "the host keeps its own"
    );
}

#[test]
fn world_settings_are_this_worlds_while_one_runs_else_the_defaults_of_new_worlds() {
    assert_eq!(
        section(Scope::World, true),
        ("mods.settings.world", Some("world.belongs"))
    );
    assert_eq!(
        section(Scope::World, false),
        ("mods.settings.new-worlds", Some("world.new"))
    );
    assert_eq!(section(Scope::User, true), ("mods.settings.yours", None));
    assert_eq!(section(Scope::User, false), ("mods.settings.yours", None));
    for (heading, note) in [section(Scope::World, true), section(Scope::World, false)] {
        assert_ne!(texts::fill(heading, &[]), heading, "{heading} has a text");
        let note = note.unwrap();
        assert_ne!(texts::fill(note, &[]), note, "{note} has a text");
    }
}

#[test]
fn a_servers_rows_take_no_changes() {
    let mut shown = rows(&schema(), "", None, None);
    lock_to_server(&mut shown);
    assert!(shown.iter().all(|r| r.lock == Lock::Server));
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
