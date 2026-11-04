//! The fake's control entries: the fake controls with the `unstable`
//! feature, else entries that have none.

#[cfg(feature = "unstable")]
pub(crate) use crate::test::controls::{control, run_action};

#[cfg(not(feature = "unstable"))]
use crate::abi::{ControlFn, ControlSpec, ModHandle, Status, Str};

/// Without the `unstable` feature the fake has no controls.
#[cfg(not(feature = "unstable"))]
pub(crate) extern "system" fn control(
    _me: ModHandle,
    spec: *const ControlSpec,
    _callback: ControlFn,
    _user: *mut core::ffi::c_void,
) -> Status {
    // SAFETY: the safe layer passes a live `ControlSpec` for the call.
    match unsafe { crate::abi::read_headed(spec, size_of::<ControlSpec>()) } {
        Some(_) => Status::MISSING,
        None => Status::BAD_ARGS,
    }
}

/// Without the `unstable` feature the fake runs no actions.
#[cfg(not(feature = "unstable"))]
pub(crate) extern "system" fn run_action(_me: ModHandle, _action: Str, _args: Str) -> Status {
    Status::MISSING
}
