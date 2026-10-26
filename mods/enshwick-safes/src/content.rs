//! The Safe as game content: an item cloned from the wooden chest that places
//! the same vanilla chest, and a recipe beside the chest's.

use enshwick::content::items::NewItem;
use enshwick::content::recipes::NewRecipe;
use enshwick::content::{Guid, Id};
use enshwick::migrations::Migrations;

/// The vanilla wooden chest item.
pub const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// The Safe item.
pub const SAFE: Id = Id::new("safes:safe");
/// The Safe's recipe.
pub const SAFE_RECIPE: Id = Id::new("safes:safe.recipe");
/// The Safe's name text.
pub const SAFE_NAME: Id = Id::new("safes:safe.name");
/// The Safe's description text.
pub const SAFE_DESCRIPTION: Id = Id::new("safes:safe.description");
/// The GUID the Safe had before content ids; stacks saved with it stay.
const OLD_SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");

/// Every migration of the mod's content, oldest first.
#[must_use]
pub fn migrations() -> Migrations {
    Migrations::new().add("0.5.0", |m| m.rename_content(OLD_SAFE, SAFE.guid()))
}

/// Whether `item` is a Safe, under its id or one it had before.
#[must_use]
pub fn is_safe(item: u32) -> bool {
    enshwick::migrations::current_id(item) == SAFE.id()
}

/// The mod's texts, per language.
pub const TEXTS: &str = include_str!("texts.toml");

/// The Safe item.
#[must_use]
pub fn item() -> NewItem {
    NewItem {
        name: Some(SAFE_NAME.guid()),
        description: Some(SAFE_DESCRIPTION.guid()),
        ..NewItem::new(SAFE, CHEST)
    }
}

/// The Safe's recipe: the chest's, its ingredients times `cost`.
#[must_use]
pub const fn recipe(cost: u32) -> NewRecipe {
    NewRecipe {
        id: SAFE_RECIPE,
        like: CHEST,
        output: SAFE.guid(),
        output_count: 1,
        cost,
        workshop: None,
    }
}

#[cfg(test)]
mod tests {
    use enshwick::i18n::{Language, Texts};

    use super::*;

    #[test]
    fn the_texts_parse_and_cover_name_and_description() {
        let texts = Texts::parse(TEXTS).unwrap();
        for key in ["safe.name", "safe.description"] {
            assert!(texts.get(key, Language::FALLBACK).is_some(), "{key}");
            assert!(texts.get(key, Language::UK_UA).is_some(), "{key}");
        }
    }

    #[test]
    fn the_safe_is_a_chest_with_its_own_identity() {
        assert_ne!(SAFE.id(), CHEST.id());
        assert_eq!(item().like, CHEST);
        assert_eq!(recipe(3).cost, 3);
        assert_eq!(recipe(3).output, SAFE.guid());
    }
}
