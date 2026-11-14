//! One module per `cargo xtask` task, and the modules only a task runs.
//! `compat.rs` here is the root of the binary `xtask-compat`, which links
//! the loader and so is a binary of its own.

pub(crate) mod check;
pub(crate) mod close;
pub(crate) mod dist;
pub(crate) mod doc_links;
pub(crate) mod fuzz;
pub(crate) mod header;
pub(crate) mod notes;
pub(crate) mod peel;
pub(crate) mod tag_lines;
pub(crate) mod wine;
