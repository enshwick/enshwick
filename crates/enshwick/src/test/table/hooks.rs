//! The fake's hooks and code patches.

use core::sync::atomic::Ordering;

use crate::abi::{HookSpec, MidHookSpec, ModHandle, Status};
use crate::test::LinkedHook;
use crate::test::table::{put, with};

pub(crate) extern "system" fn hook(_me: ModHandle, spec: *const HookSpec) -> Status {
    // SAFETY: the safe layer passes a valid spec for the call; `as_ref` turns away null.
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let linked = with(Status::MISSING, |s| {
        if s.host.refused_hooks.contains(&spec.target) {
            return Status::REFUSED;
        }
        s.hooks.push(LinkedHook {
            target: spec.target,
            detour: spec.detour as u64,
            priority: spec.priority,
            flags: spec.flags,
        });
        Status::OK
    });
    if linked.is_ok() {
        // The chain is armed at once, and its rest is the target itself.
        // SAFETY: `next` points at the hook's static slot; `as_ref` turns away null.
        if let Some(next) = unsafe { spec.next.as_ref() } {
            next.store(usize::try_from(spec.target).unwrap_or(0), Ordering::Release);
        }
    }
    linked
}

pub(crate) extern "system" fn hook_at(_me: ModHandle, spec: *const MidHookSpec) -> Status {
    // SAFETY: the safe layer passes a valid spec for the call; `as_ref` turns away null.
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.host.refused_hooks.contains(&spec.address) {
            return Status::REFUSED;
        }
        s.mid_hooks
            .push((spec.address, spec.handler, spec.user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn code_range(start: *mut u64, len: *mut u64) -> Status {
    let (address, size) = with((0, 0), |s| s.host.code);
    if address == 0 {
        return Status::MISSING;
    }
    let status = put(start, address);
    if !status.is_ok() {
        return status;
    }
    put(len, size)
}

pub(crate) extern "system" fn patch_code(
    _me: ModHandle,
    address: u64,
    expected: *const u8,
    replacement: *const u8,
    len: usize,
) -> Status {
    if expected.is_null() || replacement.is_null() || len == 0 {
        return Status::BAD_ARGS;
    }
    // SAFETY: `expected` is non-null, checked at the top; the safe layer
    // passes `len` readable bytes there for the call.
    let expected = unsafe { core::slice::from_raw_parts(expected, len) }.to_vec();
    // SAFETY: `replacement` is non-null, checked at the top; the safe layer
    // passes `len` readable bytes there for the call.
    let replacement = unsafe { core::slice::from_raw_parts(replacement, len) }.to_vec();
    with(Status::MISSING, |s| {
        s.code_patches.push((address, expected, replacement));
        Status::OK
    })
}

pub(crate) extern "system" fn workshop_recipes() -> u32 {
    with(512, |s| s.host.workshop_recipes)
}
