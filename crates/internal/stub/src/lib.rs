//! The POWRPROF.dll proxy that starts the Enshwick loader.
//!
//! Windows loads it from the game's folder. It forwards every POWRPROF.dll
//! export to the system copy, then, outside the loader lock, loads the other
//! proxy DLLs placed in `mods/chain/`, loads `mods/enshwick/enshwick.dll` and
//! calls its `enshwick_start`. Without the loader the game runs vanilla.
//! On the dedicated server the server's own code waits until the loader has
//! armed its hooks.

#![cfg(windows)]

mod chain;
mod gate;
mod note;
mod proxy;

use core::ffi::c_void;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;
use std::path::PathBuf;

use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetProcAddress, LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW,
};
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

/// `enshwick_start(stub_abi, missing_forwards, release)`, exported by the
/// loader; it calls `release` once its hooks are armed.
type Start = unsafe extern "system" fn(
    stub_abi: u32,
    missing_forwards: u32,
    release: extern "system" fn() -> u32,
);

/// The contract between this stub and the loader's `enshwick_start`.
const STUB_ABI: u32 = 2;
/// No forward could be installed at all.
const NO_SYSTEM_COPY: u32 = u32::MAX;

/// Called by Windows when the DLL loads.
///
/// # Safety
///
/// - Called only by the Windows loader, as this DLL's entry point.
/// - `reason` and `reserved` are the values that loader passes for the call.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(
    _module: *mut c_void,
    reason: u32,
    reserved: *mut c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        // The forwards come first and never depend on the loader: the game
        // calls through them whatever happens below.
        let missing = proxy::install().unwrap_or(NO_SYSTEM_COPY);
        // Loaded as an import, before the game's entry point runs.
        if !reserved.is_null() {
            gate::hold();
        }
        // The rest waits until Windows releases the loader lock DllMain holds.
        // A thread that cannot start leaves the game vanilla; it is detached.
        let spawned = std::thread::Builder::new()
            .name("enshwick-loader-stub".into())
            .spawn(move || {
                if let Err(payload) = std::panic::catch_unwind(|| start(missing)) {
                    let message = payload
                        .downcast_ref::<&str>()
                        .copied()
                        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                        .unwrap_or("a panic without a message");
                    note::note(&format!(
                        "the stub panicked: {message}; the game runs without mods"
                    ));
                }
                gate::release();
            });
        if spawned.is_err() {
            gate::release();
        }
    }
    1
}

fn start(missing: u32) {
    if !chain::first_in_process() {
        return;
    }
    let Some(game_dir) = game_dir() else {
        note::note(&format!(
            "the game's folder was not found ({}); the game runs without mods",
            std::io::Error::last_os_error()
        ));
        return;
    };
    chain::load(&game_dir);
    let path = chain::wide(game_dir.join(chain::LOADER).as_os_str());
    // SAFETY: `path` is NUL-terminated; the altered search path lets the
    // loader's own dependencies resolve next to it.
    let module = unsafe {
        LoadLibraryExW(
            path.as_ptr(),
            core::ptr::null_mut(),
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    };
    if module.is_null() {
        note::note(&format!(
            "{} could not be loaded ({}); the game runs without mods",
            chain::LOADER,
            std::io::Error::last_os_error()
        ));
        return;
    }
    // SAFETY: `module` is loaded and the name is NUL-terminated.
    let Some(start) = (unsafe { GetProcAddress(module, c"enshwick_start".as_ptr().cast()) }) else {
        note::note(&format!(
            "{} has no enshwick_start ({}); the game runs without mods. Install the loader again",
            chain::LOADER,
            std::io::Error::last_os_error()
        ));
        return;
    };
    // SAFETY: the loader exports `enshwick_start` with the `Start` signature.
    let start: Start = unsafe { core::mem::transmute(start) };
    // SAFETY: called once, from this thread, outside the loader lock.
    unsafe { start(STUB_ABI, missing, gate::release) };
}

/// The folder of the game's executable.
fn game_dir() -> Option<PathBuf> {
    let mut exe = vec![0u16; 1024];
    #[expect(clippy::cast_possible_truncation, reason = "the buffer is 1024 units")]
    // SAFETY: the buffer and its length match; a null module is the executable.
    let len =
        unsafe { GetModuleFileNameW(core::ptr::null_mut(), exe.as_mut_ptr(), exe.len() as u32) }
            as usize;
    if len == 0 || len >= exe.len() {
        return None;
    }
    exe.truncate(len);
    let exe = PathBuf::from(OsString::from_wide(&exe));
    exe.parent().map(PathBuf::from)
}
