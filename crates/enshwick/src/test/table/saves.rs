//! The fake's world saves.

use crate::abi::{ModHandle, SaveInfo, Status};
use crate::test::table::with;

/// The fake world's save.
#[derive(Debug, Clone)]
pub(crate) struct Saves {
    /// What `save_info` reports, `None` before the world's first save.
    pub(crate) info: Option<SaveInfo>,
    /// Whether the mod holds the world's saves.
    pub(crate) held: bool,
    /// The entities the mod keeps out of the save, in the order it asked.
    pub(crate) unsaved: Vec<u32>,
}

pub(crate) extern "system" fn set_saved(me: ModHandle, entity: crate::Entity, saved: u8) -> Status {
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    if entity == crate::Entity(0) {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        let spawned = s.spawns.iter().any(|(id, _)| *id == entity.0)
            || s.edits
                .transactions
                .iter()
                .any(|t| t.owner == me && t.created.contains(&entity));
        if !spawned {
            return Status::REFUSED;
        }
        s.save.unsaved.retain(|&e| e != entity.0);
        if saved == 0 {
            s.save.unsaved.push(entity.0);
        }
        Status::OK
    })
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
