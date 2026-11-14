//! The start contract between the `powrprof.dll` stub and the Enshwick loader.
//!
//! The loader exports one start function under [`EXPORT`], whose name
//! carries the contract's version, and the stub calls it through [`Start`].
//! A stub and a loader of different contracts find no common export, so the
//! stub refuses to call before any argument crosses, and the game runs
//! without mods. The loader holds a compile-time check that its export has
//! the type [`Start`].

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// Fails when rustdoc builds this crate's examples one by one, which it does
/// without a word when one of them does not merge.
///
/// ```
/// assert_ne!(module_path!(), "rust_out", "an example does not merge: \
///     RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS='-Zunstable-options --merge-doctests=yes' \
///     cargo test --doc names it");
/// ```
#[cfg(doctest)]
struct MergedExamples;

/// The name of the loader's start export, as a string literal, so the
/// loader's `export_name` and [`EXPORT`] cannot differ.
///
/// The number is the contract's version: any change to [`Start`] or to
/// what its arguments mean takes the next one.
///
/// # Examples
///
/// ```
/// assert!(enshwick_loader_start::export_name!().starts_with("enshwick_start_v"));
/// ```
#[macro_export]
macro_rules! export_name {
    () => {
        "enshwick_start_v3"
    };
}

/// The loader's start export the stub looks up, NUL-terminated.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     enshwick_loader_start::EXPORT.to_str(),
///     Ok(enshwick_loader_start::export_name!())
/// );
/// ```
pub const EXPORT: &core::ffi::CStr =
    match core::ffi::CStr::from_bytes_with_nul(concat!(export_name!(), "\0").as_bytes()) {
        Ok(name) => name,
        Err(_) => panic!("the export name holds a NUL"),
    };

/// The `missing_forwards` the stub passes when it could not install any
/// forward to the system `powrprof.dll`.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick_loader_start::NO_SYSTEM_COPY, u32::MAX);
/// ```
pub const NO_SYSTEM_COPY: u32 = u32::MAX;

/// The type of the loader's [`EXPORT`], `(missing_forwards, release)`.
///
/// - `missing_forwards` counts the `powrprof.dll` exports the system copy
///   lacks, or is [`NO_SYSTEM_COPY`].
/// - `release` is never null: a function of the stub, valid for the
///   process's life, that lets a held dedicated server's own code go on and
///   answers 1 when it held the server until then, 2 when its hold had
///   already timed out, and anything else when it held nothing. The loader
///   calls it once its hooks are armed, from any thread; a second call is
///   harmless.
/// - The stub calls the export once per process, from a thread of its own
///   and outside the Windows loader lock; the loader ignores a second call.
/// - Nothing unwinds out of it: the loader catches its own panics.
/// - The type never changes under one [`EXPORT`]; a new type takes a new
///   export name.
///
/// # Examples
///
/// ```
/// unsafe extern "system" fn start(_: u32, _: extern "system" fn() -> u32) {}
///
/// let _: enshwick_loader_start::Start = start;
/// ```
pub type Start =
    unsafe extern "system" fn(missing_forwards: u32, release: extern "system" fn() -> u32);

#[cfg(test)]
mod tests {
    use super::{EXPORT, Start};

    #[test]
    fn a_changed_start_type_comes_with_a_new_export_name() {
        // A stub and a loader agree only on the export's name, so the name
        // must move whenever the type does.
        assert_eq!(
            (EXPORT, core::any::type_name::<Start>()),
            (
                c"enshwick_start_v3",
                "unsafe extern \"system\" fn(u32, extern \"system\" fn() -> u32)"
            )
        );
    }
}
