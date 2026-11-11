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
    let line = crate::texts::fill("join.off", &[("names", &listed(off))]);
    if taken.is_empty() {
        return Some(line);
    }
    let owners = crate::texts::fill("join.taken", &[("owners", &listed(taken))]);
    Some(format!("{line} {owners}"))
}

/// `names` joined by commas up to [`NAMES_BYTES`], the rest counted as
/// `+<n>`; the first is shown whole whatever its length.
fn listed(names: &[String]) -> String {
    let mut listed = String::new();
    let mut shown = 0;
    for name in names {
        let gap = if listed.is_empty() { 0 } else { 2 };
        if shown > 0 && listed.len() + gap + name.len() > NAMES_BYTES {
            break;
        }
        if gap > 0 {
            listed.push_str(", ");
        }
        listed.push_str(name);
        shown += 1;
    }
    if shown < names.len() {
        let _more = write!(listed, " +{}", names.len() - shown);
    }
    listed
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

/// The mods that are off, by name: the loaded ones that turned off, then
/// the ones skipped at start but a second copy of a mod that runs; the one
/// set the main menu line and the chat line at world load share.
#[cfg(windows)]
#[must_use]
pub(crate) fn mods_off(loader: &crate::host::Loader) -> Vec<String> {
    off_records(loader)
        .map(crate::host::ModRecord::label)
        .chain(skipped_off(loader).map(|skipped| skipped.label.clone()))
        .collect()
}

/// How many [`mods_off`] names, without building the names, for a line
/// drawn every frame.
#[cfg(windows)]
#[must_use]
pub(crate) fn mods_off_count(loader: &crate::host::Loader) -> usize {
    off_records(loader).count() + skipped_off(loader).count()
}

/// How many mods the main menu line counts: the loaded ones and those
/// skipped at start that count as off.
#[cfg(windows)]
#[must_use]
pub(crate) fn mods_counted(loader: &crate::host::Loader) -> usize {
    loader.mods.len() + skipped_off(loader).count()
}

#[cfg(windows)]
fn off_records(loader: &crate::host::Loader) -> impl Iterator<Item = &crate::host::ModRecord> + '_ {
    use std::sync::atomic::Ordering;

    loader
        .mods
        .iter()
        .filter(|record| record.off.load(Ordering::Acquire))
}

/// The mods skipped at start that count as off: all but a second copy of a
/// mod that runs.
#[cfg(windows)]
pub(crate) fn skipped_off(
    loader: &crate::host::Loader,
) -> impl Iterator<Item = &crate::resolve::Skipped> + '_ {
    loader
        .skipped
        .iter()
        .filter(|skipped| skipped.counts_off(|id| loader.mods.iter().any(|m| m.id == id)))
}

/// What this server has off, by name: mods off, each mod's features off
/// (`<mod>: <feature>`), the loader's parts off and the features this game
/// build lacks.
#[cfg(windows)]
#[must_use]
pub(crate) fn names(loader: &crate::host::Loader) -> Vec<String> {
    use std::sync::atomic::Ordering;

    use enshwick_base::Relock;

    let mut names = mods_off(loader);
    for record in &loader.mods {
        if record.off.load(Ordering::Acquire) {
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
        let taken = ["hooks.dll".to_owned(), "other.dll".to_owned()];
        let said = line(&off, &taken).unwrap();
        let named = said.find("chat.receive").unwrap();
        let owners = said.find("hooks.dll, other.dll").unwrap();
        assert!(named < owners, "{said}");
        assert_eq!(line(&[], &taken), None, "nothing off, nothing to say");
    }

    #[test]
    fn owners_past_the_budget_are_counted() {
        let off = ["chat.receive".to_owned()];
        let taken: Vec<String> = (0..40)
            .map(|i| format!("hooking-library-{i}.dll"))
            .collect();
        let said = line(&off, &taken).unwrap();
        assert!(said.contains("hooking-library-0.dll"), "{said}");
        assert!(!said.contains("hooking-library-39.dll"), "{said}");
        let shown = said.matches("hooking-library-").count();
        assert!(said.contains(&format!(" +{}", 40 - shown)), "{said}");
    }
}
