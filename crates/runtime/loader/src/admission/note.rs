//! The note a player whose game lacks the server's mods of side both reads.

use std::time::Duration;

use crate::join::Names;
use crate::texts;

use super::Need;

/// Mods a note names one per line; the rest are counted. With the opening,
/// the loader's line, the count and the kick, a note stays within the 16
/// lines a reply may take.
const MAX_NAMED: usize = 12;

/// Where Enshwick itself comes from: the loader's own constant, never a
/// place a server chose.
const LOADER_PLACE: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

/// The note for a player whose game lacks `needs`: `hello` when the game
/// said which mods it runs, mods named by `names`, and the time to the
/// kick, `leaving`, when the server kicks.
#[must_use]
pub fn note(needs: &[Need], hello: bool, names: &Names, leaving: Option<Duration>) -> String {
    let mut lines = Vec::with_capacity(MAX_NAMED + 4);
    if hello {
        lines.push(texts::fill("join.refused.mods", &[]));
    } else {
        lines.push(texts::fill("join.refused.vanilla", &[]));
        lines.push(texts::fill(
            "join.refused.loader",
            &[("place", LOADER_PLACE)],
        ));
    }
    for need in needs.iter().take(MAX_NAMED) {
        let name =
            enshwick_core::mods::label(names.get(&need.id).map_or("", String::as_str), &need.id);
        let mut args = vec![("mod", name.as_str()), ("version", need.version.as_str())];
        let line = match &need.yours {
            Some(yours) => {
                args.push(("yours", yours.as_str()));
                texts::fill("join.refused.mod.yours", &args)
            }
            None => texts::fill("join.refused.mod", &args),
        };
        lines.push(line);
    }
    if needs.len() > MAX_NAMED {
        let count = (needs.len() - MAX_NAMED).to_string();
        lines.push(texts::fill("join.refused.more", &[("count", &count)]));
    }
    if let Some(leaving) = leaving {
        let seconds = leaving.as_secs().to_string();
        lines.push(texts::fill(
            "join.refused.leaving",
            &[("seconds", &seconds)],
        ));
    }
    lines.join("\n")
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
    fn a_note_names_enshwick_each_mod_and_its_version_and_no_place_of_a_server() {
        let names = Names::from([("you.trading-post".to_owned(), "Trading Post".to_owned())]);
        let needs = [
            need("you.trading-post", "0.3.0", Some("0.2.0")),
            need("zones", "0.1.0", None),
        ];
        let text = note(&needs, false, &names, Some(Duration::from_secs(10)));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(lines[1].contains(LOADER_PLACE), "{text}");
        assert_eq!(lines[2], "Trading Post 0.3.0, yours is 0.2.0", "{text}");
        assert!(
            lines[3].contains("zones") && lines[3].contains("0.1.0"),
            "{text}"
        );
        assert!(lines[4].contains("10"), "{text}");
        let told = note(&needs, true, &names, None);
        assert!(!told.contains(LOADER_PLACE), "{told}");
        assert_eq!(told.lines().count(), 3, "no kick line: {told}");
    }

    #[test]
    fn a_long_note_counts_the_mods_it_does_not_name() {
        let needs: Vec<Need> = (0..40)
            .map(|i| need(&format!("mod-{i:02}"), "1.0.0", None))
            .collect();
        let text = note(&needs, false, &Names::new(), Some(Duration::from_secs(10)));
        assert!(text.lines().count() <= 16, "{text}");
        assert!(text.contains("28"), "{text}");
    }
}
