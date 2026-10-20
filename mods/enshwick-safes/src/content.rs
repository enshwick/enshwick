//! The Safe as game content: an item cloned from the wooden chest that places
//! the same vanilla chest, and a recipe beside the chest's.

use enshwick_api::content::Guid;
use enshwick_api::content::items::NewItem;
use enshwick_api::content::recipes::NewRecipe;

/// The vanilla wooden chest item.
pub const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// The Safe item; its game id is FNV-1a of these bytes.
pub const SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
/// The Safe's recipe.
pub const SAFE_RECIPE: Guid = Guid::parse("4e9d2c17-8b3a-4f65-a1c2-7d5e3b9f0a48");
/// The Safe's name text.
pub const SAFE_NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
/// The Safe's description text.
pub const SAFE_DESCRIPTION: Guid = Guid::parse("c8d2e3f4-5061-4b72-9c83-d4e5f6071829");

/// The mod's texts, per language.
pub const TEXTS: &str = include_str!("texts.toml");

/// The Safe item.
#[must_use]
pub fn item() -> NewItem {
    NewItem {
        guid: SAFE,
        like: CHEST,
        name: Some(SAFE_NAME),
        description: Some(SAFE_DESCRIPTION),
        debug_name: "Safe".into(),
    }
}

/// The Safe's recipe: the chest's, its ingredients times `cost`.
#[must_use]
pub const fn recipe(cost: u32) -> NewRecipe {
    NewRecipe {
        guid: SAFE_RECIPE,
        like: CHEST,
        output: SAFE,
        output_count: 1,
        cost,
        workshop: None,
    }
}

#[cfg(test)]
mod tests {
    use enshwick_api::i18n::{Language, Texts};

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
    }
}
