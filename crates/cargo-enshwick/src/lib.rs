//! The library side of `cargo enshwick`: what its commands share with
//! tests, fuzzing and the platform's own tasks. It is not a stable API: mods
//! use the `enshwick` crate.

#![forbid(unsafe_code)]

#[doc(hidden)]
pub mod generate;
pub mod luau;
pub mod luau_lsp;
#[doc(hidden)]
pub mod net;
#[doc(hidden)]
pub mod pin;
pub mod points;

/// A fresh temporary folder for one test, named after the calling file and
/// `name`, so tests of different modules never share one.
#[cfg(test)]
#[track_caller]
fn scratch(name: &str) -> std::path::PathBuf {
    let file = std::panic::Location::caller()
        .file()
        .replace(['/', '\\', '.'], "-");
    let dir = std::env::temp_dir().join(format!("enshwick-{file}-{name}-{}", std::process::id()));
    let _old = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
