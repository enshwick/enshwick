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
    let needle = typed.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let slack = (needle.chars().count() / 3).max(1);
    let mut scored: Vec<(usize, &str)> = names
        .into_iter()
        .filter_map(|name| {
            let lower = name.to_lowercase();
            let score = if lower.starts_with(&needle) {
                0
            } else if lower.contains(&needle) {
                1
            } else {
                let edits = strsim::levenshtein(&needle, &lower);
                if edits > slack {
                    return None;
                }
                1 + edits
            };
            Some((score, name))
        })
        .collect();
    scored.sort_unstable();
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
        assert_eq!(near("dance", names, 3), Vec::<&str>::new());
        assert_eq!(near("", names, 3), Vec::<&str>::new());
        assert_eq!(near("xx", names, 3), Vec::<&str>::new(), "one edit at most");
        assert_eq!(
            near("плaнер", ["Планер", "Плащ"], 3),
            ["Планер"],
            "characters, not bytes, in any script"
        );
    }
}
