//! The fake's hooks and code patches.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::sync::atomic::Ordering;

use crate::test::LinkedHook;
use crate::test::table::{Registered, get_headed, put, with, with_open};
use enshwick_core::__fake::rules;
use enshwick_core::abi::{HookSpec, MidHookSpec, Status};

entries! {
    pub(crate) fn hook(me as Mod, spec: *const HookSpec as args::Headed) -> Status {
        let Some(spec) = get_headed(spec).filter(|spec| !spec.next.is_null()) else {
            return Status::BAD_ARGS;
        };
        let linked = with_open(|s| {
            if !rules::is_complete_hook(spec.target, spec.detour) {
                return Status::BAD_ARGS;
            }
            if s.host.base.refused_hooks.contains(&spec.target) {
                return Status::REFUSED;
            }
            let slot = spec.next.addr();
            // One mod holds the fake, so every hook a target holds is its own.
            let clash = s.base.hooks.iter().any(|held| {
                held.target == spec.target && rules::hooks_clash(true, held.flags, spec.flags)
            });
            if clash || s.base.hook_slots.contains(&slot) {
                return Status::REFUSED;
            }
            s.base.hooks.push(LinkedHook {
                target: spec.target,
                detour: spec.detour as u64,
                priority: spec.priority,
                flags: spec.flags,
            });
            s.base.hook_slots.push(slot);
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

    pub(crate) fn hook_at(me as Mod, spec: *const MidHookSpec as args::Headed) -> Status {
        let Some(spec) = get_headed(spec) else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            let Some(handler) = spec.handler else {
                return Status::BAD_ARGS;
            };
            if spec.address == 0 {
                return Status::BAD_ARGS;
            }
            // One mod holds the fake, so a second hook at one address is its own.
            let taken = s
                .base
                .mid_hooks
                .iter()
                .any(|&(address, ..)| address == spec.address);
            if taken || s.host.base.refused_hooks.contains(&spec.address) {
                return Status::REFUSED;
            }
            s.base
                .mid_hooks
                .push((spec.address, Registered::new(handler, spec.user, me)));
            Status::OK
        })
    }

    pub(crate) fn code_range(
        me as Caller,
        start: *mut u64 as args::Out,
        len: *mut u32 as args::Out,
    ) -> Status {
        let (address, size) = with((0, 0), |s| s.host.base.code);
        if address == 0 {
            return Status::UNAVAILABLE;
        }
        let status = put(start, address);
        if !status.is_ok() {
            return status;
        }
        put(len, u32::try_from(size).unwrap_or(u32::MAX))
    }

    pub(crate) fn patch_code(
        me as Mod,
        address: u64,
        expected: *const u8,
        replacement: *const u8,
        len: u32,
    ) -> Status {
        let len = len as usize;
        if expected.is_null() || replacement.is_null() {
            return Status::BAD_ARGS;
        }
        with_open(|s| {
            let span = |address: u64, len: usize| (usize::try_from(address).unwrap_or(usize::MAX), len);
            let (start, size) = s.host.base.code;
            let text = span(start, usize::try_from(size).unwrap_or(0));
            if !rules::patch_fits(span(address, len), text) {
                return Status::BAD_ARGS;
            }
            let over = s.base.code_patches.iter().any(|(held, bytes, _)| {
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
            s.base.code_patches.push((address, expected, replacement));
            Status::OK
        })
    }

    pub(crate) fn workshop_recipes(me as Caller, out: *mut u32 as args::Out) -> Status {
        put(out, with(512, |s| s.host.base.workshop_recipes))
    }
}
