//! The fake's hooks and code patches.

use core::sync::atomic::Ordering;

use crate::abi::{HookSpec, MidHookSpec, ModHandle, Status};
use crate::rules;
use crate::test::LinkedHook;
use crate::test::table::{Registration, put, with};

pub(crate) extern "system" fn hook(_me: ModHandle, spec: *const HookSpec) -> Status {
    // SAFETY: the safe layer passes a valid spec for the call; `as_ref` turns away null.
    let Some(spec) = (unsafe { spec.as_ref() }).filter(|spec| !spec.next.is_null()) else {
        return Status::BAD_ARGS;
    };
    let linked = with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if !rules::is_complete_hook(spec.target, spec.detour) {
            return Status::BAD_ARGS;
        }
        if s.host.refused_hooks.contains(&spec.target) {
            return Status::REFUSED;
        }
        let slot = spec.next.addr();
        // One mod holds the fake, so every hook a target holds is its own.
        let clash = s.hooks.iter().any(|held| {
            held.target == spec.target && rules::hooks_clash(true, held.flags, spec.flags)
        });
        if clash || s.hook_slots.contains(&slot) {
            return Status::REFUSED;
        }
        s.hooks.push(LinkedHook {
            target: spec.target,
            detour: spec.detour as u64,
            priority: spec.priority,
            flags: spec.flags,
        });
        s.hook_slots.push(slot);
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
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if spec.address == 0 {
            return Status::BAD_ARGS;
        }
        // One mod holds the fake, so a second hook at one address is its own.
        let taken = s
            .mid_hooks
            .iter()
            .any(|&(address, ..)| address == spec.address);
        if taken || s.host.refused_hooks.contains(&spec.address) {
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
    if expected.is_null() || replacement.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        let span = |address: u64, len: usize| (usize::try_from(address).unwrap_or(usize::MAX), len);
        let (start, size) = s.host.code;
        let text = span(start, usize::try_from(size).unwrap_or(0));
        if !rules::patch_fits(span(address, len), text) {
            return Status::BAD_ARGS;
        }
        let over = s.code_patches.iter().any(|(held, bytes, _)| {
            rules::spans_overlap(span(*held, bytes.len()), span(address, len))
        });
        if over {
            return Status::REFUSED;
        }
        // SAFETY: `expected` is non-null, checked at the top; the safe layer
        // passes `len` readable bytes there for the call.
        let expected = unsafe { core::slice::from_raw_parts(expected, len) }.to_vec();
        // SAFETY: `replacement` is non-null, checked at the top; the safe layer
        // passes `len` readable bytes there for the call.
        let replacement = unsafe { core::slice::from_raw_parts(replacement, len) }.to_vec();
        s.code_patches.push((address, expected, replacement));
        Status::OK
    })
}

pub(crate) extern "system" fn workshop_recipes() -> u32 {
    with(512, |s| s.host.workshop_recipes)
}
