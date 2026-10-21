//! Forwards every POWRPROF.dll export to the system copy.
//!
//! Each export is a stub that jumps through `FORWARDS`. `install` fills the
//! table before the game can call anything; an export the system copy lacks
//! (an older Windows, or Wine's builtin) fails in the form its return type
//! has, from Microsoft's documentation of each export: `STATUS_NOT_IMPLEMENTED`
//! for an `NTSTATUS`, `FALSE` with the last error set for a `BOOLEAN`,
//! `E_NOTIMPL` for an `HRESULT`, the unspecified role for the platform role
//! queries, and `ERROR_CALL_NOT_IMPLEMENTED` for the `DWORD` error codes of
//! the rest.

use core::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{
    E_NOTIMPL, ERROR_CALL_NOT_IMPLEMENTED, STATUS_NOT_IMPLEMENTED, SetLastError,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

include!(concat!(env!("OUT_DIR"), "/stubs.rs"));

/// The address each export stub jumps to, in export order. `AtomicUsize` has
/// the layout of `usize`, which the stubs read directly.
pub(crate) static FORWARDS: [AtomicUsize; EXPORTS.len()] =
    [const { AtomicUsize::new(0) }; EXPORTS.len()];

/// How an export reports failure, by its return type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fails {
    /// `DWORD`, a Win32 error code.
    Error,
    /// `NTSTATUS`.
    Status,
    /// `BOOLEAN`, with the reason in the last error.
    False,
    /// `HRESULT`.
    Hresult,
    /// `POWER_PLATFORM_ROLE`: `PlatformRoleUnspecified`.
    Unspecified,
}

const STATUS: &[&str] = &["CallNtPowerInformation", "PowerInformationWithPrivileges"];
const BOOLEAN: &[&str] = &[
    "CanUserWritePwrScheme",
    "DeletePwrScheme",
    "DevicePowerClose",
    "DevicePowerEnumDevices",
    "DevicePowerOpen",
    "EnumPwrSchemes",
    "GetActivePwrScheme",
    "GetCurrentPowerPolicies",
    "GetPwrCapabilities",
    "GetPwrDiskSpindownRange",
    "IsAdminOverrideActive",
    "IsPwrHibernateAllowed",
    "IsPwrShutdownAllowed",
    "IsPwrSuspendAllowed",
    "PowerIsSettingRangeDefined",
    "ReadGlobalPwrPolicy",
    "ReadProcessorPwrScheme",
    "ReadPwrScheme",
    "SetActivePwrScheme",
    "SetSuspendState",
    "ValidatePowerPolicies",
    "WriteGlobalPwrPolicy",
    "WriteProcessorPwrScheme",
    "WritePwrScheme",
];
const HRESULT: &[&str] = &[
    "PowerRegisterForEffectivePowerModeNotifications",
    "PowerUnregisterFromEffectivePowerModeNotifications",
];
const ROLE: &[&str] = &["PowerDeterminePlatformRole", "PowerDeterminePlatformRoleEx"];

/// How export `name`, NUL-terminated as in `EXPORTS`, fails.
fn fails(name: &str) -> Fails {
    let name = name.trim_end_matches('\0');
    if STATUS.contains(&name) {
        Fails::Status
    } else if BOOLEAN.contains(&name) {
        Fails::False
    } else if HRESULT.contains(&name) {
        Fails::Hresult
    } else if ROLE.contains(&name) {
        Fails::Unspecified
    } else {
        Fails::Error
    }
}

extern "system" fn not_implemented() -> u32 {
    ERROR_CALL_NOT_IMPLEMENTED
}

extern "system" fn status_not_implemented() -> i32 {
    STATUS_NOT_IMPLEMENTED
}

extern "system" fn false_not_implemented() -> u8 {
    // SAFETY: sets the calling thread's last error; no other preconditions.
    unsafe { SetLastError(ERROR_CALL_NOT_IMPLEMENTED) };
    0
}

extern "system" fn hresult_not_implemented() -> i32 {
    E_NOTIMPL
}

extern "system" fn role_unspecified() -> u32 {
    0
}

/// The function a stub of an export the system copy lacks jumps to.
fn fallback(fails: Fails) -> usize {
    match fails {
        Fails::Error => (not_implemented as *const ()).addr(),
        Fails::Status => (status_not_implemented as *const ()).addr(),
        Fails::False => (false_not_implemented as *const ()).addr(),
        Fails::Hresult => (hresult_not_implemented as *const ()).addr(),
        Fails::Unspecified => (role_unspecified as *const ()).addr(),
    }
}

/// Loads the system POWRPROF.dll and points every stub at it. Returns how many
/// exports it lacks, or `None` when it cannot be loaded at all.
pub(crate) fn install() -> Option<u32> {
    for (forward, name) in FORWARDS.iter().zip(EXPORTS) {
        forward.store(fallback(fails(name)), Ordering::Release);
    }

    let mut path = [0u16; 512];
    #[expect(clippy::cast_possible_truncation, reason = "the buffer is 512 units")]
    // SAFETY: the buffer and its length match.
    let len = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    let suffix: Vec<u16> = r"\powrprof.dll".encode_utf16().chain([0]).collect();
    let end = len
        .checked_add(suffix.len())
        .filter(|&end| len != 0 && end <= path.len())?;
    path[len..end].copy_from_slice(&suffix);
    // SAFETY: `path` is NUL-terminated.
    let module = unsafe { LoadLibraryW(path.as_ptr()) };
    if module.is_null() {
        return None;
    }

    let mut missing = 0;
    for (forward, name) in FORWARDS.iter().zip(EXPORTS) {
        // SAFETY: `module` is loaded and `name` is NUL-terminated.
        match unsafe { GetProcAddress(module, name.as_ptr()) } {
            Some(function) => forward.store((function as *const ()).addr(), Ordering::Release),
            None => missing += 1,
        }
    }
    Some(missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_export_fails_in_its_own_return_type() {
        assert_eq!(fails("CallNtPowerInformation\0"), Fails::Status);
        assert!(status_not_implemented() < 0, "NT_SUCCESS must be false");
        assert_eq!(fails("GetPwrCapabilities\0"), Fails::False);
        assert_eq!(false_not_implemented(), 0);
        assert_eq!(
            fails("PowerRegisterForEffectivePowerModeNotifications\0"),
            Fails::Hresult
        );
        assert!(hresult_not_implemented() < 0, "SUCCEEDED must be false");
        assert_eq!(fails("PowerDeterminePlatformRoleEx\0"), Fails::Unspecified);
        assert_eq!(fails("PowerReadACValueIndex\0"), Fails::Error);
        assert_ne!(not_implemented(), 0);
    }

    #[test]
    fn every_listed_name_is_an_export() {
        for name in [STATUS, BOOLEAN, HRESULT, ROLE].concat() {
            assert!(
                EXPORTS.iter().any(|e| e.trim_end_matches('\0') == name),
                "{name}"
            );
        }
    }
}
