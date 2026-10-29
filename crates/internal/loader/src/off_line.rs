//! The chat line a player gets on joining a server where something is off:
//! the mods, their features and the loader's parts the server turned off,
//! by name. The reasons stay in the server's log and status summary.

use std::fmt::Write as _;

/// The most bytes of names the line carries; the rest is counted.
const NAMES_BYTES: usize = 200;

/// The line naming `off`, `None` when nothing is off. Names past
/// [`NAMES_BYTES`] are counted as `+<n>`.
#[must_use]
pub(crate) fn line(off: &[String]) -> Option<String> {
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
    Some(crate::texts::fill("join.off", &[("names", &names)]))
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
        assert_eq!(line(&[]), None);
    }

    #[test]
    fn the_line_names_what_is_off() {
        let off = ["Better Altars".to_owned(), "map markers".to_owned()];
        let line = line(&off).unwrap();
        assert!(line.contains("Better Altars, map markers"), "{line}");
    }

    #[test]
    fn names_past_the_budget_are_counted() {
        let off: Vec<String> = (0..40).map(|i| format!("feature number {i}")).collect();
        let line = line(&off).unwrap();
        assert!(line.contains("feature number 0, "), "{line}");
        assert!(!line.contains("feature number 39"), "{line}");
        let shown = line.matches("feature number").count();
        assert!(line.contains(&format!(" +{}", 40 - shown)), "{line}");
    }

    #[test]
    fn one_long_name_is_shown_whole() {
        let off = ["x".repeat(NAMES_BYTES + 10)];
        let line = line(&off).unwrap();
        assert!(line.contains(&off[0]), "{line}");
        assert!(!line.contains(" +"), "{line}");
    }
}
