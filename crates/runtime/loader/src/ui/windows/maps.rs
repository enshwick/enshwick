//! The maps of carried windows on the server.
//!
//! The server notes which map each client holds and the runs the waypoints
//! its player sets ask for. It reads a window's map from the text it carried, so a mod's
//! map acts only while its client holds the window. The loader's one
//! waypoint watcher hands on each change, so a waypoint set before the
//! window showed, or moved while it was closed, runs nothing.

use enshwick_core::abi::Metres;
use enshwick_core::actions::Value;
use enshwick_core::actions::window::{ItemView, MapView, decode};

use super::Held;
use crate::base::remote::Message;
use crate::engine::mod_index::ModIndex;

/// A map a client holds, in a window of mod `owner`.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldMap {
    /// The player's account key.
    pub key: u64,
    /// The mod whose window it is.
    pub owner: ModIndex,
    /// The window's id.
    pub window: String,
    /// The map as the server carried it to that player.
    pub map: MapView,
}

/// A run a player's waypoint asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct WaypointRun {
    /// The player's account key.
    pub key: u64,
    /// The mod whose action runs.
    pub owner: ModIndex,
    /// The action.
    pub action: String,
    /// Its values.
    pub args: Vec<Value>,
}

/// The map of a window's text; `None` for a window without one, found
/// without reading the rest of a text that has no map line.
fn map_of(text: &str) -> Option<MapView> {
    if !text.starts_with("M\t") && !text.contains("\nM\t") {
        return None;
    }
    decode(text)?.items.into_iter().find_map(|item| match item {
        ItemView::Map(map) => Some(map),
        _ => None,
    })
}

/// Notes in `maps` the map client `key` holds once `message` of mod `owner`
/// reached it.
///
/// The window's map takes the place of the one before; a window without
/// one or a close leaves none. [`kept`] then orders the maps as the client
/// holds their windows.
pub fn note_map(maps: &mut Vec<HeldMap>, key: u64, owner: ModIndex, message: &Message) {
    let (window, map) = match message {
        Message::Window { window, text, .. } => (window, map_of(text)),
        Message::WindowClose { window, .. } => (window, None),
        _ => return,
    };
    let at = maps
        .iter()
        .position(|m| m.key == key && m.owner == owner && m.window == *window);
    match (at, map) {
        (Some(at), Some(map)) => maps[at].map = map,
        (Some(at), None) => {
            maps.remove(at);
        }
        (None, Some(map)) => maps.push(HeldMap {
            key,
            owner,
            window: window.clone(),
            map,
        }),
        (None, None) => {}
    }
}

/// Keeps in `maps` only those of windows `held` says a client still holds.
///
/// They keep the order the client holds them in, so a player's last map is
/// the one of their newest window with a map, as the client shows it.
pub fn kept(maps: &mut Vec<HeldMap>, held: &Held) {
    let place = |m: &HeldMap| {
        held.iter().position(|(key, owner, window)| {
            *key == m.key && *owner == m.owner && *window == m.window
        })
    };
    maps.retain(|m| place(m).is_some());
    maps.sort_by_key(place);
}

/// The run player `key`'s waypoint, set or moved to `at`, asks of the
/// newest of `maps` that player holds; `None` when they hold none or it
/// runs nothing there.
#[must_use]
pub fn run_at(maps: &[HeldMap], key: u64, at: Metres) -> Option<WaypointRun> {
    let held = maps.iter().rev().find(|m| m.key == key)?;
    let (action, args) = held.map.run_at(at)?;
    Some(WaypointRun {
        key,
        owner: held.owner,
        action: action.to_owned(),
        args,
    })
}

#[cfg(test)]
mod tests {
    use enshwick_core::PlayerKey;

    use super::*;

    fn shown(window: &str, text: &str) -> Message {
        Message::Window {
            id: "you.admin".into(),
            window: window.into(),
            open: true,
            text: text.into(),
        }
    }

    const MAP: &str = "W\tPlayers\nM\tgoto\tplace\nK\t100\t0\t100\tAnn\tp0000000000000007\n";

    fn held_map(key: u64, window: &str) -> HeldMap {
        let mut maps = Vec::new();
        note_map(&mut maps, key, ModIndex::new(0), &shown(window, MAP));
        maps.remove(0)
    }

    #[test]
    fn a_client_holds_the_map_of_the_last_text_carried_until_it_closes() {
        let mine = ModIndex::new(0);
        let mut maps = Vec::new();
        note_map(&mut maps, 7, mine, &shown("panel", "W\tPanel\nT\tno map\n"));
        assert!(maps.is_empty(), "a window without a map");
        note_map(&mut maps, 7, mine, &shown("players", MAP));
        note_map(&mut maps, 8, mine, &shown("players", MAP));
        assert_eq!(maps.len(), 2);
        assert_eq!(maps[0].map.markers.len(), 1);

        let moved = MAP.replace("K\t100\t0\t100", "K\t5\t0\t5");
        note_map(&mut maps, 7, mine, &shown("players", &moved));
        assert_eq!(maps[0].map.markers[0].position, [5.0, 0.0, 5.0], "in place");
        note_map(&mut maps, 7, mine, &shown("players", "W\tPlayers\n"));
        assert_eq!(maps.len(), 1, "a render without the map takes it away");
        let close = Message::WindowClose {
            id: "you.admin".into(),
            window: "players".into(),
        };
        note_map(&mut maps, 8, mine, &close);
        assert!(maps.is_empty());
    }

    #[test]
    fn only_the_maps_of_windows_still_held_stay() {
        let mut maps = vec![held_map(7, "players"), held_map(8, "players")];
        let still: Held = vec![(8, ModIndex::new(0), "players".to_owned())];
        kept(&mut maps, &still);
        assert_eq!(maps, [held_map(8, "players")]);
    }

    #[test]
    fn a_window_that_gains_a_map_keeps_the_place_the_client_holds_it_in() {
        let mine = ModIndex::new(0);
        let held: Held = vec![
            (7, mine, "panel".to_owned()),
            (7, mine, "players".to_owned()),
        ];
        let mut maps = vec![held_map(7, "players")];
        note_map(&mut maps, 7, mine, &shown("panel", MAP));
        kept(&mut maps, &held);
        let order: Vec<&str> = maps.iter().map(|m| m.window.as_str()).collect();
        assert_eq!(
            order,
            ["panel", "players"],
            "the client's top window is last"
        );
    }

    #[test]
    fn a_waypoint_runs_the_map_s_action_with_the_marker_near_it_or_the_place() {
        let maps = vec![held_map(7, "players")];
        let ann = Value::Player {
            key: PlayerKey::new(7),
            name: String::new(),
        };
        assert_eq!(
            run_at(&maps, 7, [101.0, 0.0, 100.0]),
            Some(WaypointRun {
                key: 7,
                owner: ModIndex::new(0),
                action: "goto".into(),
                args: vec![ann],
            })
        );
        assert_eq!(
            run_at(&maps, 7, [400.0, 2.0, 100.0]).map(|run| run.args),
            Some(vec![
                Value::Number(400.0),
                Value::Number(2.0),
                Value::Number(100.0)
            ])
        );
        assert_eq!(run_at(&maps, 8, [101.0, 0.0, 100.0]), None, "no map held");
    }

    #[test]
    fn a_waypoint_runs_on_the_newest_map_its_player_holds() {
        let other = MAP.replace("M\tgoto\tplace", "M\t\tmark");
        let mut maps = vec![held_map(7, "players")];
        note_map(&mut maps, 7, ModIndex::new(1), &shown("marks", &other));
        let run = run_at(&maps, 7, [100.0, 0.0, 100.0]).unwrap();
        assert_eq!((run.owner, run.action.as_str()), (ModIndex::new(1), "mark"));
    }
}
