//! The checks many entries make before their call into the domain, each
//! written once.

use enshwick_core::abi::{ResourceKey, Side, Status};
use enshwick_core::content::Guid;

use crate::base::ids::UndeclaredContentError;
use crate::engine::loader::{Loader, ModRecord};
use crate::engine::mod_index::ModIndex;
use crate::host::layer::Mod;

/// [`Status::TOO_LATE`] once a registry is `sealed`, as declarations end.
///
/// # Errors
///
/// [`Status::TOO_LATE`] when `sealed`.
pub(crate) fn open(sealed: bool) -> Result<(), Status> {
    if sealed {
        Err(Status::TOO_LATE)
    } else {
        Ok(())
    }
}

/// [`Status::TOO_LATE`] once mod `index`'s init returned.
///
/// # Errors
///
/// [`Status::TOO_LATE`] after the mod's init.
pub(crate) fn in_init(index: ModIndex) -> Result<(), Status> {
    open(!crate::engine::loader::in_init(index.get()))
}

/// [`Status::TOO_LATE`] once the loader delivered content to the game.
///
/// # Errors
///
/// [`Status::TOO_LATE`] after the delivery.
pub(crate) fn before_delivery(loader: &Loader) -> Result<(), Status> {
    open(loader.before_delivery().is_err())
}

/// Refuses a call that names new content by `guid` unless mod `caller`
/// declared the content id it comes from, so no mod adds content under
/// another's id. Every entry that takes a new content GUID asks it, or
/// [`declared_resource`] or [`declared_or_game`], once its own checks of
/// when and who passed, before it adds anything.
///
/// # Errors
///
/// [`UndeclaredContentError`] for a GUID another mod or no mod declared.
pub(crate) fn declared_content(
    caller: &ModRecord,
    guid: Guid,
) -> Result<(), UndeclaredContentError> {
    crate::base::ids::declared_by(&caller.id, guid)
}

/// As [`declared_content`] for a GUID that may name the game's own content
/// too, as the new side of a rename may: a GUID the game's archive holds
/// passes.
///
/// # Errors
///
/// [`UndeclaredContentError`] for a GUID the game lacks and another mod or
/// no mod declared.
pub(crate) fn declared_or_game(
    caller: &ModRecord,
    guid: Guid,
) -> Result<(), UndeclaredContentError> {
    if crate::engine::archive::game().is_some_and(|archive| archive.holds_guid(guid)) {
        return Ok(());
    }
    declared_content(caller, guid)
}

/// Refuses a GUID a mod other than `caller` declared, as the old side of a
/// rename names: no mod renames another's content away.
///
/// # Errors
///
/// [`UndeclaredContentError::Other`] for a GUID another mod declared.
pub(crate) fn not_another_mods(
    caller: &ModRecord,
    guid: Guid,
) -> Result<(), UndeclaredContentError> {
    match declared_content(caller, guid) {
        Err(other @ UndeclaredContentError::Other { .. }) => Err(other),
        _ => Ok(()),
    }
}

/// As [`declared_content`] for the resource `key` mod `caller` serves from
/// `base`, when the key names new content: a key served from its own GUID
/// changes content, a key the game's archive holds is the game's, and an
/// old GUID of the caller's own renames serves its renamed content.
///
/// # Errors
///
/// [`UndeclaredContentError`] for a new key whose GUID another mod or no
/// mod declared.
pub(crate) fn declared_resource(
    caller: &Mod,
    key: &ResourceKey,
    base: &ResourceKey,
) -> Result<(), UndeclaredContentError> {
    if key.guid == base.guid
        || crate::engine::archive::game().is_some_and(|archive| archive.contains(key))
        || crate::content::content_renames::renames_from(caller.index, key.guid)
    {
        return Ok(());
    }
    declared_content(caller.record, key.guid)
}

/// [`Status::UNAVAILABLE`] off the client, which alone has what the entry
/// serves.
///
/// # Errors
///
/// [`Status::UNAVAILABLE`] on a server.
pub(crate) fn on_client(loader: &Loader) -> Result<(), Status> {
    if loader.side == Side::CLIENT {
        Ok(())
    } else {
        Err(Status::UNAVAILABLE)
    }
}
