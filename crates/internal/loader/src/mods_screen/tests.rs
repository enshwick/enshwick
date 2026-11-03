use enshwick::config::Field;

use super::*;
use crate::configure::rows::Widget;

fn schema() -> Schema {
    Schema::new(
        "",
        "config",
        vec![
            Field::new("lamps", &true),
            Field::new("radius", &4.0).with_range(Some(0.0), Some(8.0)),
            Field::new("token", "").with_secret(true),
        ],
    )
}

fn row(lock: Lock) -> Row {
    Row {
        key: "lamps".to_owned(),
        label: "Lamps".to_owned(),
        group: None,
        doc: String::new(),
        widget: Widget::Toggle(true),
        lock,
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
    assert_eq!(
        back(Kind::ModServerSettings),
        Back::List,
        "a server page a mod opened goes back to the mods"
    );
    assert_eq!(back(Kind::Actions), Back::Inside);
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
fn the_gear_shows_only_where_a_world_s_settings_are_at_hand() {
    assert!(!gear(false, false), "the main menu");
    assert!(gear(true, false), "single player or a hosted world");
    assert!(gear(false, true), "another's world whose server sent them");
    assert_ne!(
        texts::fill("mods.settings.in-world", &[]),
        "mods.settings.in-world"
    );
}

#[test]
fn in_another_s_world_every_row_is_the_server_s_and_no_secret_shows() {
    let shown = server_rows(schema(), "lamps = false\nradius = 2.0\n", None);
    let keys: Vec<&str> = shown.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["lamps", "radius"]);
    assert!(shown.iter().all(|r| r.lock == Lock::Server));
    assert_eq!(shown[0].widget, Widget::Toggle(false));
}

#[test]
fn a_rows_label_names_the_server_that_sets_it() {
    assert_eq!(row_label(&row(Lock::Open)), "Lamps");
    assert_eq!(row_label(&row(Lock::File)), "Lamps");
    let set = texts::fill("row.server", &[("label", "Lamps")]);
    assert_eq!(row_label(&row(Lock::Server)), set);
}
