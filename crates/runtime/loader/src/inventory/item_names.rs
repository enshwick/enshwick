//! The game's items and the ones mods serve, by the names this client's
//! game shows, for the picker of an action's item argument: a language's
//! list is read once from the client's archive, what its mods serve and
//! their texts, so no name travels from the server.

use std::sync::Arc;

use enshwick_core::content::Guid;

use crate::content::names::{Cache, List, listed};

static ITEMS: Cache = Cache::new();

/// Content read from what mods serve, then the archive, for the name tags
/// of mods' items, once its facts read.
static SERVED: std::sync::OnceLock<enshwick_core::content::vanilla::Vanilla> =
    std::sync::OnceLock::new();

/// Every item the game shows a name for in `language`, vanilla and the
/// ones mods serve, sorted by name; `None` while the list is built on a
/// thread of its own, which reads every item's resource from the archive.
pub(crate) fn items(language: enshwick_core::i18n::Language) -> Option<Arc<List>> {
    ITEMS.current("item names", language, build)
}

/// The list of `language`, read from the client's archive and texts: the
/// game's items, and the items mods serve with the names their texts give.
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

/// The items mods that are on serve, each with the content id that names
/// it and its name in `language` from its mod's texts; an item that keeps
/// a vanilla item's name or has no text in any language is left out.
fn mod_items(language: enshwick_core::i18n::Language) -> Vec<(Guid, String, &'static str)> {
    let Some(served) = served() else {
        return Vec::new();
    };
    crate::content::served::new_keys_of_type(crate::engine::catalogue::types::ITEM_INFO)
        .into_iter()
        .filter_map(|key| {
            let item = key.guid;
            let text =
                crate::content::served::mod_text(served.name_tag(item)?.fnv1a(), language.0)?;
            Some((item, crate::base::ids::content_id(item)?, text))
        })
        .collect()
}

/// Content read from what mods serve, then the client's archive; a try
/// before its facts read leaves it for the next build to make.
fn served() -> Option<&'static enshwick_core::content::vanilla::Vanilla> {
    if let Some(served) = SERVED.get() {
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
    Some(SERVED.get_or_init(|| made))
}

/// The client's vanilla content, read through its own archive.
fn vanilla() -> Option<&'static enshwick_core::content::vanilla::Vanilla> {
    crate::engine::archive::vanilla(crate::engine::loader::loader()?).ok()
}
