//! Inventories on the server: read what a chest, corpse or player holds, and
//! change it through the game's own transactions.
//!
//! Reads are synchronous plain memory reads. Make them where no inventory
//! changes underneath: on the server's game thread between steps (a game
//! event or a [`Scheduler`](crate::schedule::Scheduler) task) with [`Inventories::read`], or inside a
//! hooked ECS system with [`Inventories::read_in`]. Anywhere else they race
//! with the game's writes, and the game-thread reader returns nothing.
//!
//! Changes are requests. Submit them from any thread; the loader runs each in
//! the game's inventory system as one transaction, all or nothing, and hands
//! the [`Outcome`] to a callback on the game thread after a later step. The
//! [`Ticket`] a request returns holds the same outcome for polling.
//!
//! Items are not entities: an inventory links up to 16 sub-inventories of 8
//! slots each, and only items with durability or a level have an entity of
//! their own, the item's PIDE.

use crate::Entity;

/// The most operations in one [`Batch`].
pub const MAX_OPS: usize = 64;
/// The most inventories one [`Batch`] may name; the game takes 16 touched
/// inventories per transaction.
pub const MAX_INVENTORIES: usize = 16;
/// The game's op log holds this many records per transaction.
const OP_LOG: u32 = 0x200;
/// Slots per sub-inventory.
const SLOTS: u8 = 8;
/// Links per `InventorySetup`.
const LINKS: usize = 16;
/// How deep virtual sub-inventories nest before the walk stops.
const MAX_DEPTH: u32 = 4;
/// How far the owner walk climbs.
const MAX_OWNERS: usize = 8;

/// How many distinct inventories `ops` name.
fn inventories(ops: &[Op]) -> usize {
    let mut seen: Vec<Entity> = ops
        .iter()
        .flat_map(Op::inventories)
        .filter(|e| !e.is_none())
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

mod ops;
mod read;
mod results;
mod slots;
#[cfg(test)]
mod tests;

pub use ops::*;
pub use read::*;
pub use results::*;
pub use slots::*;
