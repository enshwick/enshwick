//! The maps of carried windows on the server.
//!
//! The server notes which map each client holds and the runs the waypoints
//! its player sets ask for. It reads a window's map from the text it carried, so a mod's
//! map acts only while its client holds the window. A player's waypoint is
//! looked at every [`WATCH_MS`]; the first look only notes it, so a waypoint
//! set before the window showed runs nothing.

use enshwick_core::actions::Value;
use enshwick_core::actions::window::{ItemView, MapView, decode};

use super::Held;
use crate::base::remote::Message;
use crate::engine::mod_index::ModIndex;

/// How often the server looks at the waypoints of players holding a map:
/// a person moves one far more slowly, so every change is seen.
pub const WATCH_MS: u64 = 100;

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

/// The map of a window's text; `None` for a window without one.
fn map_of(text: &str) -> Option<MapView> {
    decode(text)?.items.into_iter().find_map(|item| match item {
        ItemView::Map(map) => Some(map),
        _ => None,
    })
}

/// Notes in `maps` the map client `key` holds once `message` of mod `owner`
/// reached it.
///
/// The window's map takes the place of the one before; a window without
/// one or a close leaves none. A map keeps its place, so the newest
/// window's map stays last.
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
pub fn kept(maps: &mut Vec<HeldMap>, held: &Held) {
    maps.retain(|m| super::holds(held, m.key, m.owner, &m.window));
}

/// The runs the waypoints of the players holding `maps` ask for, given
/// `seen`, each player's waypoint at the last look, which it moves on;
/// `waypoint` reads a player's waypoint now, `None` when it cannot.
///
/// A player's waypoint runs on their newest map, and only once it changed
/// to a place since the last look.
pub fn runs(
    maps: &[HeldMap],
    seen: &mut Vec<(u64, Option<[f64; 3]>)>,
    waypoint: impl Fn(u64) -> Option<Option<[f64; 3]>>,
) -> Vec<WaypointRun> {
    seen.retain(|(key, _)| maps.iter().any(|m| m.key == *key));
    let mut runs = Vec::new();
    for (index, held) in maps.iter().enumerate() {
        let newest = !maps[index + 1..].iter().any(|m| m.key == held.key);
        if !newest {
            continue;
        }
        let Some(now) = waypoint(held.key) else {
            continue;
        };
        let looked = seen.iter_mut().find(|(key, _)| *key == held.key);
        let before = looked.map(|(_, at)| core::mem::replace(at, now));
        if before.is_none() {
            seen.push((held.key, now));
        }
        let (Some(before), Some(at)) = (before, now) else {
            continue;
        };
        if before == Some(at) {
            continue;
        }
        if let Some((action, args)) = held.map.run_at(at) {
            runs.push(WaypointRun {
                key: held.key,
                owner: held.owner,
                action: action.to_owned(),
                args,
            });
        }
    }
    runs
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

    fn held(key: u64, window: &str) -> HeldMap {
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
        let mut maps = vec![held(7, "players"), held(8, "players")];
        let still: Held = vec![(8, ModIndex::new(0), "players".to_owned())];
        kept(&mut maps, &still);
        assert_eq!(maps, [held(8, "players")]);
    }

    #[test]
    fn a_waypoint_runs_only_once_it_moves_to_a_place_after_the_first_look() {
        let maps = vec![held(7, "players")];
        let mut seen = Vec::new();
        let ann = Value::Player {
            key: PlayerKey::new(7),
            name: String::new(),
        };
        let at = |place: Option<[f64; 3]>| move |_| Some(place);

        let near = Some([101.0, 0.0, 100.0]);
        assert!(
            runs(&maps, &mut seen, at(near)).is_empty(),
            "the first look notes it"
        );
        assert!(runs(&maps, &mut seen, at(near)).is_empty(), "unchanged");
        assert!(runs(&maps, &mut seen, at(None)).is_empty(), "cleared");
        assert_eq!(
            runs(&maps, &mut seen, at(near)),
            [WaypointRun {
                key: 7,
                owner: ModIndex::new(0),
                action: "goto".into(),
                args: vec![ann],
            }]
        );
        let far = Some([400.0, 2.0, 100.0]);
        assert_eq!(
            runs(&maps, &mut seen, at(far))[0].args,
            [
                Value::Number(400.0),
                Value::Number(2.0),
                Value::Number(100.0)
            ]
        );
        assert!(
            runs(&maps, &mut seen, |_| None).is_empty(),
            "an unread waypoint changes nothing"
        );
        assert!(runs(&maps, &mut seen, at(far)).is_empty());
        assert!(runs(&[], &mut seen, at(near)).is_empty());
        assert!(seen.is_empty(), "a player without a map is forgotten");
    }

    #[test]
    fn a_waypoint_runs_on_the_newest_map_its_player_holds() {
        let other = MAP.replace("M\tgoto\tplace", "M\t\tmark");
        let mut maps = vec![held(7, "players")];
        note_map(&mut maps, 7, ModIndex::new(1), &shown("marks", &other));
        let mut seen = vec![(7, None)];
        let run = runs(&maps, &mut seen, |_| Some(Some([100.0, 0.0, 100.0])));
        assert_eq!(run.len(), 1);
        assert_eq!(
            (run[0].owner, run[0].action.as_str()),
            (ModIndex::new(1), "mark")
        );
    }
}
