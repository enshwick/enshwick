//! Names close to a mistyped one, for replies that say what may be meant.

/// The edit distance between `a` and `b`: the fewest characters inserted,
/// removed or replaced to turn one into the other.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, &ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == cb {
                diagonal
            } else {
                1 + diagonal.min(above).min(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Up to `most` of `names` close to `typed`, the closest first and equally
/// close ones in their given order, whatever the case: a name that starts
/// with what was typed, then one that holds it, then one within a third of
/// the typed length in edits, one edit at least.
pub(crate) fn near<'a>(
    typed: &str,
    names: impl IntoIterator<Item = &'a str>,
    most: usize,
) -> Vec<&'a str> {
    let typed: Vec<char> = typed.to_lowercase().chars().collect();
    if typed.is_empty() {
        return Vec::new();
    }
    let needle: String = typed.iter().collect();
    let slack = (typed.len() / 3).max(1);
    let mut scored: Vec<(usize, &str)> = names
        .into_iter()
        .filter_map(|name| {
            let lower = name.to_lowercase();
            let score = if lower.starts_with(&needle) {
                0
            } else if lower.contains(&needle) {
                1
            } else {
                let edits = distance(&typed, &lower.chars().collect::<Vec<_>>());
                if edits > slack {
                    return None;
                }
                1 + edits
            };
            Some((score, name))
        })
        .collect();
    scored.sort_by_key(|&(score, _)| score);
    scored
        .into_iter()
        .take(most)
        .map(|(_, name)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edits(a: &str, b: &str) -> usize {
        distance(
            &a.chars().collect::<Vec<_>>(),
            &b.chars().collect::<Vec<_>>(),
        )
    }

    #[test]
    fn the_distance_counts_characters_inserted_removed_and_replaced() {
        assert_eq!(edits("kick", "kick"), 0);
        assert_eq!(edits("kik", "kick"), 1);
        assert_eq!(edits("kicks", "kick"), 1);
        assert_eq!(edits("kock", "kick"), 1);
        assert_eq!(edits("", "ban"), 3);
        assert_eq!(edits("ban", ""), 3);
        assert_eq!(edits("планер", "плaнер"), 1, "characters, not bytes");
        assert_eq!(edits("abc", "xyz"), 3);
    }

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
    }
}
