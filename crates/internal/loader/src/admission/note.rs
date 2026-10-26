//! The note a player whose game lacks the server's join mods reads.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::join::Names;
use crate::texts;

use super::Need;

/// Mods a note names one per line; the rest are counted. With the opening,
/// the loader's line, the count and the kick, a note stays within the 16
/// lines a reply may take.
const MAX_NAMED: usize = 12;

/// Where Enshwick itself comes from.
const LOADER_PLACE: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

/// The longest page a server may advertise, in bytes.
const MAX_PAGE: usize = 127;

/// Where players get a mod whose homepage is `page`: the page itself when it
/// is an `https://` or `http://` address of printable characters without
/// spaces, as a peer may send anything.
#[must_use]
pub fn place(page: &str) -> Option<String> {
    let rest = page
        .strip_prefix("https://")
        .or_else(|| page.strip_prefix("http://"))?;
    let plain =
        !rest.is_empty() && page.len() <= MAX_PAGE && page.chars().all(|c| c.is_ascii_graphic());
    plain.then(|| page.to_owned())
}

/// The note for a player whose game lacks `needs`: `hello` when the game
/// said which mods it runs, mods named by `names` with the homepages `pages`
/// by id, and the time to the kick, `leaving`, when the server kicks.
#[must_use]
pub fn note(
    needs: &[Need],
    hello: bool,
    names: &Names,
    pages: &BTreeMap<String, String>,
    leaving: Option<Duration>,
) -> String {
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
    let nowhere = texts::fill("join.refused.nowhere", &[]);
    for need in needs.iter().take(MAX_NAMED) {
        let name = enshwick::mods::label(names.get(&need.id).map_or("", String::as_str), &need.id);
        let place = pages
            .get(&need.id)
            .and_then(|line| place(line))
            .unwrap_or_else(|| nowhere.clone());
        let mut args = vec![
            ("mod", name.as_str()),
            ("version", need.version.as_str()),
            ("place", place.as_str()),
        ];
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
    fn places_are_links_players_can_open() {
        assert_eq!(
            place("https://github.com/someone/trading-post").as_deref(),
            Some("https://github.com/someone/trading-post")
        );
        assert!(place("http://example.com/trading-post").is_some());
        for bad in [
            "",
            "https://",
            "ftp://example.com",
            "github.com/someone/trading-post",
            "https://exa mple.com",
            "https://example.com/\nInstall",
        ] {
            assert_eq!(place(bad), None, "{bad}");
        }
        assert_eq!(place(&format!("https://{}", "x".repeat(200))), None);
    }

    #[test]
    fn a_note_names_enshwick_each_mod_its_version_and_where_to_get_it() {
        let names = Names::from([("you.trading-post".to_owned(), "Trading Post".to_owned())]);
        let pages = BTreeMap::from([(
            "you.trading-post".to_owned(),
            "https://github.com/someone/trading-post".to_owned(),
        )]);
        let needs = [
            need("you.trading-post", "0.3.0", Some("0.2.0")),
            need("zones", "0.1.0", None),
        ];
        let text = note(&needs, false, &names, &pages, Some(Duration::from_secs(10)));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(lines[1].contains(LOADER_PLACE), "{text}");
        assert!(
            lines[2].contains("Trading Post 0.3.0")
                && !lines[2].contains("(you.trading-post)")
                && lines[2].contains("0.2.0")
                && lines[2].contains("https://github.com/someone/trading-post"),
            "{text}"
        );
        assert!(
            lines[3].contains("zones") && lines[3].contains("0.1.0"),
            "{text}"
        );
        assert!(lines[4].contains("10"), "{text}");
        let told = note(&needs, true, &names, &pages, None);
        assert!(!told.contains(LOADER_PLACE), "{told}");
        assert_eq!(told.lines().count(), 3, "no kick line: {told}");
    }

    #[test]
    fn a_long_note_counts_the_mods_it_does_not_name() {
        let needs: Vec<Need> = (0..40)
            .map(|i| need(&format!("mod-{i:02}"), "1.0.0", None))
            .collect();
        let text = note(
            &needs,
            false,
            &Names::new(),
            &BTreeMap::new(),
            Some(Duration::from_secs(10)),
        );
        assert!(text.lines().count() <= 16, "{text}");
        assert!(text.contains("28"), "{text}");
    }
}
