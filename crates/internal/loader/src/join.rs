//! The client's join check: before it joins a server, it compares the server's
//! mod set with its own.
//!
//! Mods the server does not run are turned off on the client, with a note; when
//! the server runs mods the client lacks, or in another version, the client
//! says which and does not join. A client that joins runs its join mods with
//! the server's join values over its own settings files, and with its files'
//! values again on joining a server without them; the files stay as they are.
//!
//! A client kept out shows one dialog naming each of the server's mods it
//! lacks, the server's version and where to get it ([`refusal`]), and tells
//! its mods which they are, with the versions and homepages the server
//! advertises
//! ([`Event::JOIN_REFUSED`](enshwick::abi::Event::JOIN_REFUSED)).
//!
//! Every join from the game's menus hands its online provider a target with
//! the server's address and query port. The hook asks the server, over the
//! Steam query protocol, for its `enshwick` rule before the game connects;
//! a refused join goes on without its host, so the game's own join step fails
//! and the game goes back to its menus, its own dialog held back
//! ([`interaction`]).

use std::collections::BTreeMap;

pub mod interaction;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game lists missing mods")
)]
pub(crate) mod missing;
mod plan;
pub mod provider;
pub mod refusal;
mod rules;
mod target;

pub(crate) use plan::entries;
use plan::label;
pub use plan::{Plan, differences, plan, wanted};
pub use rules::{
    changes, mod_rules_value, mods_complete, name, page, query_rules, rule, server_values,
    sync_value,
};
pub use target::{
    Destination, GAME_PORT, HAS_ADDRESS, HOST, IP, PASSWORD, QUERY_PORT, TARGET_BYTES, Target,
    destination, refuse,
};

/// The names players read, by mod id: the client's own and those the server
/// advertises.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::Names;
///
/// let mut names = Names::new();
/// names.insert("you.trading-post".to_owned(), "Trading Post".to_owned());
/// assert_eq!(names["you.trading-post"], "Trading Post");
/// ```
pub type Names = BTreeMap<String, String>;

#[cfg(windows)]
pub(crate) use runtime::{NEXT, detour};

#[cfg(windows)]
mod runtime;

#[cfg(windows)]
pub(crate) use runtime::{names, set_names};
