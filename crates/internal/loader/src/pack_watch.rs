//! Which packs changed between two looks at their files: a file that came,
//! went or changed marks its pack, so a script added while the game runs
//! counts as a change of its mod.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// The pack indices whose files differ between `before` and `now`, each a
/// list of `(pack index, path, stamp)`, in ascending order.
pub(crate) fn touched<S: PartialEq>(
    before: &[(usize, PathBuf, S)],
    now: &[(usize, PathBuf, S)],
) -> Vec<usize> {
    let seen: BTreeMap<_, _> = before.iter().map(|(i, p, s)| (p, (*i, s))).collect();
    let current: BTreeMap<_, _> = now.iter().map(|(i, p, s)| (p, (*i, s))).collect();
    let mut packs: Vec<usize> = current
        .iter()
        .filter(|(path, now)| seen.get(*path) != Some(*now))
        .map(|(_, (index, _))| *index)
        .chain(
            seen.iter()
                .filter(|(path, _)| !current.contains_key(*path))
                .map(|(_, (index, _))| *index),
        )
        .collect();
    packs.sort_unstable();
    packs.dedup();
    packs
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn a_pack_counts_as_touched_when_a_file_comes_goes_or_changes() {
        let at = |n: u64| Some(n);
        let file = |index: usize, name: &str, seen| (index, PathBuf::from(name), seen);
        let before = vec![
            file(0, "a/main.luau", at(1)),
            file(1, "b/main.luau", at(1)),
            file(2, "c/main.luau", at(1)),
            file(3, "d/loot.toml", None),
        ];
        let mut now = before.clone();
        assert!(touched(&before, &now).is_empty(), "nothing changed");
        now.push(file(0, "a/lib/new.luau", at(2)));
        now.retain(|(_, path, _)| path != Path::new("b/main.luau"));
        now[1].2 = at(3);
        assert_eq!(touched(&before, &now), [0, 1, 2]);
    }
}
