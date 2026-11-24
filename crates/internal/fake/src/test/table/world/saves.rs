//! The fake's world saves.

use crate::test::table::base::args;
use crate::test::table::entries;
use crate::test::table::with;
use enshwick_core::abi::{SaveInfo, Status};

/// The fake world's save.
#[derive(Debug, Clone, Default)]
pub(crate) struct Saves {
    /// What `save_info` reports, `None` before the world's first save.
    pub(crate) info: Option<SaveInfo>,
    /// Whether the mod holds the world's saves.
    pub(crate) held: bool,
    /// Whether something outside the mod refuses the world's saves: another
    /// mod's hold or the game's own block.
    pub(crate) refused: bool,
    /// Whether the mod heard the shutdown begin: a hold asked for since is
    /// not taken, as the loader does.
    pub(crate) shut: bool,
    /// The entities the mod keeps out of the save, in the order it asked.
    pub(crate) unsaved: Vec<u32>,
}

entries! {
    pub(crate) fn set_saved(
        me as Caller,
        entity: enshwick_core::Entity,
        saved: u8,
    ) -> Status {
        if entity == enshwick_core::Entity(0) {
            return Status::BAD_ARGS;
        }
        if with(false, |s| super::entity_calls_off(s)) {
            return Status::UNAVAILABLE;
        }
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        with(Status::MISSING, |s| {
            let spawned_at = s
                .world
                .spawns
                .iter()
                .zip(&s.world.spawn_steps)
                .find(|((id, _), _)| *id == entity.0)
                .map(|(_, &at)| at);
            let placed = s
                .world
                .edits
                .transactions
                .iter()
                .any(|t| t.created.contains(&entity));
            match spawned_at {
                Some(at) if at >= s.base.ticks.step => return Status::MISSING,
                Some(_) => {}
                None if placed => {}
                None => {
                    let vanilla = s.host.content.templates.contains_key(&entity.0)
                        || s.host.world.placed.iter().any(|(_, p)| p.entity == entity)
                        || s.host.world.props.iter().any(|p| p.entity == entity);
                    return if vanilla {
                        Status::REFUSED
                    } else {
                        Status::MISSING
                    };
                }
            }
            s.world.save.unsaved.retain(|&e| e != entity.0);
            if saved == 0 {
                s.world.save.unsaved.push(entity.0);
            }
            Status::OK
        })
    }

    pub(crate) fn save_info(me as Caller, out: *mut SaveInfo as args::HeadedOut) -> Status {
        let Some(info) = with(None, |s| s.world.save.info) else {
            return Status::MISSING;
        };
        crate::test::table::put_headed(out, info)
    }
}

/// Lets go of the world's save holds, as the loader does when a hosted game
/// ends or shuts down: no mod holds saves, and the shutdown is still to come.
pub(crate) const fn end_saves(s: &mut crate::test::table::State) {
    s.world.save.held = false;
    s.world.save.refused = false;
    s.world.save.shut = false;
}

entries! {
    pub(crate) fn hold_saves(me as Mod, held: u8 as args::YesNo) -> Status {
        if held > 1 {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if !(s.world.save.shut && held == 1) {
                s.world.save.held = held == 1;
            }
            Status::OK
        })
    }
}
