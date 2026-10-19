//! Hook chains: every mod's links on every target, and the slots that join
//! them.
//!
//! The game calls the target, which jumps to the chain's head slot; each link
//! calls on through its own `next` slot, and the last one reaches the original.
//! Wiring writes only those slots, so turning a faulty mod off while the game
//! runs never touches code.

use std::sync::atomic::{AtomicUsize, Ordering};

use api::abi::{HookFlags, Status};
use rustc_hash::FxHashMap;

/// One mod's link on a target.
#[derive(Debug)]
pub struct Link {
    /// The owning mod's index.
    pub owner: usize,
    /// The link's entry point.
    pub detour: usize,
    /// The link's `next` slot, a static inside the mod's DLL.
    pub next: &'static AtomicUsize,
    /// Lower runs earlier.
    pub priority: i32,
    /// Hook flags.
    pub flags: HookFlags,
}

/// Every link on one game function.
#[derive(Debug)]
pub struct Chain {
    /// The game function.
    pub target: u64,
    /// In registration order, which is mod load order.
    pub links: Vec<Link>,
    /// Where the target jumps to; set when the chain is armed.
    pub head: Option<&'static AtomicUsize>,
    /// The original function's trampoline; set when the chain is armed.
    pub original: usize,
}

impl Chain {
    /// Whether any link reads the caller frame.
    #[must_use]
    pub fn wants_caller_frame(&self) -> bool {
        self.links
            .iter()
            .any(|l| l.flags.contains(HookFlags::CALLER_FRAME))
    }

    /// Links in running order: by priority, then registration order.
    fn running_order(&self) -> Vec<&Link> {
        let mut links: Vec<&Link> = self.links.iter().collect();
        links.sort_by_key(|l| l.priority);
        links
    }

    /// Points the head and every `next` slot past links whose owner is
    /// `faulted`, ending at the original.
    pub fn wire(&self, faulted: impl Fn(usize) -> bool) {
        let links = self.running_order();
        let after = |from: usize| {
            links[from..]
                .iter()
                .find(|l| !faulted(l.owner))
                .map_or(self.original, |l| l.detour)
        };
        if let Some(head) = self.head {
            head.store(after(0), Ordering::Release);
        }
        for (i, link) in links.iter().enumerate() {
            link.next.store(after(i + 1), Ordering::Release);
        }
    }
}

/// Every chain, keyed by target.
#[derive(Debug, Default)]
pub struct Chains {
    chains: FxHashMap<u64, Chain>,
    armed: bool,
}

impl Chains {
    /// Adds `link` to the chain of `target`.
    ///
    /// Refused after arming, for a second link of the same mod on one target,
    /// and when either side claims the target exclusively.
    pub fn add(&mut self, target: u64, link: Link) -> Status {
        if self.armed {
            return Status::TOO_LATE;
        }
        if target == 0 || link.detour == 0 {
            return Status::BAD_ARGS;
        }
        let chain = self.chains.entry(target).or_insert_with(|| Chain {
            target,
            links: Vec::new(),
            head: None,
            original: 0,
        });
        let clash = chain.links.iter().any(|l| {
            l.owner == link.owner
                || l.flags.contains(HookFlags::EXCLUSIVE)
                || link.flags.contains(HookFlags::EXCLUSIVE)
        });
        if clash {
            return Status::REFUSED;
        }
        chain.links.push(link);
        Status::OK
    }

    /// Every chain, for arming.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Chain> {
        self.chains.values_mut()
    }

    /// Marks the chains armed: no more links.
    pub fn seal(&mut self) {
        self.armed = true;
    }

    /// Rewires every chain past the links of faulted mods.
    pub fn rewire(&self, faulted: impl Fn(usize) -> bool + Copy) {
        for chain in self.chains.values() {
            if chain.head.is_some() {
                chain.wire(faulted);
            }
        }
    }

    /// The mods linked on `target`.
    #[must_use]
    pub fn owners(&self, target: u64) -> Vec<usize> {
        self.chains
            .get(&target)
            .map(|c| c.links.iter().map(|l| l.owner).collect())
            .unwrap_or_default()
    }

    /// How many targets have links.
    #[must_use]
    pub fn len(&self) -> usize {
        self.chains.len()
    }

    /// Whether no target has a link.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chains.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SLOTS: [AtomicUsize; 32] = [const { AtomicUsize::new(0) }; 32];
    static NEXT_SLOT: AtomicUsize = AtomicUsize::new(0);

    fn slot() -> &'static AtomicUsize {
        &SLOTS[NEXT_SLOT.fetch_add(1, Ordering::Relaxed)]
    }

    fn link(owner: usize, detour: usize, priority: i32, flags: HookFlags) -> Link {
        Link {
            owner,
            detour,
            next: slot(),
            priority,
            flags,
        }
    }

    fn value(slot: &AtomicUsize) -> usize {
        slot.load(Ordering::Acquire)
    }

    #[test]
    fn links_run_by_priority_and_end_at_the_original() {
        let mut chains = Chains::default();
        assert_eq!(
            chains.add(0x10, link(0, 0xa, 0, HookFlags::NONE)),
            Status::OK
        );
        assert_eq!(
            chains.add(0x10, link(1, 0xb, -5, HookFlags::NONE)),
            Status::OK
        );
        assert_eq!(
            chains.add(0x10, link(2, 0xc, 0, HookFlags::NONE)),
            Status::OK
        );
        let head = slot();
        for chain in chains.iter_mut() {
            chain.head = Some(head);
            chain.original = 0xff;
            chain.wire(|_| false);
        }
        chains.seal();
        let chain = &chains.chains[&0x10];
        assert_eq!(value(head), 0xb);
        assert_eq!(value(chain.links[1].next), 0xa);
        assert_eq!(value(chain.links[0].next), 0xc);
        assert_eq!(value(chain.links[2].next), 0xff);

        chains.rewire(|owner| owner == 0);
        assert_eq!(value(chain.links[1].next), 0xc);
        // The faulted link, if a call is still inside it, continues correctly.
        assert_eq!(value(chain.links[0].next), 0xc);

        chains.rewire(|_| true);
        assert_eq!(value(head), 0xff);
        assert_eq!(
            chains.add(0x20, link(0, 0xd, 0, HookFlags::NONE)),
            Status::TOO_LATE
        );
    }

    #[test]
    fn exclusive_and_duplicate_links_are_refused() {
        let mut chains = Chains::default();
        assert_eq!(
            chains.add(0x10, link(0, 0xa, 0, HookFlags::EXCLUSIVE)),
            Status::OK
        );
        assert_eq!(
            chains.add(0x10, link(1, 0xb, 0, HookFlags::NONE)),
            Status::REFUSED
        );
        assert_eq!(
            chains.add(0x20, link(1, 0xb, 0, HookFlags::NONE)),
            Status::OK
        );
        assert_eq!(
            chains.add(0x20, link(1, 0xc, 0, HookFlags::NONE)),
            Status::REFUSED
        );
        assert_eq!(
            chains.add(0x20, link(2, 0xc, 0, HookFlags::EXCLUSIVE)),
            Status::REFUSED
        );
        assert_eq!(
            chains.add(0, link(2, 0xc, 0, HookFlags::NONE)),
            Status::BAD_ARGS
        );
        assert_eq!(chains.len(), 2);
    }
}
