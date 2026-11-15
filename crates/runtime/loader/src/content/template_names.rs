//! The game's entity templates and the ones mods serve, by the names the
//! client's game data gives them, for the picker of an action's template
//! argument: read once from the client's archive and what its mods serve,
//! so no name travels from the server.

#[cfg(windows)]
use std::sync::Arc;

use enshwick_core::content::Guid;

#[cfg(windows)]
use crate::content::names::{Cache, List};
use crate::content::names::{Shown, listed};

#[cfg(windows)]
static TEMPLATES: Cache = Cache::new();

/// Every template of the client's game and of the mods on, sorted by name;
/// `None` while the list is built on a thread of its own, which reads every
/// template's resource from the archive.
#[cfg(windows)]
pub(crate) fn templates(language: enshwick_core::i18n::Language) -> Option<Arc<List>> {
    TEMPLATES.current("template names", language, build)
}

/// The list of every vanilla template by its name and every template mods
/// serve by its content id; the names hold in every language.
#[cfg(windows)]
fn build(language: enshwick_core::i18n::Language, mods: u64) -> Option<List> {
    let loader = crate::engine::loader::loader()?;
    let vanilla = crate::engine::archive::vanilla(loader)
        .ok()?
        .templates()
        .ok()?;
    let served =
        crate::content::served::new_keys_of_type(crate::engine::catalogue::types::TEMPLATE)
            .into_iter()
            .filter_map(|key| Some((key.guid, crate::base::ids::content_id(key.guid)?)));
    let items = named(
        vanilla
            .into_iter()
            .map(|template| (template.guid, template.name))
            .chain(served),
    );
    (!items.is_empty()).then_some(List {
        language: language.0,
        mods,
        items,
    })
}

/// `(template, name)` as the picker lists them: sorted by name, and a name
/// several templates share followed by each one's GUID, so the player can
/// tell them apart.
fn named(templates: impl Iterator<Item = (Guid, String)>) -> Vec<Shown> {
    let named: Vec<(Guid, String, String)> = templates
        .map(|(template, name)| (template, template.to_string(), name))
        .collect();
    listed(
        named
            .iter()
            .map(|(template, guid, name)| (*template, guid.as_str(), name.as_str())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_that_share_a_name_are_listed_with_their_guids() {
        let wolf = Guid([2; 16]);
        let shown: Vec<String> = named(
            [
                (Guid([1; 16]), "Enemy_Wildbeast_Wolf".to_owned()),
                (wolf, "Enemy_Wildbeast_Wolf".to_owned()),
                (Guid([3; 16]), "Animal_Baby_T3_Turkey".to_owned()),
            ]
            .into_iter(),
        )
        .into_iter()
        .map(|shown| shown.name)
        .collect();
        assert_eq!(
            shown,
            [
                "Animal_Baby_T3_Turkey".to_owned(),
                format!("Enemy_Wildbeast_Wolf ({})", Guid([1; 16])),
                format!("Enemy_Wildbeast_Wolf ({wolf})"),
            ]
        );
    }
}
