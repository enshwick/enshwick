//! Host functions for hooks and code patches.

use crate::mod_index::ModIndex;
use enshwick::abi as api_frame;
use enshwick::abi::{HookSpec, MidHookSpec, ModHandle, Status};

use crate::chains::Link;
use crate::host::memory::{put, slice};
use crate::host::{caller_index, calling, entry, loader};
use crate::locks::Relock;
use crate::mid::MidLink;

pub(crate) extern "system" fn hook(me: ModHandle, spec: *const HookSpec) -> Status {
    entry("hook", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a non-null spec from a mod points at a valid `HookSpec`.
        let Some(spec) = (unsafe { spec.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: `next` is a static inside the mod's DLL, which is never unloaded.
        let Some(next) = (unsafe { spec.next.as_ref::<'static>() }) else {
            return Status::BAD_ARGS;
        };
        // A pack's links are the API's own code in the loader, shared by
        // every pack: they stay when one pack turns off.
        let owner = if loader.mods[index].pack {
            ModIndex::LOADER
        } else {
            index
        };
        let link = Link {
            owner,
            detour: spec.detour,
            next,
            priority: spec.priority,
            flags: spec.flags,
            hop: None,
        };
        let mut chains = loader.chains.relock();
        if crate::reload::reinitialising(index.get()) {
            chains.relink(spec.target, link)
        } else {
            chains.add(spec.target, link)
        }
    })
}

pub(crate) extern "system" fn hook_at(me: ModHandle, spec: *const MidHookSpec) -> Status {
    entry("hook_at", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a non-null spec from a mod points at a valid `MidHookSpec`.
        let Some(spec) = (unsafe { spec.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        let link = MidLink {
            owner: index,
            handler: spec.handler,
            user: spec.user.expose_provenance(),
            priority: spec.priority,
            flags: spec.flags,
        };
        loader.mids.relock().add(spec.address, link)
    })
}

pub(crate) extern "system" fn code_range(start: *mut u64, len: *mut u64) -> Status {
    entry("code_range", || {
        let Some((address, size)) = loader().and_then(|loader| crate::code::text(loader.base))
        else {
            return Status::MISSING;
        };
        // SAFETY: `HostApi::code_range` has the mod pass `start` null or
        // pointing at a writable, aligned `u64` that no other thread touches
        // during the call.
        let status = unsafe { put(start, address as u64) };
        if !status.is_ok() {
            return status;
        }
        // SAFETY: `HostApi::code_range` has the mod pass `len` null or pointing
        // at a writable, aligned `u64` that no other thread touches during the
        // call.
        unsafe { put(len, size as u64) }
    })
}

pub(crate) extern "system" fn patch_code(
    me: ModHandle,
    address: u64,
    expected: *const u8,
    replacement: *const u8,
    len: usize,
) -> Status {
    entry("patch_code", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let (Ok(address), Some(text)) = (usize::try_from(address), crate::code::text(loader.base))
        else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass `len` readable bytes at a non-null
        // `expected`, unchanged for the call; `register` copies them into a box
        // before it returns.
        let expected = unsafe { slice(expected, len) };
        // SAFETY: the ABI has the mod pass `len` readable bytes at a non-null
        // `replacement`, unchanged for the call; `register` copies them into a
        // box before it returns.
        let replacement = unsafe { slice(replacement, len) };
        crate::code::register(
            crate::code::CodePatch {
                owner: index,
                address,
                expected: expected.into(),
                replacement: replacement.into(),
                needs: Box::default(),
            },
            text,
        )
    })
}

pub(crate) extern "system" fn workshop_recipes() -> u32 {
    entry("workshop_recipes", crate::code::workshop_recipes)
}

pub(crate) extern "system" fn loop_wait(
    me: ModHandle,
    callback: api_frame::LoopWaitFn,
    user: *mut core::ffi::c_void,
    found: *mut api_frame::LoopWaitSites,
) -> Status {
    entry("loop_wait", || {
        let (loader, index, _) = match calling(me) {
            Ok(called) => called,
            Err(status) => return status,
        };
        let (status, sites) = crate::pacing::loop_wait(loader, index, callback, user);
        if !status.is_ok() {
            return status;
        }
        // SAFETY: `HostApi::loop_wait` has the mod pass `found` null or
        // pointing at a writable `LoopWaitSites` no other thread touches
        // during the call.
        unsafe { put(found, sites) }
    })
}

pub(crate) extern "system" fn server_wait(
    me: ModHandle,
    callback: api_frame::ServerWaitFn,
    user: *mut core::ffi::c_void,
    found: *mut u32,
) -> Status {
    entry("server_wait", || {
        let (loader, index, _) = match calling(me) {
            Ok(called) => called,
            Err(status) => return status,
        };
        let (status, hooked) = crate::pacing::server_wait(loader, index, callback, user);
        if !status.is_ok() {
            return status;
        }
        // SAFETY: `HostApi::server_wait` has the mod pass `found` null or
        // pointing at a writable `u32` no other thread touches during the
        // call.
        unsafe { put(found, u32::from(hooked)) }
    })
}

pub(crate) extern "system" fn task_workers(
    me: ModHandle,
    default: u32,
    background: u32,
    found: *mut u32,
) -> Status {
    entry("task_workers", || {
        let (loader, index, _) = match calling(me) {
            Ok(called) => called,
            Err(status) => return status,
        };
        let (status, capped) = crate::pacing::task_workers(loader, index, default, background);
        if !status.is_ok() {
            return status;
        }
        // SAFETY: `HostApi::task_workers` has the mod pass `found` null or
        // pointing at a writable `u32` no other thread touches during the
        // call.
        unsafe { put(found, u32::from(capped)) }
    })
}
