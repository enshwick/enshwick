//! Vanilla items and recipes by the names a player knows: every item and
//! recipe listed with its names, and an item found by the name the game
//! shows for it in any language.

use super::{Guid, NEAR, Vanilla, VanillaError, game};

/// A vanilla item with its names, from [`items`].
///
/// # Examples
///
/// See [`items`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub struct VanillaItem {
    /// The item.
    pub guid: Guid,
    /// Its debug name, as [`item`](super::item) takes it, such as
    /// `Tool_Glider_T1`.
    pub name: String,
    /// The tag of the name the game shows, for
    /// [`i18n::game_text_in`](crate::i18n::game_text_in); `None` for an
    /// item the archive gives no name.
    pub name_tag: Option<Guid>,
}

/// A vanilla recipe with its name and what it makes, from [`recipes`].
///
/// # Examples
///
/// See [`recipes`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub struct VanillaRecipe {
    /// The recipe.
    pub guid: Guid,
    /// Its debug name, as [`recipe`](super::recipe) takes it.
    pub name: String,
    /// The item it makes, whose name the game shows for the recipe; `None`
    /// for a recipe that names none.
    pub output: Option<Guid>,
}

/// Every vanilla item of the running game with its debug name and the tag
/// of the name the game shows, sorted by debug name: for a picker, a list
/// or suggestions.
///
/// # Errors
///
/// [`VanillaError::Unreadable`] outside the game or without the registry.
///
/// # Examples
///
/// ```
/// use enshwick::content::vanilla;
///
/// if let Ok(items) = vanilla::items() {
///     enshwick::info!("the game has {} items", items.len());
/// }
/// ```
pub fn items() -> Result<Vec<VanillaItem>, VanillaError> {
    Ok(game()?
        .item_list()?
        .into_iter()
        .map(|(guid, name)| VanillaItem {
            guid,
            name,
            name_tag: crate::content::items::vanilla_name_tag(guid),
        })
        .collect())
}

/// Every vanilla recipe of the running game with its debug name and the
/// item it makes, sorted by debug name, so a mod acts on all of them at
/// once.
///
/// # Errors
///
/// [`VanillaError::Unreadable`] outside the game or without the registry.
///
/// # Examples
///
/// ```
/// use enshwick::content::vanilla;
///
/// if let Ok(recipes) = vanilla::recipes() {
///     enshwick::info!("the game has {} recipes", recipes.len());
/// }
/// ```
pub fn recipes() -> Result<Vec<VanillaRecipe>, VanillaError> {
    game()?.recipes()
}

/// Each vanilla item with each name the game shows for it, in every
/// language the game ships, kept once they read. On a dedicated server
/// they never do: only a client's archive holds the game's texts.
fn shown_names() -> Result<&'static [(Guid, String)], VanillaError> {
    static SHOWN: crate::fresh::Fresh<Vec<(Guid, String)>> = crate::fresh::Fresh::new();
    if let Some(shown) = SHOWN.get() {
        return Ok(shown);
    }
    let no_texts = VanillaError::Unreadable("texts");
    let host = crate::host().ok_or(VanillaError::Unreadable("data"))?;
    if host.build().side == crate::abi::Side::SERVER {
        return Err(no_texts);
    }
    let mut shown: Vec<(Guid, String)> = Vec::new();
    for item in items()? {
        let Some(tag) = item.name_tag else { continue };
        let from = shown.len();
        for &language in crate::i18n::Language::ALL {
            let Some(text) = host.game_text(tag.fnv1a(), language) else {
                continue;
            };
            if !text.is_empty() && shown[from..].iter().all(|(_, known)| known != text) {
                shown.push((item.guid, text.to_owned()));
            }
        }
    }
    if shown.is_empty() {
        return Err(no_texts);
    }
    Ok(SHOWN.get_or_init(|| shown))
}

/// The vanilla item `name` is among `shown`, each item with a name the
/// game shows for it, whatever the case.
fn by_shown_name(shown: &[(Guid, String)], name: &str) -> Result<Guid, VanillaError> {
    let wanted = name.to_lowercase();
    let mut found: Vec<Guid> = shown
        .iter()
        .filter(|(_, text)| text.to_lowercase() == wanted)
        .map(|(guid, _)| *guid)
        .collect();
    found.sort_unstable_by_key(|guid| guid.0);
    found.dedup();
    match found.as_slice() {
        [one] => Ok(*one),
        [] => {
            let mut texts: Vec<&str> = shown.iter().map(|(_, text)| text.as_str()).collect();
            texts.sort_unstable();
            texts.dedup();
            let near = crate::near::near(name, texts, NEAR);
            Err(VanillaError::Unknown {
                name: name.to_owned(),
                near: near.into_iter().map(str::to_owned).collect(),
            })
        }
        _ => Err(VanillaError::Ambiguous {
            name: name.to_owned(),
            items: found,
        }),
    }
}

/// The vanilla item the game shows as `name`, such as `Glider` or `Планер`,
/// in any language it ships and whatever the case.
///
/// It is what a player or an administrator knows an item by, where
/// [`item`](super::item) takes its debug name. The names come from the game's texts, which only a client's archive
/// holds: the lookup answers on a client, in single player and in a game a
/// player hosts, and not on a dedicated server.
///
/// # Errors
///
/// [`VanillaError::Unknown`] with the names that look like it,
/// [`VanillaError::Ambiguous`] when the game shows several items by that
/// name, and [`VanillaError::Unreadable`] outside the game or where the
/// game's texts do not read, as on a dedicated server.
///
/// # Examples
///
/// ```
/// use enshwick::content::vanilla::{self, VanillaError};
///
/// match vanilla::item_by_display_name("Glider") {
///     Ok(glider) => enshwick::info!("the glider is {glider}"),
///     Err(VanillaError::Ambiguous { items, .. }) => enshwick::info!("{} gliders", items.len()),
///     Err(VanillaError::Unreadable(_)) => {} // outside the game, or a dedicated server
///     Err(other) => enshwick::warn!("{other}"),
/// }
/// ```
pub fn item_by_display_name(name: &str) -> Result<Guid, VanillaError> {
    by_shown_name(shown_names()?, name)
}

impl Vanilla {
    /// Every vanilla item with its debug name, sorted by name, or
    /// [`VanillaError::Unreadable`] without the registry.
    fn item_list(&self) -> Result<Vec<(Guid, String)>, VanillaError> {
        let mut list: Vec<(Guid, String)> = self
            .names()?
            .iter()
            .flat_map(|(name, guids)| guids.iter().map(move |guid| (*guid, name.clone())))
            .collect();
        list.sort_unstable_by(|a, b| (&a.1, a.0.0).cmp(&(&b.1, b.0.0)));
        Ok(list)
    }

    /// Every vanilla recipe with its debug name and the item it makes,
    /// sorted by name.
    ///
    /// # Errors
    ///
    /// [`VanillaError::Unreadable`] without the recipe registry.
    ///
    /// # Examples
    ///
    /// See [`recipes`].
    pub fn recipes(&self) -> Result<Vec<VanillaRecipe>, VanillaError> {
        let recipes = self.recipe_list()?;
        let mut list: Vec<VanillaRecipe> = recipes
            .names
            .iter()
            .flat_map(|(name, guids)| guids.iter().map(move |guid| (*guid, name)))
            .map(|(guid, name)| VanillaRecipe {
                guid,
                name: name.clone(),
                output: recipes
                    .made
                    .iter()
                    .find(|(made, _)| *made == guid)
                    .and_then(|(_, output)| *output),
            })
            .collect();
        list.sort_unstable_by(|a, b| (&a.name, a.guid.0).cmp(&(&b.name, b.guid.0)));
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_is_found_by_a_shown_name_whatever_the_case_or_named_as_unknown_or_ambiguous() {
        let (glider, cape, cloak) = (Guid([1; 16]), Guid([2; 16]), Guid([3; 16]));
        let shown = vec![
            (glider, "Glider".to_owned()),
            (glider, "Планер".to_owned()),
            (cape, "Cape".to_owned()),
            (cloak, "Cape".to_owned()),
            (cape, "Плащ".to_owned()),
        ];
        assert_eq!(by_shown_name(&shown, "glider"), Ok(glider));
        assert_eq!(by_shown_name(&shown, "ПЛАНЕР"), Ok(glider));
        assert_eq!(
            by_shown_name(&shown, "плащ"),
            Ok(cape),
            "one item in that language"
        );
        assert_eq!(
            by_shown_name(&shown, "Cape"),
            Err(VanillaError::Ambiguous {
                name: "Cape".into(),
                items: vec![cape, cloak],
            })
        );
        assert_eq!(
            by_shown_name(&shown, "Glidr"),
            Err(VanillaError::Unknown {
                name: "Glidr".into(),
                near: vec!["Glider".into()],
            })
        );
        assert_eq!(
            by_shown_name(&shown, "cap"),
            Err(VanillaError::Unknown {
                name: "cap".into(),
                near: vec!["Cape".into()],
            }),
            "a name two items share is suggested once"
        );
        let mut crowded = shown.clone();
        crowded.extend((4..9).map(|n| (Guid([n; 16]), "Cape".to_owned())));
        crowded.push((Guid([9; 16]), "Capelin".to_owned()));
        assert_eq!(
            by_shown_name(&crowded, "cap"),
            Err(VanillaError::Unknown {
                name: "cap".into(),
                near: vec!["Cape".into(), "Capelin".into()],
            }),
            "items that share a name take one place among the near names"
        );
    }
}
