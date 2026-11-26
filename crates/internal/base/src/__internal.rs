//! What the crates above the base and the loader call and a mod must not.
//! Neither `enshwick-core` nor the facade re-exports this module, so a mod,
//! which depends on `enshwick` alone, cannot name it; a method on a type the
//! facade shows would be in its reach, hidden or not.

use std::sync::atomic::Ordering;

use crate::__private::{MAX_LEVEL, PACK_LEVELS, tagged_level};
use crate::abi::{HostApi, ModHandle};
use crate::ecs::ComponentType;
use crate::game_events::CraftRecipeEvent;
use crate::{Entity, Host, RecipeId};

mod builds;
pub mod inventory_queue;
pub mod renames;
pub mod served_content;
pub mod waypoints;

pub use crate::game_events::notify::NotifyLayout;
pub use builds::{
    Build, BuildResourceError, Built, FirstBuild, Held, KeepBase, PatchFailure, Patches,
    ServedTable, Unbuilt, View, build, build_first, patch_runs, read_built, read_served, rebuild,
    rebuild_in, rebuild_one, rebuild_readers, rekeys_another,
};

/// Whether the scheduler counts time only as a fake host's steps move it,
/// which it does once a fake host is installed in the process; the loader's
/// tests check that theirs is the wall clock.
#[cfg(feature = "test")]
#[must_use]
pub fn fake_clock() -> bool {
    crate::schedule::fake_clock()
}

/// Makes the calling thread's atomic file writes wait about 30 ms per step
/// for a held file when `briefly`, not a second, and a second again when
/// not.
///
/// For the game thread while a world runs: a stall there freezes the game,
/// and the caller keeps what a failed write held for its next write. Its
/// writes at the shutdown, which no later save follows, wait the full time.
pub fn wait_briefly_on_this_thread(briefly: bool) {
    crate::files::wait_briefly(briefly);
}

/// Runs `f` with the calling thread's atomic file writes waiting about
/// 30 ms per step for a held file, then gives the thread its own wait back.
///
/// For one write on a thread that must not stall, such as a client's main
/// thread, whose caller keeps the text of a write refused for a held file,
/// as [`held`] tells, and writes it again elsewhere.
pub fn briefly<T>(f: impl FnOnce() -> T) -> T {
    crate::files::briefly(f)
}

/// Whether `error`, from an atomic file write, says another handle holds
/// the file for now, as a virus scanner or an indexer briefly does; Windows
/// also refuses access so to a file that stays read-only.
#[must_use]
pub fn held(error: &std::io::Error) -> bool {
    crate::files::busy(error)
}

/// Runs `step` again for up to about a second while another handle holds
/// its file.
///
/// The wait is the full one whatever the calling thread's: for a one-off
/// step, such as renaming a world's folders, whose failure costs more than
/// the stall.
///
/// # Errors
///
/// `step`'s error once it is not refused for access, or the tries run out.
pub fn while_busy<T>(step: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    crate::files::while_busy_fully(step)
}

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
/// the tick that runs it; for core's tests, as the base runs the queues
/// itself.
#[cfg(any(test, feature = "fake"))]
#[must_use]
pub fn scheduler(me: ModHandle) -> (crate::schedule::Scheduler, bool) {
    crate::schedule::Scheduler::get(me)
}

/// Runs the tasks of `scheduler` that are due on a pass in `game`; for
/// core's tests.
#[cfg(any(test, feature = "fake"))]
pub fn run_due(scheduler: crate::schedule::Scheduler, game: u64) {
    scheduler.run_due(game);
}

/// Sends the place of each poisoned lock that [`crate::Relock`] and its kin
/// take to `sink` instead of a mod's log, for a crate with a log of its own;
/// the first call wins.
pub fn report_poison_to(sink: fn(&std::panic::Location<'_>)) {
    crate::locks::report_to(sink);
}

/// A pointer to `entity`'s component of type `ty` on the server's game
/// thread, valid until the game thread returns to the game; `None` when the
/// entity has none or the loader cannot read it now.
#[must_use]
pub fn component(
    ty: ComponentType,
    host: Host,
    entity: Entity,
) -> Option<*const core::ffi::c_void> {
    ty.read(host, entity).ok().flatten()
}

/// [`component`], with the loader's status when it cannot read now (off the
/// game thread, no world), for reads that answer it as an error.
///
/// # Errors
///
/// The loader's status, never [`Status::OK`](crate::abi::Status::OK) or
/// [`Status::MISSING`](crate::abi::Status::MISSING), which is `Ok(None)`.
pub fn component_status(
    ty: ComponentType,
    host: Host,
    entity: Entity,
) -> Result<Option<*const core::ffi::c_void>, crate::abi::Status> {
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

/// Makes the table `api` with the handle `me` this process's host, for
/// the loader's own copy of the API, which runs data packs.
///
/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
pub unsafe fn install_host(api: *const HostApi, me: ModHandle) {
    // SAFETY: `Host::new` needs `api` null or a table readable for its
    // header's size that outlives the process; this function's `# Safety`
    // bullets guarantee both.
    if let Some(host) = unsafe { Host::new(api, me) } {
        MAX_LEVEL.store(host.install().log_level().0, Ordering::Relaxed);
    }
}

/// Runs `call`, the loader's call into code of the data pack `me`, with
/// [`host`](crate::host()) answering the pack on this thread, so the pack's
/// logs, settings reads and faults go out under its own handle.
///
/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
pub unsafe fn as_pack<R>(api: *const HostApi, me: ModHandle, call: impl FnOnce() -> R) -> R {
    // SAFETY: `Host::new` needs `api` null or a table readable for its
    // header's size that outlives the process; this function's `# Safety`
    // bullets guarantee both.
    match unsafe { Host::new(api, me) } {
        Some(host) => host.scope(call),
        None => call(),
    }
}

/// Runs `call`, the loader's call into its own code or a mod DLL's, as the
/// process's host, also inside a data pack's call.
///
/// A panic or a line there is then not the pack's.
pub fn as_loader<R>(call: impl FnOnce() -> R) -> R {
    crate::host::running_as(None, call)
}

/// Rereads the levels this copy of the API keeps from the loader: the
/// process host's, and in the loader's own copy every data pack's.
///
/// A mod's copy runs it on each
/// [`Event::LOADER_SETTINGS_CHANGED`](crate::abi::Event::LOADER_SETTINGS_CHANGED);
/// the loader's own copy subscribes to nothing, so the loader calls it before
/// it raises that event.
pub fn refresh_levels() {
    let Some(own) = as_loader(crate::host) else {
        return;
    };
    MAX_LEVEL.store(own.log_level().0, Ordering::Relaxed);
    for (pack, kept) in PACK_LEVELS.made() {
        let mut level = crate::abi::Level::INFO;
        let _ = (own.api().log_level)(pack, &raw mut level);
        kept.store(tagged_level(level), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use super::*;

    #[test]
    fn run_due_runs_the_due_tasks_of_the_scheduler_it_is_handed() {
        let (scheduler, subscribe) = scheduler(ModHandle(61));
        assert!(subscribe);
        let runs = Arc::new(AtomicU32::new(0));
        let count = Arc::clone(&runs);
        scheduler.every(Duration::ZERO, move || {
            count.fetch_add(1, Ordering::Relaxed);
            ControlFlow::Continue(())
        });
        run_due(scheduler, 1);
        run_due(scheduler, 1);
        assert_eq!(runs.load(Ordering::Relaxed), 2);
    }
}
