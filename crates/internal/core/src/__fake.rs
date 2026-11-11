//! What the fake loader of `enshwick-fake` reads inside this crate, and
//! the one check it hands down; only the `fake` feature, which only that
//! crate turns on, builds it.

#[cfg(feature = "unstable")]
use core::ffi::c_void;
use std::sync::OnceLock;

pub use crate::__private::{
    EXPERIENCE_SOURCES, MOD_OFF, as_loader, as_pack, copy_out, ids, refresh_levels, rules,
};
#[cfg(feature = "unstable")]
use crate::abi::CallFn;
use crate::abi::{HostApi, Manifest, ModHandle, PointFn};
#[cfg(feature = "unstable")]
use crate::actions::ActionTarget;
pub use crate::fresh::renew;
use crate::points::{Point, Verdict};
pub use crate::rates::{Rate, combine, is_valid_factor, scale_whole};
#[cfg(feature = "unstable")]
use crate::services::Call;
use crate::voxels::{EditSource, Voxel};
#[cfg(feature = "unstable")]
use crate::world::HeldState;

/// The fields of an image resource the game's catalogue names, in the order
/// an icon reads them.
pub const IMAGE_FIELDS: [&str; 6] = crate::content::icons::IMAGE_FIELDS;

/// The most markers one mod places on the map.
pub const MARKER_LIMIT: usize = crate::ui::MARKER_LIMIT;

/// The game's screens a mod may extend, by name.
pub const SCREENS: &[&str] = &crate::ui::screens::NAMES;

/// Why the fake refuses the manifest of a mod whose init runs on a table,
/// or `None` when it takes it.
pub type ManifestCheck = fn(*const HostApi, &Manifest) -> Option<String>;

// The fake sits above this crate, so it hands its check down when it
// installs; the first one stays.
static MANIFEST_CHECK: OnceLock<ManifestCheck> = OnceLock::new();

/// Asks `check` about every mod's manifest before its init from now on;
/// the first call wins.
pub fn check_manifests(check: ManifestCheck) {
    MANIFEST_CHECK.get_or_init(|| check);
}

/// Why the fake's check refuses `manifest` on `api`; `None` when it takes
/// it or no fake installed one.
pub(crate) fn refused(api: *const HostApi, manifest: &Manifest) -> Option<String> {
    MANIFEST_CHECK.get().and_then(|check| check(api, manifest))
}

/// The handler and its user pointer that serve the call `C` with
/// `handler`, as [`Load::serve_call`](crate::Load::serve_call) registers
/// them; the pointer stays valid for the process.
#[cfg(feature = "unstable")]
#[must_use = "the handler stays leaked unless the fake registers it"]
pub fn served_call<C, F>(handler: F) -> (CallFn, *mut c_void)
where
    C: Call,
    F: Fn(&C) -> C::Output + Send + Sync + 'static,
{
    let user = Box::into_raw(Box::new(crate::services::Served::new(C::NAME, handler)));
    (crate::services::run_typed::<C, F>, user.cast())
}

/// The function that runs a link of the point `P` whose user pointer is a
/// leaked `Box<F>`, as [`Load::extend_point`](crate::Load::extend_point)
/// attaches it.
#[must_use]
pub fn link_runner<P, F>() -> PointFn
where
    P: Point,
    F: Fn(&P, &P::Output) -> Verdict<P::Output>,
{
    crate::points::run_link::<P, F>
}

/// The action target `raw` names, as the safe layer reads it from the
/// loader; `None` for none.
#[cfg(feature = "unstable")]
#[must_use]
pub fn action_target(raw: &crate::abi::ActionTarget) -> Option<ActionTarget> {
    ActionTarget::from_raw(raw)
}

/// `target` as the safe layer hands it to the loader.
#[cfg(feature = "unstable")]
#[must_use]
pub fn raw_action_target(target: ActionTarget) -> crate::abi::ActionTarget {
    target.to_raw()
}

/// The `TransactionInfo::HOLDS_` bit of `state`.
#[cfg(feature = "unstable")]
#[must_use]
pub fn held_bit(state: HeldState) -> u32 {
    state.bit()
}

/// `voxel` as the loader writes it, and reads it from a mod.
#[must_use]
pub const fn raw_voxel(voxel: Voxel) -> crate::abi::Voxel {
    voxel.to_abi()
}

/// The handle a write of the world's voxels by `source` comes under for the
/// mod `me`, the inverse of how an edit's source is read: the game writes
/// under none, another mod under the handle after `me`.
#[must_use]
pub fn edit_handle(source: EditSource, me: ModHandle) -> ModHandle {
    match source {
        EditSource::Game => ModHandle::NONE,
        EditSource::ThisMod => me,
        EditSource::OtherMod => ModHandle(me.0.wrapping_add(1).max(1)),
    }
}

// The two below serve this crate's own tests that need the fake, which run
// beside the other pack tests in `enshwick`'s `tests/`.

/// Spawns `body` on a thread named `name` that answers
/// [`host`](crate::host()) as the calling thread does.
///
/// # Errors
///
/// When the system cannot start a thread.
pub fn spawn(
    name: &str,
    body: impl FnOnce() + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    crate::host::spawn(name, body)
}

/// The state of the call site `site` for the data pack this thread runs, as
/// a once or every site keeps it; `site` itself outside a pack.
#[must_use]
pub fn for_pack<T: Default + Send + Sync + 'static>(site: &'static T) -> &'static T {
    crate::__private::for_pack(site)
}
