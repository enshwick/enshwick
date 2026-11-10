//! Names close to a mistyped one, for replies that say what may be meant.

/// Up to `most` of `names` close to `typed`, the closest first and equally
/// close ones by name, whatever the case: a name that starts
/// with what was typed, then one that holds it, then one within a third of
/// the typed length in edits, one edit at least.
pub(crate) fn near<'a>(
    typed: &str,
    names: impl IntoIterator<Item = &'a str>,
    most: usize,
) -> Vec<&'a str> {
    near_lowered(
        typed,
        names.into_iter().map(|name| (name, name.to_lowercase())),
        most,
    )
}

/// [`near`] over names paired with their lower case, made once by the
/// caller; a name listed twice takes one place.
pub(crate) fn near_lowered<'a, L: AsRef<str>>(
    typed: &str,
    names: impl IntoIterator<Item = (&'a str, L)>,
    most: usize,
) -> Vec<&'a str> {
    let needle = typed.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let typed_len = needle.chars().count();
    let slack = (typed_len / 3).max(1);
    let mut scored: Vec<(usize, &str)> = names
        .into_iter()
        .filter_map(|(name, lower)| {
            let lower = lower.as_ref();
            let score = if lower.starts_with(&needle) {
                0
            } else if lower.contains(&needle) {
                1
            } else {
                // The edits are at least the length difference, which costs nothing to know.
                if typed_len.abs_diff(lower.chars().count()) > slack {
                    return None;
                }
                let edits = strsim::levenshtein(&needle, lower);
                if edits > slack {
                    return None;
                }
                1 + edits
            };
            Some((score, name))
        })
        .collect();
    scored.sort_unstable();
    scored.dedup();
    scored
        .into_iter()
        .take(most)
        .map(|(_, name)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_names_start_with_hold_or_almost_spell_what_was_typed() {
        let names = ["online", "kick", "ban", "unban", "banlist", "time"];
        assert_eq!(near("ban", names, 3), ["ban", "banlist", "unban"]);
        assert_eq!(near("BAN", names, 1), ["ban"], "whatever the case");
        assert_eq!(near("kik", names, 3), ["kick"]);
        assert_eq!(near("onlnie", names, 3), ["online"], "two edits in six");
        assert_eq!(near("tame", names, 3), ["time"]);
        assert_eq!(
            near("ban", ["bam", "unban"], 3),
            ["unban", "bam"],
            "a name that holds it before one an edit away"
        );
        assert_eq!(near("dance", names, 3), Vec::<&str>::new());
        assert_eq!(near("", names, 3), Vec::<&str>::new());
        assert_eq!(near("xx", names, 3), Vec::<&str>::new(), "one edit at most");
        assert_eq!(
            near("плaнер", ["Планер", "Плащ"], 3),
            ["Планер"],
            "characters, not bytes, in any script"
        );
    }

    #[test]
    fn a_long_typed_name_over_many_names_is_answered_without_measuring_each_one() {
        let names: Vec<String> = (0..30_000).map(|n| format!("Item name {n}")).collect();
        let typed = "й".repeat(1_000);
        let started = std::time::Instant::now();
        assert!(near(&typed, names.iter().map(String::as_str), 3).is_empty());
        assert!(
            // Measuring each one took 7.7 s in a debug build; skipping them a few ms.
            started.elapsed() < std::time::Duration::from_secs(3),
            "names far longer or shorter than the typed one are never measured: {:?}",
            started.elapsed()
        );
        assert_eq!(
            near("onlinee", ["online"], 3),
            ["online"],
            "one char longer"
        );
    }
}
