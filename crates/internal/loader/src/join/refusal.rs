//! The one dialog a client shows when it stays out of a server: each of the
//! server's mods it lacks, with the server's version and where to get it.

use std::collections::BTreeMap;

use super::Names;
use crate::admission::Need;
use crate::texts;

/// Mods the dialog names one per line; the rest are counted, so the game's
/// popup keeps its button on screen.
const MAX_NAMED: usize = 8;

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
/// assert!(dialog.detail.contains("safes 0.3.0"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    /// The popup's title.
    pub title: String,
    /// The line over the list.
    pub message: String,
    /// One line per mod, then what to do next.
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
/// assert!(dialog.detail.contains("https://github.com/someone/safes/releases"));
/// ```
#[must_use]
pub fn dialog(
    needs: &[Need],
    off: &Off,
    names: &Names,
    sources: &BTreeMap<String, String>,
) -> Dialog {
    let nowhere = texts::fill("join.refused.nowhere", &[]);
    let mut lines: Vec<String> = needs
        .iter()
        .take(MAX_NAMED)
        .map(|need| {
            let name = super::label(names, &need.id);
            let place = sources
                .get(&need.id)
                .and_then(|line| crate::admission::place(line))
                .unwrap_or_else(|| nowhere.clone());
            let mut args = vec![
                ("mod", name.as_str()),
                ("version", need.version.as_str()),
                ("place", place.as_str()),
                ("file", crate::modlist::LIST_PATH),
            ];
            if off.by_player.contains(&need.id) {
                texts::fill("join.refused.mod.off", &args)
            } else if off.this_game.contains(&need.id) {
                texts::fill("join.refused.mod.session", &args)
            } else if let Some(yours) = &need.yours {
                args.push(("yours", yours.as_str()));
                texts::fill("join.refused.mod.yours", &args)
            } else {
                texts::fill("join.refused.mod", &args)
            }
        })
        .collect();
    if needs.len() > MAX_NAMED {
        let count = (needs.len() - MAX_NAMED).to_string();
        lines.push(texts::fill("join.refused.more", &[("count", &count)]));
    }
    lines.push(String::new());
    lines.push(texts::fill("join.dialog.next", &[]));
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

    #[test]
    fn each_missing_mod_gets_a_line_with_its_version_and_where_to_get_it() {
        let names = Names::from([("build-camera".to_owned(), "Build camera".to_owned())]);
        let sources = BTreeMap::from([(
            "build-camera".to_owned(),
            "github:someone/camera:build-camera".to_owned(),
        )]);
        let dialog = dialog(
            &[
                need("build-camera", "0.3.0", None),
                need("safes", "0.2.0", Some("0.1.0")),
            ],
            &Off::default(),
            &names,
            &sources,
        );
        let lines: Vec<&str> = dialog.detail.lines().collect();
        assert_eq!(
            lines,
            [
                "Build camera (build-camera) 0.3.0: https://github.com/someone/camera/releases",
                "safes 0.2.0, yours is 0.1.0: ask the server's admin",
                "",
                "Install them, restart the game and join again.",
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
        let dialog = dialog(
            &[need("count", "0.2.0", None), need("safes", "0.3.0", None)],
            &off,
            &Names::new(),
            &BTreeMap::new(),
        );
        let lines: Vec<&str> = dialog.detail.lines().collect();
        assert_eq!(
            lines[..2],
            [
                "count: turned off while the game ran (the log says why); restart the game",
                "safes: you turned it off in mods/enshwick/mods.toml; turn it on and restart the game",
            ]
        );
    }

    #[test]
    fn a_long_list_names_the_first_mods_and_counts_the_rest() {
        let needs: Vec<Need> = (0..20)
            .map(|i| need(&format!("mod-{i:02}"), "1.0.0", None))
            .collect();
        let dialog = dialog(&needs, &Off::default(), &Names::new(), &BTreeMap::new());
        let lines: Vec<&str> = dialog.detail.lines().collect();
        assert_eq!(lines.len(), MAX_NAMED + 3, "{}", dialog.detail);
        assert_eq!(lines[MAX_NAMED], "And 12 more.");
    }
}
