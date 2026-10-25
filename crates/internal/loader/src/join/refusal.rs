//! The one dialog a client shows when it stays out of a server: each of the
//! server's mods it lacks, with the server's version, grouped by where to get
//! them.
//!
//! The game's popup centres every line and wraps it at a fixed width (client
//! build 426), so lines stay short: a heading per place, then one mod per
//! line, and no more lines than keep the button on a 1080p screen.

use std::collections::BTreeMap;

use super::Names;
use crate::admission::Need;
use crate::texts;

/// The most lines the list takes, headings and the count of the rest among
/// them.
const MAX_LINES: usize = 12;

/// The texts of the refusal dialog.
///
/// # Examples
///
/// ```
/// use enshwick::admission::Need;
/// use enshwick::join::{Names, refusal};
///
/// let need = Need { id: "safes".into(), version: "0.3.0".into(), yours: None };
/// let dialog = refusal::dialog(&[need], &refusal::Off::default(), &Names::new(), &Default::default());
/// assert!(dialog.detail.lines().any(|line| line == "safes 0.3.0"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    /// The popup's title, on one line.
    pub title: String,
    /// What happened and what to do, over the list.
    pub message: String,
    /// The mods, grouped by where to get them.
    pub detail: String,
}

/// The client's mods that are off, by id: the server's mods among them are
/// the player's to turn on, not to install.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Off {
    /// Turned off by the player in the mod list.
    pub by_player: Vec<String>,
    /// Turned off by the loader since the game started.
    pub this_game: Vec<String>,
}

/// Where the player gets a mod, in the order the dialog lists them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Get {
    /// From the page the server's update source for it names.
    From(String),
    /// From the server's admin: the server names no source.
    Ask,
    /// By turning it on in the mod list.
    TurnOn,
    /// By restarting the game, which turned it off.
    Restart,
}

/// The dialog for a client that lacks `needs`, the server's mods it does not
/// run in the server's version; mods named by `names`, places from the
/// update sources the server advertises, `sources` by id.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
///
/// use enshwick::admission::Need;
/// use enshwick::join::{Names, refusal};
///
/// let need = Need { id: "safes".into(), version: "0.3.0".into(), yours: Some("0.2.0".into()) };
/// let sources = BTreeMap::from([("safes".to_owned(), "github:someone/safes:safes".to_owned())]);
/// let dialog = refusal::dialog(&[need], &refusal::Off::default(), &Names::new(), &sources);
/// assert!(dialog.detail.contains("github.com/someone/safes/releases"));
/// ```
#[must_use]
pub fn dialog(
    needs: &[Need],
    off: &Off,
    names: &Names,
    sources: &BTreeMap<String, String>,
) -> Dialog {
    let mut groups: BTreeMap<Get, Vec<String>> = BTreeMap::new();
    for need in needs {
        let get = if off.by_player.contains(&need.id) {
            Get::TurnOn
        } else if off.this_game.contains(&need.id) {
            Get::Restart
        } else {
            sources
                .get(&need.id)
                .and_then(|line| crate::admission::place(line))
                .map_or(Get::Ask, |place| {
                    Get::From(place.trim_start_matches("https://").to_owned())
                })
        };
        let name = super::label(names, &need.id);
        let mut args = vec![("mod", name.as_str()), ("version", need.version.as_str())];
        let line = match &need.yours {
            Some(yours) if matches!(get, Get::From(_) | Get::Ask) => {
                args.push(("yours", yours.as_str()));
                texts::fill("join.dialog.mod.yours", &args)
            }
            _ => texts::fill("join.dialog.mod", &args),
        };
        groups.entry(get).or_default().push(line);
    }
    let mut lines = Vec::with_capacity(MAX_LINES);
    let mut listed = 0;
    'groups: for (get, mods) in &groups {
        // A heading needs one of its mods beside it, and the count a line.
        if lines.len() + 2 > MAX_LINES - 1 {
            break;
        }
        lines.push(match get {
            Get::From(place) => texts::fill("join.dialog.from", &[("place", place)]),
            Get::Ask => texts::fill("join.dialog.ask", &[]),
            Get::TurnOn => texts::fill(
                "join.dialog.turn-on",
                &[("file", crate::modlist::LIST_PATH)],
            ),
            Get::Restart => texts::fill("join.dialog.restart", &[]),
        });
        for line in mods {
            let last_room = lines.len() + 1 == MAX_LINES;
            if last_room && listed + 1 < needs.len() {
                break 'groups;
            }
            lines.push(line.clone());
            listed += 1;
        }
    }
    if listed < needs.len() {
        let count = (needs.len() - listed).to_string();
        lines.push(texts::fill("join.refused.more", &[("count", &count)]));
    }
    Dialog {
        title: texts::fill("join.dialog.title", &[]),
        message: texts::fill("join.dialog.message", &[]),
        detail: lines.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn need(id: &str, version: &str, yours: Option<&str>) -> Need {
        Need {
            id: id.to_owned(),
            version: version.to_owned(),
            yours: yours.map(str::to_owned),
        }
    }

    fn lines(dialog: &Dialog) -> Vec<&str> {
        dialog.detail.lines().collect()
    }

    #[test]
    fn missing_mods_are_listed_under_where_to_get_them() {
        let names = Names::from([("build-camera".to_owned(), "Build camera".to_owned())]);
        let sources = BTreeMap::from([
            (
                "build-camera".to_owned(),
                "github:someone/mods:build-camera".to_owned(),
            ),
            ("safes".to_owned(), "github:someone/mods:safes".to_owned()),
        ]);
        let dialog = dialog(
            &[
                need("build-camera", "0.3.0", None),
                need("safes", "0.2.0", Some("0.1.0")),
                need("zones", "0.1.0", None),
            ],
            &Off::default(),
            &names,
            &sources,
        );
        assert_eq!(
            lines(&dialog),
            [
                "From github.com/someone/mods/releases:",
                "Build camera (build-camera) 0.3.0",
                "safes 0.2.0, yours 0.1.0",
                "Ask the server's admin for:",
                "zones 0.1.0",
            ]
        );
        assert!(!dialog.title.is_empty() && !dialog.message.is_empty());
    }

    #[test]
    fn the_dialog_never_offers_a_download() {
        let dialog = dialog(
            &[need("safes", "0.2.0", None)],
            &Off::default(),
            &Names::new(),
            &BTreeMap::new(),
        );
        let all = format!("{} {} {}", dialog.title, dialog.message, dialog.detail).to_lowercase();
        assert!(!all.contains("download"), "{all}");
    }

    #[test]
    fn mods_the_player_or_the_game_turned_off_are_to_turn_on_not_to_install() {
        let off = Off {
            by_player: vec!["safes".to_owned()],
            this_game: vec!["count".to_owned()],
        };
        let sources =
            BTreeMap::from([("safes".to_owned(), "github:someone/mods:safes".to_owned())]);
        let dialog = dialog(
            &[need("count", "0.2.0", None), need("safes", "0.3.0", None)],
            &off,
            &Names::new(),
            &sources,
        );
        assert_eq!(
            lines(&dialog),
            [
                "Turn on in mods/enshwick/mods.toml:",
                "safes 0.3.0",
                "Turned off in this game; restart it:",
                "count 0.2.0",
            ]
        );
    }

    #[test]
    fn every_line_is_short_enough_to_stay_on_one_line() {
        let id = "survivors-per-altar";
        let sources = BTreeMap::from([(id.to_owned(), format!("github:themaiby/enshwick:{id}"))]);
        let off = Off {
            by_player: vec!["safes".to_owned()],
            this_game: vec!["count".to_owned()],
        };
        let dialog = dialog(
            &[
                need(id, "0.1.4", Some("0.1.3")),
                need("safes", "0.1.0", None),
                need("count", "0.1.0", None),
                need("zones", "0.1.0", None),
            ],
            &off,
            &Names::new(),
            &sources,
        );
        for line in lines(&dialog) {
            assert!(line.chars().count() <= 44, "{line}");
        }
    }

    #[test]
    fn a_long_list_keeps_to_its_lines_and_counts_the_rest() {
        let needs: Vec<Need> = (0..20)
            .map(|i| need(&format!("mod-{i:02}"), "1.0.0", None))
            .collect();
        let dialog = dialog(&needs, &Off::default(), &Names::new(), &BTreeMap::new());
        let lines = lines(&dialog);
        assert_eq!(lines.len(), MAX_LINES, "{}", dialog.detail);
        assert_eq!(lines[0], "Ask the server's admin for:");
        assert_eq!(lines[MAX_LINES - 1], "And 10 more.");
    }

    #[test]
    fn a_list_that_fits_is_never_cut() {
        let needs: Vec<Need> = (0..11)
            .map(|i| need(&format!("mod-{i:02}"), "1.0.0", None))
            .collect();
        let dialog = dialog(&needs, &Off::default(), &Names::new(), &BTreeMap::new());
        let lines = lines(&dialog);
        assert_eq!(lines.len(), MAX_LINES);
        assert_eq!(lines[MAX_LINES - 1], "mod-10 1.0.0");
    }
}
