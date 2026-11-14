//! The checks many entries make before their call into the domain, each
//! written once.

use enshwick_core::abi::{Side, Status};

use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;

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
    open(loader.delivery.get().is_some())
}

/// [`Status::MISSING`] off the client, which alone has what the entry
/// serves.
///
/// # Errors
///
/// [`Status::MISSING`] on a server.
pub(crate) fn on_client(loader: &Loader) -> Result<(), Status> {
    if loader.side == Side::CLIENT {
        Ok(())
    } else {
        Err(Status::MISSING)
    }
}
