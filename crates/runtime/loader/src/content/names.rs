//! Named content for the pickers of an action's arguments: the search, the
//! sorted list and its cache, which the item and template pickers share.

#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use arc_swap::ArcSwapOption;
use enshwick_core::content::Guid;

/// Content with the name the game shows for it, and that name's lower case,
/// which a search compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    pub(crate) guid: Guid,
    pub(crate) name: String,
    lower: String,
}

impl Shown {
    pub(crate) fn new(guid: Guid, name: &str) -> Self {
        Self {
            guid,
            name: name.to_owned(),
            lower: name.to_lowercase(),
        }
    }
}

/// The named content of one language, sorted by name.
#[derive(Debug)]
pub(crate) struct List {
    /// The game's number of the language.
    pub(crate) language: u32,
    /// Which mods were on and whether their texts were sealed when it was
    /// built, as [`mods_mark`] marks them: a list of other mods is stale.
    pub(crate) mods: u64,
    pub(crate) items: Vec<Shown>,
}

/// The indices of `items` whose name holds `query` whatever the case, at
/// most `most`: those that start with it first, each group in the order of
/// `items`.
pub(crate) fn matching(items: &[Shown], query: &str, most: usize) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let mut starts = Vec::with_capacity(most);
    let mut holds = Vec::new();
    for (index, shown) in items.iter().enumerate() {
        if shown.lower.starts_with(&query) {
            starts.push(index);
            if starts.len() == most {
                break;
            }
        } else if holds.len() < most && shown.lower.contains(&query) {
            holds.push(index);
        }
    }
    starts.extend(holds);
    starts.truncate(most);
    starts
}

/// `found` with the content `picked` first when a search left it out, so
/// the list still shows the pick; at most `most`.
pub(crate) fn keep_pick(
    found: &mut Vec<usize>,
    items: &[Shown],
    picked: Option<Guid>,
    most: usize,
) {
    let Some(picked) = picked else { return };
    if found.iter().any(|&at| items[at].guid == picked) {
        return;
    }
    if let Some(at) = items.iter().position(|shown| shown.guid == picked) {
        found.insert(0, at);
        found.truncate(most);
    }
}

/// `(content, debug name, shown name)` as the picker lists them: sorted by
/// shown name, without empty names, and a name several share followed by
/// each one's debug name, so the player can tell them apart.
pub(crate) fn listed<'a>(named: impl Iterator<Item = (Guid, &'a str, &'a str)>) -> Vec<Shown> {
    let mut named: Vec<(Shown, &str)> = named
        .filter(|(_, _, shown)| !shown.is_empty())
        .map(|(guid, debug, shown)| (Shown::new(guid, shown), debug))
        .collect();
    named.sort_unstable_by(|(a, _), (b, _)| (&a.lower, a.guid.0).cmp(&(&b.lower, b.guid.0)));
    let mut items: Vec<Shown> = Vec::with_capacity(named.len());
    for (at, (shown, debug)) in named.iter().enumerate() {
        let shared =
            |other: Option<&(Shown, &str)>| other.is_some_and(|(o, _)| o.lower == shown.lower);
        if shared(at.checked_sub(1).and_then(|before| named.get(before)))
            || shared(named.get(at + 1))
        {
            items.push(Shown::new(shown.guid, &format!("{} ({debug})", shown.name)));
        } else {
            items.push(shown.clone());
        }
    }
    items
}

/// One picker's list: the one built last, which a frame loads without a
/// lock, and whether a build runs.
#[cfg(windows)]
pub(crate) struct Cache {
    list: ArcSwapOption<List>,
    building: AtomicBool,
}

/// How often a build whose archive or texts do not read yet tries again,
/// and how many times, before the next draw of a picker starts it anew.
#[cfg(windows)]
const BUILD_RETRY: std::time::Duration = std::time::Duration::from_secs(1);
#[cfg(windows)]
const BUILD_TRIES: u32 = 30;

#[cfg(windows)]
impl Cache {
    pub(crate) const fn new() -> Self {
        Self {
            list: ArcSwapOption::const_empty(),
            building: AtomicBool::new(false),
        }
    }

    /// The list of `language` for the mods on now. `None` while `build`
    /// makes it on a thread named `what`, and while it finds nothing to
    /// list, as before the archive reads or on a server, which a later call
    /// tries again.
    pub(crate) fn current(
        &'static self,
        what: &'static str,
        language: enshwick_core::i18n::Language,
        build: fn(enshwick_core::i18n::Language, u64) -> Option<List>,
    ) -> Option<Arc<List>> {
        let mods = mods_mark();
        if let Some(list) = self
            .list
            .load_full()
            .filter(|list| list.language == language.0 && list.mods == mods)
        {
            return Some(list);
        }
        if !self.building.swap(true, Ordering::AcqRel) {
            let started = crate::engine::off_thread::spawn(what, move || {
                for _ in 0..BUILD_TRIES {
                    if let Some(list) = build(language, mods) {
                        self.list.store(Some(Arc::new(list)));
                        break;
                    }
                    std::thread::sleep(BUILD_RETRY);
                }
                self.building.store(false, Ordering::Release);
            });
            if let Err(why) = started {
                // Asked each frame a picker shows, so a thread that does not
                // start is told once.
                crate::log_once!(WARN, "{what}: no thread to build them ({why})");
                self.building.store(false, Ordering::Release);
            }
        }
        None
    }
}

/// A mark of which mods are on and whether their texts are sealed, which
/// changes when the content mods serve or its names can: a client turns
/// mods off and on as it joins worlds. A few atomic reads, for each draw.
#[cfg(windows)]
fn mods_mark() -> u64 {
    let on = crate::engine::loader::loader()
        .into_iter()
        .flat_map(|loader| loader.mods.iter().map(|m| !m.off.load(Ordering::Acquire)));
    mark(on, crate::content::served::mod_texts_sealed())
}

/// [`mods_mark`] of the mods whose on flags `on` lists in order, and of
/// whether their texts are `sealed`.
fn mark(on: impl IntoIterator<Item = bool>, sealed: bool) -> u64 {
    use core::hash::Hasher;
    let mut hash = rustc_hash::FxHasher::default();
    for flag in on.into_iter().chain([sealed]) {
        hash.write_u8(u8::from(flag));
    }
    hash.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_built_for_other_mods_or_before_their_texts_has_another_mark() {
        let built = mark([true, false, true], true);
        assert_eq!(built, mark([true, false, true], true));
        assert_ne!(built, mark([true, true, true], true), "a mod turned on");
        assert_ne!(built, mark([true, false, false], true), "a mod turned off");
        assert_ne!(built, mark([true, false, true], false), "texts not sealed");
        assert_ne!(built, mark([true, false], true), "a mod fewer");
    }

    #[test]
    fn a_search_finds_names_that_start_with_it_first_whatever_the_case() {
        let items: Vec<Shown> = ["Bone Axe", "Iron Axe", "Axe Handle", "Torch", "Wax"]
            .iter()
            .enumerate()
            .map(|(n, name)| Shown::new(Guid([u8::try_from(n).unwrap(); 16]), name))
            .collect();
        assert_eq!(matching(&items, "axe", 8), [2, 0, 1]);
        assert_eq!(matching(&items, " AXE ", 2), [2, 0], "at most two");
        assert_eq!(matching(&items, "", 3), [0, 1, 2], "every name, in order");
        assert!(matching(&items, "shield", 8).is_empty());
        assert_eq!(matching(&items, "ax", 8), [2, 0, 1, 4]);

        let torch = items[3].guid;
        let mut found = matching(&items, "axe", 3);
        keep_pick(&mut found, &items, Some(torch), 3);
        assert_eq!(found, [3, 2, 0], "the pick first, the last match out");
        keep_pick(&mut found, &items, Some(torch), 3);
        assert_eq!(found, [3, 2, 0], "listed once");
        keep_pick(&mut found, &items, Some(Guid([9; 16])), 3);
        assert_eq!(
            found,
            [3, 2, 0],
            "content without a shown name adds nothing"
        );
    }

    #[test]
    fn content_that_shares_a_shown_name_is_listed_with_its_debug_names() {
        let named = [
            (Guid([3; 16]), "Cape_T2", "Cape"),
            (Guid([1; 16]), "Torch", "torch"),
            (Guid([2; 16]), "Cape_T1", "cape"),
            (Guid([4; 16]), "Hidden", ""),
            (Guid([5; 16]), "Axe", "Axe"),
        ];
        let shown: Vec<String> = listed(named.into_iter())
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(shown, ["Axe", "cape (Cape_T1)", "Cape (Cape_T2)", "torch"]);
    }
}
