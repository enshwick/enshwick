//! What the fake loader of `enshwick-fake` reads inside this crate; only
//! the `fake` feature, which `enshwick-core`'s turns on, builds it.

#[cfg(feature = "unstable")]
use core::ffi::c_void;

pub use crate::__internal::{as_loader, as_pack, refresh_levels};
pub use crate::__private::{EXPERIENCE_SOURCES, MOD_OFF, copy_out, ids};
#[cfg(feature = "unstable")]
use crate::abi::CallFn;
use crate::abi::PointFn;
pub use crate::fresh::renew;
use crate::points::{Point, Verdict};
#[cfg(feature = "unstable")]
use crate::services::Call;

/// The handler and its user pointer that serve the call `C` with
/// `handler`, as `Load::serve_call` registers them; the pointer stays valid
/// for the process.
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
/// leaked `Box<F>`, as `Load::extend_point` attaches it.
#[must_use]
pub fn link_runner<P, F>() -> PointFn
where
    P: Point,
    F: Fn(&P, &P::Output) -> Verdict<P::Output>,
{
    crate::points::run_link::<P, F>
}

// The two below serve the API's own tests that need the fake, which run
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
