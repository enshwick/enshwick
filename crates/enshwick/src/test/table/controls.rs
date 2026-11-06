//! The fake's control entries: the fake controls with the `unstable`
//! feature, else entries that have none.

#[cfg(feature = "unstable")]
pub(crate) use crate::test::controls::{control, run_action, run_action_at};

#[cfg(not(feature = "unstable"))]
use crate::abi::{ActionTarget, ControlFn, ControlSpec, ModHandle, Status, Str};

/// Without the `unstable` feature the fake runs no actions.
#[cfg(not(feature = "unstable"))]
pub(crate) extern "system" fn run_action_at(
    _me: ModHandle,
    _action: Str,
    _args: Str,
    target: *const ActionTarget,
) -> Status {
    // SAFETY: the safe layer passes null or a live `ActionTarget` for the call.
    let _read = unsafe { crate::abi::read_headed(target, size_of::<ActionTarget>()) };
    Status::MISSING
}

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
