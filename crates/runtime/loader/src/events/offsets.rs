//! The server layout the events links read, in groups a link needs alone.

use super::MAX_SLOTS;
use crate::engine::loader::Loader;
use crate::engine::pinned;
use crate::engine::pinned::Constant;

/// Offsets into the game's server, from the build table, each group only
/// where the build has all of it: a link needs only its own.
#[derive(Debug, Clone, Copy)]
pub(super) struct Offsets {
    /// The server instance entry's state byte, for the state link.
    pub(super) entry_state: Option<usize>,
    /// For the step's joins and the player removal.
    pub(super) slots: Option<Slots>,
    /// For the step's capture of the server's world.
    pub(super) world: Option<WorldAt>,
    /// For the building links.
    pub(super) building: Option<Building>,
}

/// The server's player slots.
#[derive(Debug, Clone, Copy)]
pub(super) struct Slots {
    pub(super) at: usize,
    pub(super) size: usize,
    pub(super) count: usize,
    pub(super) state: usize,
    pub(super) entity: usize,
    pub(super) key: usize,
}

/// Where the server keeps its world and the world its entities.
#[derive(Debug, Clone, Copy)]
pub(super) struct WorldAt {
    pub(super) server_world: usize,
    pub(super) entities: usize,
}

/// The building calls' context and pickup target.
#[derive(Debug, Clone, Copy)]
pub(super) struct Building {
    pub(super) ctx_player: usize,
    pub(super) ctx_replay: usize,
    /// The pickup target's entity id and mode byte.
    pub(super) pickup_entity: usize,
    pub(super) pickup_mode: usize,
}

/// Each group of [`Offsets`] on this build, or the first constant it lacks.
#[derive(Debug, Clone, Copy)]
pub(super) struct Found {
    pub(super) entry_state: Result<usize, Constant>,
    pub(super) slots: Result<Slots, Constant>,
    pub(super) world: Result<WorldAt, Constant>,
    pub(super) building: Result<Building, Constant>,
}

impl Found {
    /// What the step lacks: the world it captures. Its joins read the slots
    /// too, but only they stop without them, under the player removal's
    /// fuse, which names the slot constant.
    pub(super) fn step(&self) -> Option<Constant> {
        self.world.err()
    }

    /// The groups found, for the links.
    pub(super) fn offsets(&self) -> Offsets {
        Offsets {
            entry_state: self.entry_state.ok(),
            slots: self.slots.ok(),
            world: self.world.ok(),
            building: self.building.ok(),
        }
    }
}

/// The server layout from the build table, each group with the first
/// constant it lacks.
pub(super) fn found(loader: &Loader) -> Found {
    let constant = |name: Constant| {
        loader
            .build
            .constant(name.id())
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(name)
    };
    let slots = || {
        Ok(Slots {
            at: constant(pinned::server::PLAYER_SLOTS)?,
            size: constant(pinned::server::PLAYER_SLOT_SIZE)?,
            count: constant(pinned::server::PLAYER_SLOT_COUNT)?.min(MAX_SLOTS),
            state: constant(pinned::server::PLAYER_SLOT_STATE)?,
            entity: constant(pinned::server::PLAYER_SLOT_ENTITY)?,
            key: constant(pinned::server::PLAYER_SLOT_KEY)?,
        })
    };
    let world = || {
        Ok(WorldAt {
            server_world: constant(pinned::server::WORLD)?,
            entities: constant(pinned::world::ENTITIES)?,
        })
    };
    let building = || {
        Ok(Building {
            ctx_player: constant(pinned::building::CTX_PLAYER)?,
            ctx_replay: constant(pinned::building::CTX_REPLAY)?,
            pickup_entity: constant(pinned::building::PICKUP_ENTITY)?,
            pickup_mode: constant(pinned::building::PICKUP_MODE)?,
        })
    };
    Found {
        entry_state: constant(pinned::server::ENTRY_STATE),
        slots: slots(),
        world: world(),
        building: building(),
    }
}
