//! The start contract between the `powrprof.dll` stub and the Enshwick loader.
//!
//! The stub calls the loader's `enshwick_start` export through [`Start`],
//! and the loader holds a compile-time check that its export has that type,
//! so the two sides cannot drift apart in their signature.

#![no_std]
#![forbid(unsafe_code)]

/// The version of the start contract the stub passes as `stub_abi`; from 2
/// on, the loader calls `release` once its hooks are armed.
///
/// # Examples
///
/// ```
/// assert!(enshwick_loader_start::STUB_ABI >= 2);
/// ```
pub const STUB_ABI: u32 = 2;

/// The `missing_forwards` the stub passes when it could not install any
/// forward to the system `powrprof.dll`.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick_loader_start::NO_SYSTEM_COPY, u32::MAX);
/// ```
pub const NO_SYSTEM_COPY: u32 = u32::MAX;

/// The signature of the loader's `enshwick_start(stub_abi, missing_forwards,
/// release)` export, which the stub calls once, outside the loader lock.
///
/// # Examples
///
/// ```
/// unsafe extern "system" fn start(_: u32, _: u32, _: extern "system" fn() -> u32) {}
///
/// let _: enshwick_loader_start::Start = start;
/// ```
pub type Start = unsafe extern "system" fn(
    stub_abi: u32,
    missing_forwards: u32,
    release: extern "system" fn() -> u32,
);
