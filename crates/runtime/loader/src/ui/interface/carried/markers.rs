//! The markers of the carried windows' map on the game's own map: those of
//! the newest held window with a map, the one whose waypoints the server
//! runs.

use enshwick_core::abi::{Header, Str, UiMarker};
use enshwick_core::actions::window::{ItemView, MapView};

use super::{Held, STATE};
use enshwick_base::Relock;

/// The map of the newest held window with one, the map whose waypoints
/// the server runs.
fn newest_map(held: &[Held]) -> Option<&MapView> {
    held.iter().rev().find_map(|h| {
        h.view.items.iter().find_map(|item| match item {
            ItemView::Map(map) => Some(map),
            _ => None,
        })
    })
}

/// Puts the markers of the newest held window's map on the game's map as
/// the loader's, once they changed.
pub(super) fn mark() {
    let mut marked = STATE.marked.relock();
    let held = STATE.held.load();
    let newest = newest_map(held.as_deref().map_or(&[], Vec::as_slice));
    if marked.as_ref() == newest {
        return;
    }
    let markers: Vec<UiMarker> = newest
        .into_iter()
        .flat_map(|map| &map.markers)
        .take(super::super::map::MAX_MARKERS)
        .map(|marker| UiMarker {
            header: Header::of::<UiMarker>(),
            position: marker.position,
            label: Str::new(&marker.label),
            color: UiMarker::YELLOW,
            marker_type: 0,
        })
        .collect();
    // Refused only while the map markers are off, which their fuse logs
    // once; `take` keeps the count in bounds.
    let _off = super::super::map::set_carried(&markers);
    *marked = newest.cloned();
}

#[cfg(test)]
mod tests {
    use enshwick_core::actions::window::{MarkerView, View};

    use super::*;

    fn window(id: &str, x: Option<f64>) -> Held {
        let map = x.map(|x| {
            ItemView::Map(MapView {
                markers: vec![MarkerView {
                    position: [x, 0.0, 0.0],
                    label: "m".into(),
                    args: Vec::new(),
                }],
                ..MapView::default()
            })
        });
        Held {
            mod_id: "you.admin".into(),
            window: id.into(),
            view: View {
                items: map.into_iter().collect(),
                ..View::default()
            },
        }
    }

    #[test]
    fn only_the_newest_held_window_s_map_puts_its_markers_on_the_map() {
        let first = |held: &[Held]| newest_map(held).map(|m| m.markers[0].position[0]);
        let held = [window("old", Some(1.0)), window("new", Some(2.0))];
        assert_eq!(first(&held), Some(2.0), "the map whose waypoints run");
        let under = [window("old", Some(1.0)), window("panel", None)];
        assert_eq!(
            first(&under),
            Some(1.0),
            "a newer window without a map leaves the older map's markers"
        );
        assert_eq!(first(&[window("panel", None)]), None);
    }
}
