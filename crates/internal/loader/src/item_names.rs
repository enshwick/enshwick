//! The game's items by the names this client's game shows, for the picker
//! of an action's item argument: a language's list is read once from the
//! client's archive and texts, so no name travels from the server.

#[cfg(windows)]
use std::sync::{Arc, OnceLock};

#[cfg(windows)]
use arc_swap::ArcSwapOption;
use enshwick::content::Guid;
#[cfg(windows)]
use enshwick::content::vanilla::Vanilla;

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

/// The list read last; a frame loads it without a lock.
#[cfg(windows)]
static LIST: ArcSwapOption<List> = ArcSwapOption::const_empty();

/// Every vanilla item the game shows a name for in `language`, sorted by
/// name; `None` while the archive or its texts do not read, as on a
/// server, which a later call tries again.
// shortcut: vanilla items only; a mod's own items join once the loader lists the items it serves with their name tags.
#[cfg(windows)]
pub(crate) fn items(language: enshwick::i18n::Language) -> Option<Arc<List>> {
    if let Some(list) = LIST.load_full().filter(|list| list.language == language.0) {
        return Some(list);
    }
    let mut items: Vec<Shown> = vanilla()?
        .items()
        .ok()?
        .iter()
        .filter_map(|item| {
            let text = crate::game_texts::text(item.name_tag?.fnv1a(), language.0)?;
            (!text.is_empty()).then(|| Shown::new(item.guid, text))
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    items.sort_unstable_by(|a, b| (&a.lower, a.item.0).cmp(&(&b.lower, b.item.0)));
    let list = Arc::new(List {
        language: language.0,
        items,
    });
    LIST.store(Some(Arc::clone(&list)));
    Some(list)
}

/// The client's vanilla content, read through its own archive.
#[cfg(windows)]
fn vanilla() -> Option<&'static Vanilla> {
    static VANILLA: OnceLock<Option<Vanilla>> = OnceLock::new();
    VANILLA
        .get_or_init(|| {
            let loader = crate::host::loader()?;
            Vanilla::new(&crate::catalogue::Reflected(&loader.reflection), |key| {
                crate::archive::game()?.resource(&key)
            })
            .ok()
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
