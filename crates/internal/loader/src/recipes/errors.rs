//! Why recipe files and rules do not read, resolve or take an edit.

use enshwick::content::Guid;

use super::{list, near_list};

/// Why a recipe file is no recipe file.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}: {}", source.message().trim_end())]
pub struct ParseRulesError {
    /// The file's name.
    pub file: String,
    /// The line of the problem, from 1.
    pub line: usize,
    #[source]
    pub(super) source: toml::de::Error,
}

/// Why a rule names no recipe.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveRuleError {
    /// The rule's name is empty.
    #[error("an empty name")]
    Empty,
    /// No recipe has the GUID.
    #[error("no recipe {name}")]
    NoRecipeGuid {
        /// The name as written.
        name: String,
    },
    /// Several recipes have the debug name.
    #[error("{} recipes are named {name:?}: {}; name one by its GUID", guids.len(), list(guids))]
    SeveralRecipes {
        /// The debug name.
        name: String,
        /// The recipes that carry it.
        guids: Vec<Guid>,
    },
    /// No recipe has the debug name.
    #[error("no recipe {name:?}{}", near_list(near))]
    NoRecipe {
        /// The debug name.
        name: String,
        /// The names that look like it.
        near: Vec<String>,
    },
    /// Several items have the debug name.
    #[error("{} items are named {name:?}: {}; name one by its GUID", guids.len(), list(guids))]
    SeveralItems {
        /// The debug name.
        name: String,
        /// The items that carry it.
        guids: Vec<Guid>,
    },
    /// No item has the debug name.
    #[error("no item {name:?}{}", near_list(near))]
    NoItem {
        /// The debug name.
        name: String,
        /// The names that look like it.
        near: Vec<String>,
    },
    /// No recipe makes the item.
    #[error("no recipe makes {name}")]
    NothingMakes {
        /// The item as written.
        name: String,
    },
    /// The item is no workshop's.
    #[error("{name} has no workshop")]
    NoWorkshop {
        /// The item as written.
        name: String,
    },
    /// No recipe is made at the workshop.
    #[error("no recipe is made at {name}")]
    NothingAt {
        /// The workshop as written.
        name: String,
    },
}

/// Why a recipe file was not edited.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EditRulesError {
    /// The file is no TOML.
    #[error("{0}")]
    Toml(#[from] toml_edit::TomlError),
    /// `[disable]` or `[enable]` is no table.
    #[error("[{table}] is no table")]
    NotTable {
        /// The table's name.
        table: &'static str,
    },
    /// A list of the table is no list.
    #[error("{table}.{key} is no list")]
    NotList {
        /// The table's name.
        table: &'static str,
        /// The list's key.
        key: &'static str,
    },
}
