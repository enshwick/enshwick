//! The engine's pacing places, where the loader runs mods' handlers: the
//! event loop about to wait on its handles, the server loop picking its wait
//! between passes, and the task system reading its worker caps. Each place is
//! a build table entry; the loader checks the bytes at every place, and the
//! code before a wait, before it hooks it, and leaves out a place that does
//! not check out or that it cannot link.

use core::ffi::c_void;

use enshwick::abi::{
    HookFlags, LoopWaitData, LoopWaitFn, LoopWaitSites, Registers, ServerWaitFn, Side,
};

use crate::host::Loader;
use crate::locks::Relock;
use crate::mid::MidLink;
use crate::mod_index::ModIndex;
use crate::pinned::{self, Symbol};

/// The table ids this part names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::pacing::BACKGROUND_WORKERS.id(),
    pinned::pacing::DEFAULT_WORKERS.id(),
    pinned::pacing::LOOP_WAIT.id(),
    pinned::pacing::MESSAGE_WAIT.id(),
    pinned::pacing::SERVER_WAIT.id(),
];

/// `mov rax, 0x431bde82d7b634db`: the start of a division by 1 000 000.
const DIVIDE: [u8; 10] = [0x48, 0xB8, 0xDB, 0x34, 0xB6, 0xD7, 0x82, 0xDE, 0x1B, 0x43];
/// `imul rbx; sar rdx, 18`: the dividend is `rbx`, the nanoseconds left.
const DIVIDE_RBX: [u8; 7] = [0x48, 0xF7, 0xEB, 0x48, 0xC1, 0xFA, 0x12];
/// How far before a wait's hook the division may start.
const DIVIDE_SPAN: usize = 0x80;

/// Whether `before`, the code ahead of a wait's hook, divides `rbx` by a
/// million, so `rbx` holds the nanoseconds left at the hook (1.0 builds).
fn divides_rbx(before: &[u8]) -> bool {
    before
        .windows(DIVIDE.len())
        .enumerate()
        .any(|(at, window)| {
            window == DIVIDE
                && before[at + DIVIDE.len()..]
                    .windows(DIVIDE_RBX.len())
                    .take(9)
                    .any(|next| next == DIVIDE_RBX)
        })
}

/// A mod's handler with its pointer, kept for the life of the process as a
/// hook's user pointer.
#[derive(Clone, Copy, Debug)]
struct Handler<F> {
    callback: F,
    user: usize,
}

/// The wait in milliseconds the loop clamped into `r9` (the wait on
/// handles) or `r8` (the client's message wait), with the nanoseconds left
/// in `rbx` (1.0 builds).
fn wait_of(ms: u64, rbx: u64) -> LoopWaitData {
    LoopWaitData {
        header: enshwick::abi::Header::of::<LoopWaitData>(),
        ms: u32::try_from(ms & 0xffff_ffff).unwrap_or(u32::MAX),
        reserved: 0,
        left_ns: i64::from_ne_bytes(rbx.to_ne_bytes()),
    }
}

/// Runs a mod's loop wait handler at the wait on handles.
///
/// # Safety
///
/// - `registers` is the hook's saved registers, valid for the call.
/// - `user` is the leaked `Handler<LoopWaitFn>` the hook was added with.
unsafe extern "system" fn on_handles(registers: *mut Registers, user: *mut c_void) {
    // SAFETY: the mid hook passes its saved registers, valid for the call.
    let registers = unsafe { &*registers };
    // SAFETY: the user pointer is the leaked handler, never freed.
    let handler = unsafe { *user.cast::<Handler<LoopWaitFn>>() };
    let wait = wait_of(registers.r9, registers.rbx);
    // SAFETY: the mod's callback with its own pointer; the safe layer
    // catches its panics.
    unsafe {
        (handler.callback)(
            &raw const wait,
            core::ptr::with_exposed_provenance_mut(handler.user),
        );
    }
}

/// Runs a mod's loop wait handler at the client's message wait.
///
/// # Safety
///
/// - `registers` is the hook's saved registers, valid for the call.
/// - `user` is the leaked `Handler<LoopWaitFn>` the hook was added with.
unsafe extern "system" fn on_messages(registers: *mut Registers, user: *mut c_void) {
    // SAFETY: the mid hook passes its saved registers, valid for the call.
    let registers = unsafe { &*registers };
    // SAFETY: the user pointer is the leaked handler, never freed.
    let handler = unsafe { *user.cast::<Handler<LoopWaitFn>>() };
    let wait = wait_of(registers.r8, registers.rbx);
    // SAFETY: the mod's callback with its own pointer; the safe layer
    // catches its panics.
    unsafe {
        (handler.callback)(
            &raw const wait,
            core::ptr::with_exposed_provenance_mut(handler.user),
        );
    }
}

/// Runs a mod's server wait handler on the wait in `r9`, in nanoseconds,
/// which the event loop then takes as its wait (1.0 builds).
///
/// # Safety
///
/// - `registers` is the hook's saved registers, valid and writable for the
///   call.
/// - `user` is the leaked `Handler<ServerWaitFn>` the hook was added with.
unsafe extern "system" fn on_server_wait(registers: *mut Registers, user: *mut c_void) {
    // SAFETY: the mid hook passes its saved registers, valid and writable
    // for the call.
    let registers = unsafe { &mut *registers };
    // SAFETY: the user pointer is the leaked handler, never freed.
    let handler = unsafe { *user.cast::<Handler<ServerWaitFn>>() };
    // SAFETY: the mod's callback with its own pointer; the safe layer
    // catches its panics.
    unsafe {
        (handler.callback)(
            &raw mut registers.r9,
            core::ptr::with_exposed_provenance_mut(handler.user),
        );
    }
}

/// Lowers the worker cap the task system loaded into `rax` to the cap the
/// user pointer holds; a lower cap is one the task system takes (1.0
/// builds).
///
/// # Safety
///
/// - `registers` is the hook's saved registers, valid and writable for the
///   call.
/// - `user` holds the cap as an address.
unsafe extern "system" fn on_workers(registers: *mut Registers, user: *mut c_void) {
    // SAFETY: the mid hook passes its saved registers, valid and writable
    // for the call.
    let registers = unsafe { &mut *registers };
    registers.rax = registers.rax.min(user.addr() as u64);
}

/// The address of `id` on this build when its code is the table's
/// ([`Loader::code_holds`]);
/// `None`, logged when the build has the place, otherwise.
fn verified(loader: &Loader, id: Symbol) -> Option<u64> {
    let at = loader.address(id)?;
    if loader.code_holds(id) {
        return Some(at);
    }
    crate::log!(
        WARN,
        "{id} on {}: the code is not what the build table expects; it is not hooked",
        loader.build.name
    );
    None
}

/// The address of the wait `id` on this build, when its bytes are the
/// table's and the code before it divides `rbx` by a million, so the hook
/// reads the time left.
fn checked_wait(loader: &Loader, id: Symbol) -> Option<u64> {
    let at = verified(loader, id)?;
    let code = crate::code::text_bytes(loader.base)?;
    let end = usize::try_from(at.checked_sub(code.as_ptr().addr() as u64)?).ok()?;
    let checked = code
        .get(end.saturating_sub(DIVIDE_SPAN)..end)
        .is_some_and(divides_rbx);
    if !checked {
        crate::log!(
            WARN,
            "{id} on {}: the code before it does not read the time left; it is not hooked",
            loader.build.name
        );
    }
    checked.then_some(at)
}

/// Links `handler` with `user` for mod `owner` at `address`; whether it is
/// linked. A refused link is logged and leaves nothing behind.
fn link(
    loader: &Loader,
    owner: ModIndex,
    id: Symbol,
    address: u64,
    handler: enshwick::abi::MidHookFn,
    user: usize,
) -> bool {
    let status = loader.mids.relock().add(
        address,
        MidLink {
            owner,
            handler,
            user,
            priority: 0,
            flags: HookFlags::NONE,
        },
    );
    if !status.is_ok() {
        crate::log!(WARN, "{id}: the hook was refused ({})", status.0);
    }
    status.is_ok()
}

/// A mod's handler with its pointer as a hook's user pointer, kept for the
/// life of the process.
fn keep<F>(callback: F, user: *mut c_void) -> usize {
    let handler = Box::leak(Box::new(Handler {
        callback,
        user: user.expose_provenance(),
    }));
    core::ptr::from_mut(handler).expose_provenance()
}

/// Runs `callback` with `user` for mod `owner` at every wait of the event
/// loop this build has and the loader could link; the waits linked.
pub(crate) fn loop_wait(
    loader: &Loader,
    owner: ModIndex,
    callback: LoopWaitFn,
    user: *mut c_void,
) -> LoopWaitSites {
    let handler = keep(callback, user);
    let sites: [(Symbol, enshwick::abi::MidHookFn, LoopWaitSites); 2] = [
        (
            pinned::pacing::LOOP_WAIT,
            on_handles,
            LoopWaitSites::HANDLES,
        ),
        (
            pinned::pacing::MESSAGE_WAIT,
            on_messages,
            LoopWaitSites::MESSAGES,
        ),
    ];
    let mut found = LoopWaitSites::NONE;
    for (id, hook, site) in sites {
        if let Some(at) = checked_wait(loader, id)
            && link(loader, owner, id, at, hook, handler)
        {
            found = LoopWaitSites(found.0 | site.0);
        }
    }
    found
}

/// Runs `callback` with `user` for mod `owner` where the server loop has
/// picked its wait; whether it is linked.
pub(crate) fn server_wait(
    loader: &Loader,
    owner: ModIndex,
    callback: ServerWaitFn,
    user: *mut c_void,
) -> bool {
    let id = pinned::pacing::SERVER_WAIT;
    let Some(at) = verified(loader, id) else {
        return false;
    };
    link(loader, owner, id, at, on_server_wait, keep(callback, user))
}

/// Caps the task groups' workers for mod `owner`, 0 leaving a group alone;
/// whether a cap is linked: only on the dedicated server, which starts its
/// task system after the hooks arm, and where this build has the places.
pub(crate) fn task_workers(
    loader: &Loader,
    owner: ModIndex,
    default: u32,
    background: u32,
) -> bool {
    if loader.side != Side::SERVER {
        return false;
    }
    let mut linked = false;
    for (id, cap) in [
        (pinned::pacing::DEFAULT_WORKERS, default),
        (pinned::pacing::BACKGROUND_WORKERS, background),
    ] {
        if cap == 0 {
            continue;
        }
        if let Some(at) = verified(loader, id) {
            linked |= link(loader, owner, id, at, on_workers, cap as usize);
        }
    }
    linked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_division_of_rbx_by_a_million_is_recognised() {
        let mut code = vec![0x90; 16];
        code.extend(DIVIDE);
        code.extend([0x90; 3]);
        code.extend(DIVIDE_RBX);
        assert!(divides_rbx(&code));
        let mut other = code.clone();
        other[16 + DIVIDE.len() + 3 + 2] = 0xEA;
        assert!(!divides_rbx(&other));
        assert!(!divides_rbx(&code[..20]));
    }

    #[test]
    fn a_wait_reads_its_milliseconds_and_the_nanoseconds_left() {
        let wait = wait_of(0x1_0000_0003, (-5_i64).cast_unsigned());
        assert_eq!((wait.ms, wait.left_ns), (3, -5));
    }
}
