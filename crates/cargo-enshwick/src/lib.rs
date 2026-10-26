//! The library side of `cargo enshwick`: what its commands share with
//! tests and fuzzing. It is no stable API; mods use the `enshwick` crate.

#![forbid(unsafe_code)]

pub mod points;
