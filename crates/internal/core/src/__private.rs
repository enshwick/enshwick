//! What the macros' output names: the base's support, and the mod's init
//! and ready, which need [`Load`] and [`Mod`].

use std::sync::atomic::Ordering;

pub use enshwick_content::__private::*;

use crate::abi::{HostApi, ModHandle, Side, Status};
use crate::{Host, Load, Mod};

/// The rules the loader and the fake host refuse a mod's requests by.
pub mod rules {
    pub use enshwick_base::rules::*;

    pub use crate::rates::may_declare_rate;
}

/// `id`, a dependency's content id, once the loader lets this mod name
/// it, for `deps!`.
///
/// # Errors
///
/// [`Off`](crate::Off) when the loader refuses the id, as one the
/// dependency does not export.
pub fn dependency_id(id: crate::content::Id) -> Result<crate::content::Id, crate::Off> {
    enshwick_content::__internal::named(id)
}

/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
pub unsafe fn init<M: Mod>(
    api: *const HostApi,
    me: ModHandle,
    manifested: &Manifested,
    texts: Option<&'static str>,
    slot: &Slot<M>,
) -> Status {
    let Some((manifest, unread)) = manifested.with_built(|built| {
        (
            core::ptr::from_ref::<crate::abi::Manifest>(&built.manifest),
            built.unread.clone(),
        )
    }) else {
        return Status::FAULTED;
    };
    // SAFETY: the built manifest is never freed once built, so it stays
    // valid for the process.
    let manifest = unsafe { &*manifest };
    // SAFETY: `Host::new` needs `api` null or a table readable for its
    // header's size that outlives the process; this function's `# Safety`
    // bullets guarantee both.
    let Some(host) = (unsafe { Host::new(api, me) }) else {
        return Status::BAD_ARGS;
    };
    let host = enshwick_base::__internal::install(host);
    crate::host::log_panics();
    MAX_LEVEL.store(host.log_level().0, Ordering::Relaxed);
    let _watched = host.subscribe(
        crate::abi::Event::LOADER_SETTINGS_CHANGED,
        refresh_level,
        core::ptr::null_mut(),
    );
    if let Some(reason) = unread {
        host.fault(&reason);
        return Status::REFUSED;
    }
    #[cfg(feature = "fake")]
    if let Some(refused) = crate::__fake::refused(api, manifest) {
        host.fault(&refused);
        return Status::REFUSED;
    }
    let outcome =
        crate::host::caught(|| M::init(&mut Load::of(host, manifest.side == Side::BOTH, texts)));
    let _set = INITIALISED.set(());
    match outcome {
        Ok(Ok(state)) => match slot.set(state) {
            Ok(()) => Status::OK,
            Err(state) => {
                // A second init's state, dropped unused: its drop's panic changes nothing.
                let _dropped = crate::host::caught(|| drop(state));
                Status::REFUSED
            }
        },
        Ok(Err(off)) => {
            host.fault(off.to_reason());
            Status::REFUSED
        }
        Err(panicked) => {
            host.fault(&format!("init panicked: {}", panicked.text()));
            Status::FAULTED
        }
    }
}

/// Runs `init` with a [`Load`] for the data pack `me` of `side`, as the
/// loader does for a pack folder.
///
/// A pack of [`Side::BOTH`] declares what players' games read, as a mod of
/// that side does. [`host`](crate::host()) answers the pack for the call,
/// and the process's host stays as it is.
///
/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
pub unsafe fn init_pack(
    api: *const HostApi,
    me: ModHandle,
    side: Side,
    init: impl FnOnce(&mut Load<'_>) -> Result<(), crate::Off>,
) -> Status {
    // SAFETY: `Host::new` needs `api` null or a table readable for its
    // header's size that outlives the process; this function's `# Safety`
    // bullets guarantee both.
    let Some(host) = (unsafe { Host::new(api, me) }) else {
        return Status::BAD_ARGS;
    };
    // A pack that runs again reads its level afresh; the loader refreshes it
    // on a change of its settings (`refresh_levels`).
    forget_pack_level(me);
    let outcome = crate::host::caught(|| {
        enshwick_base::__internal::scope(host, || {
            init(&mut Load::of(host, side == Side::BOTH, None))
        })
    });
    match outcome {
        Ok(Ok(())) => Status::OK,
        Ok(Err(off)) => {
            host.fault(off.to_reason());
            Status::REFUSED
        }
        // `host`, not `fault_with`: the pack's scope ended with the panic.
        Err(panicked) => {
            host.fault(&format!("init panicked: {}", panicked.text()));
            Status::FAULTED
        }
    }
}

pub fn ready<M: Mod>(slot: &'static Slot<M>) {
    let (Some(state), Some(host)) = (slot.get(), crate::host()) else {
        return;
    };
    crate::host::guard("panic in ready", || (), || state.ready(host));
}
