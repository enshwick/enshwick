//! R19: how the game unlocks and takes back skills. Every change of a
//! character's unlocked skill nodes goes through one function that adds a
//! level to a node (`add` 1, from an unlock the player paid for) or takes
//! one away (`add` 0, a refund the game checks against the tree and undoes
//! when the tree would break); this logs each call with its node, its
//! direction, its answer and the node's level before and after.

use std::sync::atomic::{AtomicU32, Ordering};

use enshwick_loader_api::{Load, Next, Off};

use crate::memory;

/// The most calls logged in a run.
const MOST: u32 = 500;
/// The unlocked nodes: 256 ids from `+4`, a level byte each from `+0x804`
/// in its top three bits; build 1269579.
const IDS: usize = 4;
const LEVELS: usize = 0x804;
const NODES: usize = 256;

static SEEN: AtomicU32 = AtomicU32::new(0);

type Change = unsafe extern "system" fn(node: u32, nodes: *mut u8, add: u8, view: *const u8) -> u8;

enshwick_loader_api::detour! {
    static CHANGE: unsafe extern "system" fn(
        node: u32,
        nodes: *mut u8,
        add: u8,
        view: *const u8,
    ) -> u8 = change;
}

/// Links the skill change.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    load.hook(&CHANGE, load.symbol("research.skill_change")?)
}

fn change(next: Next<Change>, node: u32, nodes: *mut u8, add: u8, view: *const u8) -> u8 {
    let before = level(nodes.addr(), node);
    // SAFETY: the game's own arguments, forwarded unchanged.
    let answer = unsafe { next.get()(node, nodes, add, view) };
    let number = SEEN.fetch_add(1, Ordering::Relaxed) + 1;
    if number <= MOST {
        let after = level(nodes.addr(), node);
        let unlocked = count(nodes.addr());
        enshwick_loader_api::info!(
            "R19 skill change {number}: node {node:#010x}, add {add}, answer {answer}, level {before:?} -> {after:?}, {unlocked:?} nodes unlocked"
        );
    }
    answer
}

/// The level of `node` in the unlocked nodes at `nodes`, `None` when it is
/// not unlocked or the block is unreadable.
fn level(nodes: usize, node: u32) -> Option<u8> {
    let mut ids = [0u8; NODES * 4];
    if memory::read(nodes + IDS, &mut ids) < ids.len() {
        return None;
    }
    let slot = ids
        .as_chunks::<4>()
        .0
        .iter()
        .position(|id| u32::from_le_bytes(*id) == node)?;
    let mut byte = [0u8; 1];
    (memory::read(nodes + LEVELS + slot, &mut byte) == 1).then_some(byte[0] >> 5)
}

/// How many nodes the block holds.
fn count(nodes: usize) -> Option<usize> {
    let mut ids = [0u8; NODES * 4];
    (memory::read(nodes + IDS, &mut ids) == ids.len()).then(|| {
        ids.as_chunks::<4>()
            .0
            .iter()
            .filter(|id| u32::from_le_bytes(**id) != 0)
            .count()
    })
}
