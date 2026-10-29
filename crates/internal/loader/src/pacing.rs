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
    pinned::pacing::WAIT_DIVIDE.id(),
];

/// Whether the wait at `wait` lies after the division at `divide`, inside
/// the `span` bytes of code from the division on that its table pattern
/// covers, so `rbx` holds the nanoseconds left at the hook (1.0 builds).
fn after_division(divide: u64, span: usize, wait: u64) -> bool {
    u64::try_from(span).is_ok_and(|span| wait > divide && wait - divide < span)
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

/// The address of the wait `id` on this build, when its code and the
/// division before it are the table's and the wait lies in the code the
/// division's pattern spans, so the hook reads the time left.
fn checked_wait(loader: &Loader, id: Symbol) -> Option<u64> {
    let at = verified(loader, id)?;
    let divide = pinned::pacing::WAIT_DIVIDE;
    let span = loader
        .build
        .head(divide.id())
        .and_then(|(pattern, start)| pattern.len().checked_sub(usize::try_from(start).ok()?));
    let checked = verified(loader, divide)
        .zip(span)
        .is_some_and(|(divide, span)| after_division(divide, span, at));
    if !checked {
        crate::log!(
            WARN,
            "{id} on {}: {divide} does not lead to it, so the time left is unknown; it is not hooked",
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
    fn a_wait_counts_only_inside_the_code_after_its_division() {
        assert!(after_division(0x1000, 0x40, 0x1031));
        assert!(after_division(0x1000, 0x40, 0x103f));
        assert!(!after_division(0x1000, 0x40, 0x1040), "past the span");
        assert!(!after_division(0x1000, 0x40, 0x1000), "the division itself");
        assert!(!after_division(0x1000, 0x40, 0xff0), "before it");
    }

    /// In every embedded table, the division's pattern starts at the
    /// division and spans each wait of the event loop the table has.
    #[test]
    fn every_table_s_division_leads_to_each_of_its_waits() {
        for table in crate::builds::load() {
            let resolved = table.resolve();
            let name = resolved.name().to_owned();
            let divide = resolved
                .symbol(pinned::pacing::WAIT_DIVIDE.id())
                .unwrap_or_else(|| panic!("{name}: no division"));
            let (pattern, at) = resolved
                .head(pinned::pacing::WAIT_DIVIDE.id())
                .unwrap_or_else(|| panic!("{name}: the division has no pattern"));
            let span = pattern.len().saturating_sub(usize::try_from(at).unwrap());
            let mut waits = 0;
            for wait in [pinned::pacing::LOOP_WAIT, pinned::pacing::MESSAGE_WAIT] {
                if let Some(address) = resolved.symbol(wait.id()) {
                    assert!(after_division(divide, span, address), "{name}: {wait}");
                    waits += 1;
                }
            }
            assert!(waits > 0, "{name}: no wait");
        }
    }

    /// In each executable `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name, the
    /// division and the waits found by their patterns alone, as on a build
    /// without its own table, still have each wait after the division;
    /// skipped without them.
    #[test]
    fn the_patterns_find_the_division_before_each_wait_in_the_game_s_executables() {
        use enshwick_gamedata::reflect::{Image, Layout};

        let tables = crate::builds::load();
        for (variable, side) in [
            ("ENSH_SERVER_EXE", Side::SERVER),
            ("ENSH_CLIENT_EXE", Side::CLIENT),
        ] {
            let Some(path) = std::env::var_os(variable) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image = Image::parse(&bytes, Layout::File, None).unwrap();
            let text = image.section(".text").unwrap();
            let range = image.range(text);
            let code = image.read(range.start, range.end - range.start).unwrap();
            let found = crate::builds::by_patterns(
                &tables,
                side,
                (code, range.start - image.base()),
                |rva| image.u32(image.base() + rva),
            )
            .unwrap();
            let divide = found
                .symbol(pinned::pacing::WAIT_DIVIDE.id())
                .unwrap_or_else(|| panic!("{variable}: no division"));
            let (pattern, at) = found.head(pinned::pacing::WAIT_DIVIDE.id()).unwrap();
            let span = pattern.len() - usize::try_from(at).unwrap();
            let waits: &[Symbol] = if side == Side::CLIENT {
                &[pinned::pacing::LOOP_WAIT, pinned::pacing::MESSAGE_WAIT]
            } else {
                &[pinned::pacing::LOOP_WAIT]
            };
            for &wait in waits {
                let address = found
                    .symbol(wait.id())
                    .unwrap_or_else(|| panic!("{variable}: no {wait}"));
                assert!(after_division(divide, span, address), "{variable}: {wait}");
            }
        }
    }

    #[test]
    fn a_wait_reads_its_milliseconds_and_the_nanoseconds_left() {
        let wait = wait_of(0x1_0000_0003, (-5_i64).cast_unsigned());
        assert_eq!((wait.ms, wait.left_ns), (3, -5));
    }
}
