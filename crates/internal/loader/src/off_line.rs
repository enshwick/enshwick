//! The chat line a player gets on joining a server where something is off:
//! the mods, their features and the loader's parts the server turned off,
//! by name. The reasons stay in the server's log and status summary.

use std::fmt::Write as _;

/// The most bytes of names the line carries; the rest is counted.
const NAMES_BYTES: usize = 200;

/// The line naming `off`, `None` when nothing is off. Names past
/// [`NAMES_BYTES`] are counted as `+<n>`; `taken` names who took game code
/// the hooks needed, in a sentence of its own.
#[must_use]
pub(crate) fn line(off: &[String], taken: &[String]) -> Option<String> {
    if off.is_empty() {
        return None;
    }
    let mut names = String::new();
    let mut shown = 0;
    for name in off {
        let gap = if names.is_empty() { 0 } else { 2 };
        if shown > 0 && names.len() + gap + name.len() > NAMES_BYTES {
            break;
        }
        if gap > 0 {
            names.push_str(", ");
        }
        names.push_str(name);
        shown += 1;
    }
    if shown < off.len() {
        let _more = write!(names, " +{}", off.len() - shown);
    }
    let line = crate::texts::fill("join.off", &[("names", &names)]);
    if taken.is_empty() {
        return Some(line);
    }
    let owners = crate::texts::fill("join.taken", &[("owners", &taken.join(", "))]);
    Some(format!("{line} {owners}"))
}

/// Who took game code a hook needed, each once: a module by its file name,
/// else the words for something other than Enshwick.
#[cfg(windows)]
#[must_use]
pub(crate) fn takers() -> Vec<String> {
    crate::shipped::takers()
        .into_iter()
        .map(|taken| match taken {
            crate::shipped::Taken::By(module) => module,
            crate::shipped::Taken::Unknown => crate::texts::text("join.taken.unknown").to_owned(),
        })
        .collect()
}

/// What this server has off, by name: mods off, each mod's features off
/// (`<mod>: <feature>`), the loader's parts off and the features this game
/// build lacks.
#[cfg(windows)]
#[must_use]
pub(crate) fn names(loader: &crate::host::Loader) -> Vec<String> {
    use std::sync::atomic::Ordering;

    use crate::locks::Relock;

    let mut names = Vec::new();
    for record in &loader.mods {
        if record.off.load(Ordering::Acquire) {
            names.push(record.label());
            continue;
        }
        let features = record.features.relock();
        names.extend(
            features
                .iter()
                .filter(|f| f.off.is_some())
                .map(|f| format!("{}: {}", record.label(), f.name)),
        );
    }
    names.extend(crate::diag::parts_off().into_iter().map(|p| p.part));
    let lacking = loader.build.lacking(crate::TABLE_IDS);
    if !lacking.is_empty() {
        names.push(lacking.to_owned());
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_off_sends_no_line() {
        assert_eq!(line(&[], &[]), None);
    }

    #[test]
    fn the_line_names_what_is_off() {
        let off = ["Better Altars".to_owned(), "map markers".to_owned()];
        let line = line(&off, &[]).unwrap();
        assert!(line.contains("Better Altars, map markers"), "{line}");
    }

    #[test]
    fn names_past_the_budget_are_counted() {
        let off: Vec<String> = (0..40).map(|i| format!("feature number {i}")).collect();
        let line = line(&off, &[]).unwrap();
        assert!(line.contains("feature number 0, "), "{line}");
        assert!(!line.contains("feature number 39"), "{line}");
        let shown = line.matches("feature number").count();
        assert!(line.contains(&format!(" +{}", 40 - shown)), "{line}");
    }

    #[test]
    fn one_long_name_is_shown_whole() {
        let off = ["x".repeat(NAMES_BYTES + 10)];
        let line = line(&off, &[]).unwrap();
        assert!(line.contains(&off[0]), "{line}");
        assert!(!line.contains(" +"), "{line}");
    }

    #[test]
    fn who_took_game_code_follows_what_is_off() {
        let off = ["chat.receive".to_owned()];
        let taken = ["kiln-core.dll".to_owned(), "other.dll".to_owned()];
        let line = line(&off, &taken).unwrap();
        let named = line.find("chat.receive").unwrap();
        let owners = line.find("kiln-core.dll, other.dll").unwrap();
        assert!(named < owners, "{line}");
        assert_eq!(line(&[], &taken), None, "nothing off, nothing to say");
    }
}
