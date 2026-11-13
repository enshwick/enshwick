//! The game's items and the ones mods serve, by the names this client's
//! game shows, for the picker of an action's item argument: a language's
//! list is read once from the client's archive, what its mods serve and
//! their texts, so no name travels from the server.

#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use arc_swap::ArcSwapOption;
use enshwick_core::content::Guid;

/// An item with the name the game shows for it, and that name's lower
/// case, which a search compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    pub(crate) item: Guid,
    pub(crate) name: String,
    lower: String,
}

impl Shown {
    pub(crate) fn new(item: Guid, name: &str) -> Self {
        Self {
            item,
            name: name.to_owned(),
            lower: name.to_lowercase(),
        }
    }
}

/// The items of one language, sorted by name.
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

/// `found` with the item `picked` first when a search left it out, so the
/// list still shows the pick; at most `most`.
pub(crate) fn keep_pick(
    found: &mut Vec<usize>,
    items: &[Shown],
    picked: Option<Guid>,
    most: usize,
) {
    let Some(picked) = picked else { return };
    if found.iter().any(|&at| items[at].item == picked) {
        return;
    }
    if let Some(at) = items.iter().position(|shown| shown.item == picked) {
        found.insert(0, at);
        found.truncate(most);
    }
}

/// `(item, debug name, shown name)` as the picker lists them: sorted by
/// shown name, without empty names, and a name several items share
/// followed by each one's debug name, so the player can tell them apart.
pub(crate) fn listed<'a>(named: impl Iterator<Item = (Guid, &'a str, &'a str)>) -> Vec<Shown> {
    let mut named: Vec<(Shown, &str)> = named
        .filter(|(_, _, shown)| !shown.is_empty())
        .map(|(item, debug, shown)| (Shown::new(item, shown), debug))
        .collect();
    named.sort_unstable_by(|(a, _), (b, _)| (&a.lower, a.item.0).cmp(&(&b.lower, b.item.0)));
    let mut items: Vec<Shown> = Vec::with_capacity(named.len());
    for (at, (shown, debug)) in named.iter().enumerate() {
        let shared =
            |other: Option<&(Shown, &str)>| other.is_some_and(|(o, _)| o.lower == shown.lower);
        if shared(at.checked_sub(1).and_then(|before| named.get(before)))
            || shared(named.get(at + 1))
        {
            items.push(Shown::new(shown.item, &format!("{} ({debug})", shown.name)));
        } else {
            items.push(shown.clone());
        }
    }
    items
}

/// The picker's lists: the one built last, which a frame loads without a
/// lock, and whether a build runs; and the reader of the items mods serve.
#[cfg(windows)]
struct State {
    list: ArcSwapOption<List>,
    building: AtomicBool,
    /// Content read from what mods serve, then the archive, for the name
    /// tags of mods' items, once its facts read.
    served: std::sync::OnceLock<enshwick_core::content::vanilla::Vanilla>,
}

/// How often a build whose archive or texts do not read yet tries again,
/// and how many times, before the next draw of a picker starts it anew.
#[cfg(windows)]
const BUILD_RETRY: std::time::Duration = std::time::Duration::from_secs(1);
#[cfg(windows)]
const BUILD_TRIES: u32 = 30;

#[cfg(windows)]
static STATE: State = State {
    list: ArcSwapOption::const_empty(),
    building: AtomicBool::new(false),
    served: std::sync::OnceLock::new(),
};

/// Every item the game shows a name for in `language`, vanilla and the
/// ones mods serve, sorted by name. `None` while the list is built on a thread of its own, which
/// reads every item's resource from the archive, and while the archive or
/// its texts do not read, as on a server, which a later call tries again.
#[cfg(windows)]
pub(crate) fn items(language: enshwick_core::i18n::Language) -> Option<Arc<List>> {
    let mods = mods_mark();
    if let Some(list) = STATE
        .list
        .load_full()
        .filter(|list| list.language == language.0 && list.mods == mods)
    {
        return Some(list);
    }
    if !STATE.building.swap(true, Ordering::AcqRel) {
        let built = std::thread::Builder::new()
            .name("enshwick-item-names".into())
            .spawn(move || {
                for _ in 0..BUILD_TRIES {
                    if let Some(list) = build(language, mods) {
                        STATE.list.store(Some(Arc::new(list)));
                        break;
                    }
                    std::thread::sleep(BUILD_RETRY);
                }
                STATE.building.store(false, Ordering::Release);
            });
        if built.is_err() {
            STATE.building.store(false, Ordering::Release);
        }
    }
    None
}

/// The list of `language`, read from the client's archive and texts: the
/// game's items, and the items mods serve with the names their texts give.
#[cfg(windows)]
fn build(language: enshwick_core::i18n::Language, mark: u64) -> Option<List> {
    let vanilla = vanilla()?.items().ok()?;
    let mods = mod_items(language);
    let items = listed(
        vanilla
            .iter()
            .filter_map(|item| {
                let text = crate::engine::game_texts::text(item.name_tag?.fnv1a(), language.0)?;
                Some((item.guid, item.name.as_str(), text))
            })
            .chain(
                mods.iter()
                    .map(|(item, id, text)| (*item, id.as_str(), *text)),
            ),
    );
    (!items.is_empty()).then_some(List {
        language: language.0,
        mods: mark,
        items,
    })
}

/// A mark of which mods are on and whether their texts are sealed, which
/// changes when the items mods serve or their names can: a client turns
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

/// The items mods that are on serve, each with the content id that names
/// it and its name in `language` from its mod's texts; an item that keeps
/// a vanilla item's name or has no text in any language is left out.
#[cfg(windows)]
fn mod_items(language: enshwick_core::i18n::Language) -> Vec<(Guid, String, &'static str)> {
    let Some(served) = served() else {
        return Vec::new();
    };
    crate::content::served::new_keys_of_type(crate::engine::catalogue::types::ITEM_INFO)
        .into_iter()
        .filter_map(|key| {
            let item = Guid(key.guid);
            let text =
                crate::content::served::mod_text(served.name_tag(item)?.fnv1a(), language.0)?;
            Some((item, crate::base::ids::content_id(item)?, text))
        })
        .collect()
}

/// Content read from what mods serve, then the client's archive; a try
/// before its facts read leaves it for the next build to make.
#[cfg(windows)]
fn served() -> Option<&'static enshwick_core::content::vanilla::Vanilla> {
    if let Some(served) = STATE.served.get() {
        return Some(served);
    }
    let loader = crate::engine::loader::loader()?;
    let made = enshwick_core::content::vanilla::Vanilla::new(
        &crate::engine::catalogue::Reflected(&loader.reflection),
        |key| {
            crate::content::served::served_now(key)
                .ok()
                .or_else(|| crate::engine::archive::game()?.resource(&key))
        },
    )
    .ok()?;
    Some(STATE.served.get_or_init(|| made))
}

/// The client's vanilla content, read through its own archive.
#[cfg(windows)]
fn vanilla() -> Option<&'static enshwick_core::content::vanilla::Vanilla> {
    crate::engine::archive::vanilla(crate::engine::loader::loader()?).ok()
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

        let torch = items[3].item;
        let mut found = matching(&items, "axe", 3);
        keep_pick(&mut found, &items, Some(torch), 3);
        assert_eq!(found, [3, 2, 0], "the pick first, the last match out");
        keep_pick(&mut found, &items, Some(torch), 3);
        assert_eq!(found, [3, 2, 0], "listed once");
        keep_pick(&mut found, &items, Some(Guid([9; 16])), 3);
        assert_eq!(
            found,
            [3, 2, 0],
            "an item without a shown name adds nothing"
        );
    }

    #[test]
    fn items_that_share_a_shown_name_are_listed_with_their_debug_names() {
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
