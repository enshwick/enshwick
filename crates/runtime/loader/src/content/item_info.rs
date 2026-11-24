//! An item's static data as the game keeps it. The inventory reads it from
//! the item registry it finds and names its lookup here when it seals, so
//! the rates and the world's transactions read items without naming it.

use std::sync::OnceLock;

use enshwick_core::abi::ItemTypeInfo;

use crate::engine::loader::Loader;
use crate::engine::world::ServerAccess;

/// Why an item's static data could not be read; every reason is the item
/// missing to its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ItemInfoError {
    /// The build lacks the item lookup.
    #[error("this build lacks the item lookup")]
    Unread,
    /// The loader knows no item registry yet.
    #[error("no item registry yet")]
    NoRegistry,
    /// No world runs.
    #[error("no world runs")]
    NoWorld,
    /// The game has no such item.
    #[error("the game has no such item")]
    NoItem,
}

/// How the inventory reads an item's static data.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lookup {
    /// The game's `ItemInfo` of an item, which lives as long as the item
    /// registry, so while the access is held; `None` for an item the game
    /// does not have.
    pub(crate) at:
        for<'a> fn(&Loader, &ServerAccess<'a>, u32) -> Result<Option<*const u8>, ItemInfoError>,
    /// The static data of an item, once a world runs.
    pub(crate) info: fn(&Loader, u32) -> Result<ItemTypeInfo, ItemInfoError>,
}

/// What the item lookup keeps for the process.
struct State {
    /// The inventory's lookup, once it sealed.
    lookup: OnceLock<Lookup>,
}

static STATE: State = State {
    lookup: OnceLock::new(),
};

/// Names `lookup` as how items are read; the first one named stays.
pub(crate) fn on_lookup(lookup: Lookup) {
    let _first = STATE.lookup.set(lookup);
}

/// The game's static data of `item`, its `ItemInfo`, while `access` is held;
/// `None` for an item the game does not have.
///
/// # Errors
///
/// [`ItemInfoError`] while no lookup is named, the loader knows no item
/// registry yet, or on a build without the lookup.
pub(crate) fn item_info_at(
    loader: &Loader,
    access: &ServerAccess<'_>,
    item: u32,
) -> Result<Option<*const u8>, ItemInfoError> {
    (STATE.lookup.get().ok_or(ItemInfoError::Unread)?.at)(loader, access, item)
}

/// The static data of `item`, once a world runs.
///
/// # Errors
///
/// [`ItemInfoError`] as [`item_info_at`] gives it, without a world and for an
/// item the game does not have.
pub(crate) fn item_info(loader: &Loader, item: u32) -> Result<ItemTypeInfo, ItemInfoError> {
    (STATE.lookup.get().ok_or(ItemInfoError::Unread)?.info)(loader, item)
}
