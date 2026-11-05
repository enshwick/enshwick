//! The fake's world saves.

use crate::abi::{ModHandle, SaveInfo, Status};
use crate::test::table::with;

/// The fake world's save.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Saves {
    /// What `save_info` reports, `None` before the world's first save.
    pub(crate) info: Option<SaveInfo>,
    /// Whether the mod holds the world's saves.
    pub(crate) held: bool,
}

pub(crate) extern "system" fn save_info(_me: ModHandle, out: *mut SaveInfo) -> Status {
    let Some(info) = with(None, |s| s.save.info) else {
        return Status::MISSING;
    };
    super::put_headed(out, info)
}

pub(crate) extern "system" fn hold_saves(_me: ModHandle, held: u8) -> Status {
    if held > 1 {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        s.save.held = held == 1;
        Status::OK
    })
}
