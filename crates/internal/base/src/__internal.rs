//! What the crates above the base call on its types and a mod must not.
//! Neither `enshwick-core` nor the facade re-exports this module, so a mod,
//! which depends on `enshwick` alone, cannot name it; a method on a type the
//! facade shows would be in its reach, hidden or not.

use crate::abi::ModHandle;
use crate::ecs::ComponentType;
use crate::game_events::CraftRecipeEvent;
use crate::schedule::Scheduler;
use crate::{Entity, Host, RecipeId};

/// Makes `host` the answer of [`crate::host`]; the first call wins.
#[must_use]
pub fn install(host: Host) -> Host {
    host.install()
}

/// Runs `call` with `host` as the answer of [`crate::host`] on this thread,
/// so a data pack's code logs, reads and faults as the pack.
pub fn scope<R>(host: Host, call: impl FnOnce() -> R) -> R {
    host.scope(call)
}

/// The scheduler of the mod `me`, and whether the caller is to subscribe
/// the tick that runs it.
#[must_use]
pub fn scheduler(me: ModHandle) -> (Scheduler, bool) {
    Scheduler::get(me)
}

/// Runs the tasks of `scheduler` that are due on a pass in `game`.
pub fn run_due(scheduler: Scheduler, game: u64) {
    scheduler.run_due(game);
}

/// A pointer to `entity`'s component of type `ty` on the server's game
/// thread, valid until the game thread returns to the game; `None` when the
/// entity has none.
#[must_use]
pub fn component(
    ty: ComponentType,
    host: Host,
    entity: Entity,
) -> Option<*const core::ffi::c_void> {
    ty.read(host, entity)
}

/// The event of a craft check, as the craft veto hands it to a filter; the
/// event is `non_exhaustive`, so a mod never builds one.
#[must_use]
pub const fn craft_recipe_event(
    crafting_operator_id: Entity,
    crafting_station_id: Option<Entity>,
    recipe_id: RecipeId,
    recipe_amount: u16,
) -> CraftRecipeEvent {
    CraftRecipeEvent::new(
        crafting_operator_id,
        crafting_station_id,
        recipe_id,
        recipe_amount,
    )
}
