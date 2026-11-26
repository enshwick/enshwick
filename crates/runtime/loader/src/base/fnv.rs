//! FNV-1a 64, the hash the game and the loader's own wire use for names,
//! account keys and checks.

pub(crate) use enshwick_values::fnv::{BASIS64 as BASIS, PRIME64 as PRIME, fnv1a64};
