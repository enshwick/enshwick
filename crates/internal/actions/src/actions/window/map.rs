//! The map of a window: markers the server puts on a player's own map while
//! they hold the window, and the actions the waypoint they set there runs.
//!
//! Its text is an `M` line, the action a waypoint at a marker runs and the
//! one a waypoint elsewhere runs (empty for none), then a `K` line for each
//! marker: its position, its label and the values its run gives.

use core::fmt::Write as _;

use super::{ARG, Text};
use crate::actions::Value;
use crate::actions::sheet::{self, clean};

/// The most markers a window's map holds.
pub const MAX_MARKERS: usize = 64;

/// How near a marker a waypoint counts as set on it: metres across the
/// ground, apart from height, which the map does not show.
pub const NEAR: f64 = 16.0;

/// A marker of a [`WindowMap`].
#[derive(Clone, Debug, PartialEq)]
struct Marker {
    position: [f64; 3],
    label: Text,
    args: Vec<Value>,
}

/// The map of a [`Window`](super::Window): markers on the player's own map
/// while they hold the window, and what the waypoint they set there runs.
///
/// A waypoint set within [`NEAR`] of a marker runs the
/// [`on_marker`](Self::on_marker) action with that marker's values; one
/// set elsewhere runs the [`on_point`](Self::on_point) action with its x,
/// y and z in metres. Only a waypoint moved while the player holds the
/// window runs anything, and the server checks each run as any other. The
/// markers past the room of a window's text are left out, the first kept.
///
/// # Examples
///
/// ```
/// use enshwick::PlayerKey;
/// use enshwick::actions::{Value, Window, WindowMap};
///
/// let ann = Value::Player { key: PlayerKey::new(7), name: "Ann".into() };
/// let map = WindowMap::new()
///     .marker([120.0, 41.5, -36.0], "Ann", &[ann])
///     .on_marker("goto")
///     .on_point("place");
/// let window = Window::new("players", "Players").map(map);
/// assert_eq!(window.id(), "players");
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowMap {
    on_marker: Option<String>,
    on_point: Option<String>,
    markers: Vec<Marker>,
}

impl WindowMap {
    /// A map without markers or actions.
    ///
    /// # Examples
    ///
    /// See [`WindowMap`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a marker at `position`, in metres, labelled with the text key
    /// `label`; a waypoint set on it runs the
    /// [`on_marker`](Self::on_marker) action with `args`, all the values
    /// that action takes.
    ///
    /// # Examples
    ///
    /// See [`WindowMap`].
    #[must_use]
    pub fn marker(mut self, position: [f64; 3], label: &str, args: &[Value]) -> Self {
        self.markers.push(Marker {
            position,
            label: Text::new(label, &[]),
            args: args.to_vec(),
        });
        self
    }

    /// The mod's action a waypoint set on a marker runs, with that
    /// marker's values.
    ///
    /// # Examples
    ///
    /// See [`WindowMap`].
    #[must_use]
    pub fn on_marker(mut self, action: &str) -> Self {
        self.on_marker = Some(action.to_owned());
        self
    }

    /// The mod's action a waypoint set away from every marker runs, with
    /// the waypoint's x, y and z in metres: an action of three
    /// [numbers](crate::actions::Arg::number).
    ///
    /// # Examples
    ///
    /// See [`WindowMap`].
    #[must_use]
    pub fn on_point(mut self, action: &str) -> Self {
        self.on_point = Some(action.to_owned());
        self
    }

    /// The ids of the actions the map runs.
    pub(super) fn actions(&self) -> impl Iterator<Item = &str> {
        self.on_marker
            .iter()
            .chain(&self.on_point)
            .map(String::as_str)
    }

    /// Why the map cannot be shown, `check` saying what is wrong with how
    /// it uses an action; `None` when it can.
    pub(super) fn fault(&self, check: super::Check<'_>) -> Option<String> {
        if self.markers.len() > MAX_MARKERS {
            return Some(format!(
                "a map of {} markers, at most {MAX_MARKERS}",
                self.markers.len()
            ));
        }
        if let Some(action) = &self.on_point
            && let Some(fault) = check(action, super::Use::Point)
        {
            return Some(fault);
        }
        let on_marker = self.on_marker.as_deref();
        self.markers.iter().find_map(|marker| match on_marker {
            Some(action) => check(action, super::Use::Marker(&marker.args)),
            None if marker.args.is_empty() => None,
            None => Some("a marker gives values, but the map runs no action on markers".into()),
        })
    }

    /// The map's lines within `room` bytes, `say` filling a label in the
    /// reader's language, and whether every marker fit: the markers past
    /// the room are left out, the first kept, and a map whose own line does
    /// not fit is left out whole.
    pub(super) fn lines(&self, say: super::Say<'_>, room: usize) -> (String, bool) {
        let id = |action: &Option<String>| action.as_deref().map_or_default(clean);
        let mut lines = format!("M\t{}\t{}\n", id(&self.on_marker), id(&self.on_point));
        if lines.len() > room {
            return (String::new(), false);
        }
        let mut line = String::new();
        for marker in &self.markers {
            let [x, y, z] = marker.position;
            let args = sheet::encode_args(&marker.args).replace('\t', ARG);
            line.clear();
            let _ = writeln!(line, "K\t{x}\t{y}\t{z}\t{}\t{args}", marker.label.line(say));
            if lines.len() + line.len() > room {
                return (lines, false);
            }
            lines.push_str(&line);
        }
        (lines, true)
    }
}

/// The map of a window as the client reads it.
///
/// # Examples
///
/// ```
/// use enshwick::actions::window::{ItemView, decode};
///
/// let view = decode("W\tPlayers\nM\tgoto\tplace\nK\t10\t2\t-5\tAnn\t\n").unwrap();
/// let Some(ItemView::Map(map)) = view.items.first() else { return };
/// assert_eq!(map.markers[0].label, "Ann");
/// assert_eq!(map.run_at([12.0, 0.0, -5.0]).map(|(action, _)| action), Some("goto"));
/// assert_eq!(map.run_at([500.0, 0.0, 0.0]).map(|(action, _)| action), Some("place"));
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct MapView {
    /// The action a waypoint on a marker runs.
    pub on_marker: Option<String>,
    /// The action a waypoint elsewhere runs.
    pub on_point: Option<String>,
    /// The markers, in order.
    pub markers: Vec<MarkerView>,
}

/// A marker of a [`MapView`].
///
/// # Examples
///
/// See [`MapView`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MarkerView {
    /// Where, in metres.
    pub position: [f64; 3],
    /// Its label, in the player's language.
    pub label: String,
    /// The values the run of a waypoint on it gives.
    pub args: Vec<Value>,
}

impl MarkerView {
    /// A `MarkerView` of its parts.
    ///
    /// # Examples
    ///
    /// See [`MarkerView`].
    #[must_use]
    pub const fn new(position: [f64; 3], label: String, args: Vec<Value>) -> Self {
        Self {
            position,
            label,
            args,
        }
    }
}

impl MapView {
    /// The map of an `M` line's fields: the action ids, a malformed one
    /// read as none.
    pub(super) fn of(on_marker: &str, on_point: &str) -> Self {
        let id = |id: &str| crate::actions::is_valid_id(id).then(|| id.to_owned());
        Self {
            on_marker: id(on_marker),
            on_point: id(on_point),
            markers: Vec::new(),
        }
    }

    /// Adds the marker of a `K` line's fields, when it reads and the map
    /// has room.
    pub(super) fn add(&mut self, fields: [&str; 5], cut: impl Fn(&str) -> String) {
        let [x, y, z, label, args] = fields;
        let axis = |text: &str| text.parse::<f64>().ok().filter(|n| n.is_finite());
        let (Some(x), Some(y), Some(z), Some(args)) = (
            axis(x),
            axis(y),
            axis(z),
            sheet::decode_args(&args.replace(ARG, "\t")),
        ) else {
            return;
        };
        if self.markers.len() < MAX_MARKERS {
            self.markers.push(MarkerView {
                position: [x, y, z],
                label: cut(label),
                args,
            });
        }
    }

    /// The run a waypoint the player set at `at` asks for: the
    /// [`on_marker`](Self::on_marker) action with the values of the
    /// nearest marker within [`NEAR`], else the
    /// [`on_point`](Self::on_point) action with `at`; `None` when the map
    /// has no such action.
    ///
    /// # Examples
    ///
    /// See [`MapView`].
    #[must_use]
    pub fn run_at(&self, at: [f64; 3]) -> Option<(&str, Vec<Value>)> {
        let across =
            |marker: &MarkerView| (marker.position[0] - at[0]).hypot(marker.position[2] - at[2]);
        let on_marker = self.on_marker.as_deref().and_then(|action| {
            let nearest = self
                .markers
                .iter()
                .map(|marker| (across(marker), marker))
                .filter(|(distance, _)| *distance <= NEAR)
                .min_by(|(a, _), (b, _)| a.total_cmp(b))?;
            Some((action, nearest.1.args.clone()))
        });
        on_marker.or_else(|| {
            let action = self.on_point.as_deref()?;
            Some((action, at.map(Value::Number).to_vec()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ItemView, Use, Window, decode};
    use super::*;
    use crate::PlayerKey;

    fn say(key: &str, _: &[(&str, &str)]) -> String {
        key.to_owned()
    }

    fn player(key: u64) -> Value {
        Value::Player {
            key: PlayerKey::new(key),
            name: String::new(),
        }
    }

    fn map_of(text: &str) -> MapView {
        let view = decode(text).unwrap();
        match view.items.as_slice() {
            [ItemView::Map(map)] => map.clone(),
            items => panic!("one map: {items:?}"),
        }
    }

    #[test]
    fn a_map_round_trips_with_its_markers_and_actions() {
        let window = Window::new("players", "Players").map(
            WindowMap::new()
                .marker([120.25, 41.5, -36.0], "Ann", &[player(7)])
                .marker([-0.1, 0.0, 1e6], "Bob\tB", &[player(8)])
                .on_marker("goto")
                .on_point("place"),
        );
        let map = map_of(&window.render(&say, &[]));
        assert_eq!(map.on_marker.as_deref(), Some("goto"));
        assert_eq!(map.on_point.as_deref(), Some("place"));
        assert_eq!(
            map.markers,
            [
                MarkerView {
                    position: [120.25, 41.5, -36.0],
                    label: "Ann".into(),
                    args: vec![player(7)],
                },
                MarkerView {
                    position: [-0.1, 0.0, 1e6],
                    label: "Bob B".into(),
                    args: vec![player(8)],
                },
            ]
        );
        assert_eq!(
            window.actions().collect::<Vec<_>>(),
            ["goto", "place"],
            "a player gets the map only when they may run both"
        );
    }

    #[test]
    fn a_map_past_the_window_s_room_keeps_its_first_markers_and_its_actions() {
        let long = "L".repeat(200);
        let mut map = WindowMap::new().on_marker("goto").on_point("place");
        for n in 0..MAX_MARKERS {
            map = map.marker(
                [f64::from(u8::try_from(n).unwrap()), 0.0, 0.0],
                &long,
                &[player(7)],
            );
        }
        let window = Window::new("w", "W")
            .text("first", &[])
            .action("kick")
            .map(map);
        assert!(window.draws("kick"));
        assert!(!window.draws("goto"), "a map's action needs no sheet entry");
        let text = window.render(&say, &[]);
        assert!(text.len() <= super::super::MAX_BYTES);
        let view = decode(&text).unwrap();
        let Some(ItemView::Map(kept)) = view.items.last() else {
            panic!("the map stays: {:?}", view.items);
        };
        assert_eq!(kept.on_point.as_deref(), Some("place"));
        assert!(
            (1..MAX_MARKERS).contains(&kept.markers.len()),
            "{} markers",
            kept.markers.len()
        );
        assert_eq!(kept.markers[0].position, [0.0, 0.0, 0.0], "the first kept");
    }

    #[test]
    fn a_map_without_actions_reads_as_one_without_them() {
        let map = map_of(
            &Window::new("w", "W")
                .map(WindowMap::new())
                .render(&say, &[]),
        );
        assert_eq!(map, MapView::default());
        let bad = map_of("W\tw\nM\tBAD ID\t\n");
        assert_eq!(bad.on_marker, None, "a malformed id runs nothing");
    }

    #[test]
    fn markers_that_do_not_read_or_have_no_room_or_map_are_left_out() {
        let mut text = "W\tw\nK\t1\t2\t3\tlost\t\nM\tgoto\t\n".to_owned();
        text.push_str("K\tNaN\t0\t0\tnan\t\nK\t1\tx\t0\tword\t\nK\t1\t2\t3\tbad\tq9\n");
        for n in 0..=MAX_MARKERS {
            let _ = writeln!(text, "K\t{n}\t0\t0\tm\t");
        }
        let map = map_of(&text);
        assert_eq!(map.markers.len(), MAX_MARKERS);
        assert_eq!(
            map.markers[0].position,
            [0.0, 0.0, 0.0],
            "the first that reads"
        );
    }

    #[test]
    fn a_waypoint_runs_the_nearest_marker_within_reach_else_the_point() {
        let map = MapView {
            on_marker: Some("goto".into()),
            on_point: Some("place".into()),
            markers: vec![
                MarkerView {
                    position: [100.0, 0.0, 100.0],
                    label: "Ann".into(),
                    args: vec![player(7)],
                },
                MarkerView {
                    position: [110.0, 0.0, 100.0],
                    label: "Bob".into(),
                    args: vec![player(8)],
                },
            ],
        };
        assert_eq!(
            map.run_at([108.0, 500.0, 100.0]),
            Some(("goto", vec![player(8)])),
            "the nearest across the ground, whatever the height"
        );
        assert_eq!(
            map.run_at([100.0, 0.0, 100.0 + NEAR]),
            Some(("goto", vec![player(7)])),
            "at the edge of reach"
        );
        let far = [100.0, 3.0, 100.0 + NEAR + 0.5];
        assert_eq!(
            map.run_at(far),
            Some((
                "place",
                vec![
                    Value::Number(100.0),
                    Value::Number(3.0),
                    Value::Number(116.5)
                ]
            ))
        );
        let markers_only = MapView {
            on_point: None,
            ..map.clone()
        };
        assert_eq!(markers_only.run_at(far), None);
        let points_only = MapView {
            on_marker: None,
            ..map
        };
        assert_eq!(
            points_only
                .run_at([100.0, 0.0, 100.0])
                .map(|(action, _)| action),
            Some("place"),
            "with no marker action a waypoint on a marker is a point"
        );
    }

    #[test]
    fn a_map_names_its_faults() {
        let check = |action: &str, used: Use<'_>| match (action, used) {
            ("goto", Use::Marker([Value::Player { .. }])) | ("place", Use::Point) => None,
            (action, used) => Some(format!("{action} {used:?}")),
        };
        let fine = WindowMap::new()
            .marker([0.0; 3], "Ann", &[player(7)])
            .on_marker("goto")
            .on_point("place");
        assert_eq!(fine.fault(&check), None);
        let crowd =
            (0..=MAX_MARKERS).fold(WindowMap::new(), |map, _| map.marker([0.0; 3], "m", &[]));
        assert!(crowd.fault(&check).unwrap().contains("at most"));
        let most = (0..MAX_MARKERS).fold(WindowMap::new(), |map, _| map.marker([0.0; 3], "m", &[]));
        assert_eq!(most.fault(&check), None, "at the bound, not past it");
        let bad_point = WindowMap::new().on_point("goto");
        assert_eq!(bad_point.fault(&check).as_deref(), Some("goto Point"));
        let bad_values = WindowMap::new()
            .marker([0.0; 3], "Ann", &[])
            .on_marker("goto");
        assert!(bad_values.fault(&check).unwrap().starts_with("goto Marker"));
        let unrun = WindowMap::new().marker([0.0; 3], "Ann", &[player(7)]);
        assert!(
            unrun
                .fault(&check)
                .unwrap()
                .contains("no action on markers")
        );
        let one = Window::new("w", "W").map(fine.clone());
        assert_eq!(one.fault(&check), None);
        let two = one.map(fine);
        assert_eq!(two.fault(&check).as_deref(), Some("2 maps, at most one"));
    }
}
