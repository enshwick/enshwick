//! Hook chains: every mod's links on every target, and the slots that join
//! them.
//!
//! The game calls the target, which jumps to the chain's head slot; each link
//! calls on through its own `next` slot, and the last one reaches the original.
//! Wiring writes only those slots, so turning a faulty mod off while the game
//! runs never touches code.

use core::marker::PhantomData;
// The loom model in `tests/loom_chains.rs` swaps in loom's atomics, which
// record every load and store for the model to permute.
#[cfg(loom)]
use loom::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
#[cfg(not(loom))]
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use enshwick::abi::{HookFlags, Status};
use rustc_hash::FxHashMap;

/// The owner of the loader's own links, which never turn off.
pub const LOADER: usize = usize::MAX;

/// The `next` slot of one of the loader's own links, typed by the function
/// it hooks, so a link calls on with the hooked function's signature.
#[derive(Debug)]
pub struct Next<F> {
    slot: AtomicUsize,
    function: PhantomData<F>,
}

impl<F: Copy> Next<F> {
    /// An empty slot, which the loader points at the rest of the chain when
    /// it wires it.
    ///
    /// # Safety
    ///
    /// - `F` is the function pointer type of the hooked function, with its
    ///   exact signature and calling convention.
    /// - The slot is used only as the `next` of a link on that function.
    /// - [`Next::get`] is called only from that link's detour.
    ///
    /// Chains are wired before their targets jump to them, so the slot holds
    /// a function of type `F` whenever the detour runs.
    #[cfg(not(loom))]
    #[must_use]
    pub const unsafe fn new() -> Self {
        const {
            assert!(size_of::<F>() == size_of::<usize>());
        }
        Self {
            slot: AtomicUsize::new(0),
            function: PhantomData,
        }
    }

    /// The rest of the chain: the next link, or the original function.
    #[must_use]
    pub fn get(&self) -> F {
        let address = self.slot.load(Ordering::Acquire);
        debug_assert_ne!(address, 0, "a link ran before its chain was wired");
        // SAFETY: by the contract of `new`, the slot holds a function of
        // type `F` whenever a detour calls this, and `F` is pointer-sized.
        unsafe { core::mem::transmute_copy::<usize, F>(&address) }
    }

    /// The rest of the chain, or `None` before the chain is wired, for a
    /// caller that is not the link's own detour.
    #[must_use]
    pub fn wired(&self) -> Option<F> {
        let address = self.slot.load(Ordering::Acquire);
        // SAFETY: by the contract of `new`, a slot that is not empty holds a
        // function of type `F`, and `F` is pointer-sized.
        (address != 0).then(|| unsafe { core::mem::transmute_copy::<usize, F>(&address) })
    }

    /// The untyped slot the chain wires.
    #[must_use]
    pub const fn slot(&'static self) -> &'static AtomicUsize {
        &self.slot
    }

    /// `detour`'s address, for a link whose `next` is this slot.
    #[must_use]
    pub fn address_of(detour: F) -> usize {
        // SAFETY: `F` is a function pointer type by the contract of `new`,
        // checked pointer-sized there.
        unsafe { core::mem::transmute_copy::<F, usize>(&detour) }
    }
}

/// Calls through a thunk, beside its slot; the thunk writes them.
#[repr(C)]
#[derive(Debug, Default)]
pub struct Counter {
    /// Calls since arming.
    pub calls: AtomicU64,
    /// Timed calls.
    pub samples: AtomicU64,
    /// Time-stamp counter ticks of the timed calls.
    pub ticks: AtomicU64,
}

impl Counter {
    /// The ticks every call took, estimated from the timed ones; `None`
    /// before the first timed call.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "an estimate for people to read")]
    pub fn estimate(calls: u64, samples: u64, ticks: u64) -> Option<f64> {
        (samples > 0).then(|| ticks as f64 / samples as f64 * calls as f64)
    }
}

/// A timed thunk in front of a link or the original, so each link's own
/// time can be told from the rest of the chain's: calls go to `entry`,
/// which jumps through `slot`, and `counter` keeps its calls and times.
#[derive(Debug, Clone, Copy)]
pub struct Hop {
    /// Where calls into the link or the original go.
    pub entry: usize,
    /// The thunk's slot: the link's detour, or the original.
    pub slot: &'static AtomicUsize,
    /// The thunk's counter.
    pub counter: &'static Counter,
}

/// A link a target refused, for the conflict report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// The game function.
    pub target: u64,
    /// The mod that asked.
    pub owner: usize,
    /// The mod that holds the target exclusively, or `None` for the asking
    /// mod's own second link.
    pub holder: Option<usize>,
}

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
    /// The timed thunk in front of the link, with timing on.
    pub hop: Option<Hop>,
}

impl Link {
    /// Where calls into the link go: its timed thunk, or its detour.
    #[must_use]
    pub fn entry(&self) -> usize {
        self.hop.map_or(self.detour, |hop| hop.entry)
    }
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
    /// The timed thunk in front of the original, with timing on.
    pub original_hop: Option<Hop>,
    /// Whether the chain captures the caller frame, as decided when the
    /// chains were sealed.
    pub frame: bool,
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
    #[must_use]
    pub fn running_order(&self) -> Vec<&Link> {
        let mut links: Vec<&Link> = self.links.iter().collect();
        links.sort_by_key(|l| l.priority);
        links
    }

    /// Points the head and every `next` slot past links whose owner is
    /// `faulted`, ending at the original.
    pub fn wire(&self, faulted: impl Fn(usize) -> bool) {
        let links = self.running_order();
        let original = match self.original_hop {
            Some(hop) => {
                hop.slot.store(self.original, Ordering::Release);
                hop.entry
            }
            None => self.original,
        };
        let after = |from: usize| {
            links[from..]
                .iter()
                .find(|l| !faulted(l.owner))
                .map_or(original, |l| l.entry())
        };
        // From the end back to the head: a slot is pointed at a link only
        // once that link's own `next` is set, so a game thread entering the
        // chain meanwhile never calls through an empty slot.
        for (i, link) in links.iter().enumerate().rev() {
            link.next.store(after(i + 1), Ordering::Release);
        }
        if let Some(head) = self.head {
            head.store(after(0), Ordering::Release);
        }
    }
}

/// Every chain, keyed by target.
#[derive(Debug, Default)]
pub struct Chains {
    by_target: FxHashMap<u64, Chain>,
    armed: bool,
    refused: Vec<Refusal>,
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
        // One slot joins one chain: a link whose slot is wired twice would
        // call on into the other target's chain with this one's arguments.
        if self.slot_taken(link.next) {
            return Status::REFUSED;
        }
        let chain = self.by_target.entry(target).or_insert_with(|| Chain {
            target,
            links: Vec::new(),
            head: None,
            original: 0,
            original_hop: None,
            frame: false,
        });
        let clash = chain.links.iter().find(|l| {
            l.owner == link.owner
                || l.flags.contains(HookFlags::EXCLUSIVE)
                || link.flags.contains(HookFlags::EXCLUSIVE)
        });
        if let Some(held) = clash {
            let holder = (held.owner != link.owner).then_some(held.owner);
            self.refused.push(Refusal {
                target,
                owner: link.owner,
                holder,
            });
            return Status::REFUSED;
        }
        chain.links.push(link);
        Status::OK
    }

    /// Whether `next` is already the slot of a link on any target.
    fn slot_taken(&self, next: &AtomicUsize) -> bool {
        self.by_target
            .values()
            .flat_map(|chain| &chain.links)
            .any(|l| core::ptr::eq(l.next, next))
    }

    /// Every chain, for arming.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Chain> {
        self.by_target.values_mut()
    }

    /// Every chain.
    pub fn iter(&self) -> impl Iterator<Item = &Chain> {
        self.by_target.values()
    }

    /// Marks the chains armed: no more links, except a reloaded mod's.
    pub fn seal(&mut self) {
        self.armed = true;
        for chain in self.by_target.values_mut() {
            chain.frame = chain.wants_caller_frame();
        }
    }

    /// Takes every link of `owner` out of the chains, for a mod that is
    /// reloaded: its links must already be wired past, and no game thread
    /// may still run in them.
    pub fn remove_owner(&mut self, owner: usize) {
        for chain in self.by_target.values_mut() {
            chain.links.retain(|l| l.owner != owner);
        }
    }

    /// Adds a reloaded mod's `link` to the armed chain of `target`; the
    /// caller rewires. Refused for a target that has no armed chain (a new
    /// target needs a restart), for a link that reads the caller frame on a
    /// chain armed without it, and as [`Chains::add`] refuses.
    pub fn relink(&mut self, target: u64, link: Link) -> Status {
        if link.detour == 0 {
            return Status::BAD_ARGS;
        }
        let Some(chain) = self.by_target.get_mut(&target).filter(|c| c.head.is_some()) else {
            return Status::TOO_LATE;
        };
        if link.flags.contains(HookFlags::CALLER_FRAME) && !chain.frame {
            return Status::TOO_LATE;
        }
        let chain_target = chain.target;
        if self.slot_taken(link.next) {
            return Status::REFUSED;
        }
        let Some(chain) = self.by_target.get_mut(&chain_target) else {
            return Status::TOO_LATE;
        };
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

    /// Rewires every chain past the links of faulted mods.
    pub fn rewire(&self, faulted: impl Fn(usize) -> bool + Copy) {
        for chain in self.by_target.values() {
            if chain.head.is_some() {
                chain.wire(faulted);
            }
        }
    }

    /// The mods linked on `target`.
    #[must_use]
    pub fn owners(&self, target: u64) -> Vec<usize> {
        self.by_target
            .get(&target)
            .map(|c| c.links.iter().map(|l| l.owner).collect())
            .unwrap_or_default()
    }

    /// The links targets refused, in order.
    #[must_use]
    pub fn refused(&self) -> &[Refusal] {
        &self.refused
    }

    /// How many targets have links.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_target.len()
    }

    /// Whether no target has a link.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_target.is_empty()
    }
}

// Static std slots; the loom model has its own test.
#[cfg(not(loom))]
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
            hop: None,
        }
    }

    fn value(slot: &AtomicUsize) -> usize {
        slot.load(Ordering::Acquire)
    }

    #[test]
    fn a_slot_joins_one_chain_only() {
        let mut chains = Chains::default();
        let first = link(0, 0xa, 0, HookFlags::NONE);
        let next = first.next;
        assert_eq!(chains.add(0x10, first), Status::OK);
        let again = Link {
            next,
            ..link(0, 0xa, 0, HookFlags::NONE)
        };
        assert_eq!(chains.add(0x20, again), Status::REFUSED);
        let other = Link {
            next,
            ..link(1, 0xb, 0, HookFlags::NONE)
        };
        assert_eq!(chains.add(0x10, other), Status::REFUSED);
        assert_eq!(
            chains.add(0x20, link(0, 0xa, 0, HookFlags::NONE)),
            Status::OK
        );
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
        let chain = &chains.by_target[&0x10];
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
    fn a_reloaded_mod_trades_its_links_on_armed_chains_only() {
        let mut chains = Chains::default();
        assert_eq!(
            chains.add(0x10, link(0, 0xa, 0, HookFlags::NONE)),
            Status::OK
        );
        assert_eq!(
            chains.add(0x10, link(1, 0xb, 0, HookFlags::NONE)),
            Status::OK
        );
        let head = slot();
        for chain in chains.iter_mut() {
            chain.head = Some(head);
            chain.original = 0xff;
        }
        chains.seal();
        chains.rewire(|owner| owner == 1);
        chains.remove_owner(1);
        assert_eq!(chains.owners(0x10), [0]);
        assert_eq!(
            chains.relink(0x20, link(1, 0xc, 0, HookFlags::NONE)),
            Status::TOO_LATE,
            "a new target needs a restart"
        );
        assert_eq!(
            chains.relink(0x10, link(1, 0xc, 0, HookFlags::CALLER_FRAME)),
            Status::TOO_LATE,
            "the chain was armed without the caller frame"
        );
        assert_eq!(
            chains.relink(0x10, link(1, 0xc, -1, HookFlags::NONE)),
            Status::OK
        );
        assert_eq!(
            chains.relink(0x10, link(1, 0xd, 0, HookFlags::NONE)),
            Status::REFUSED
        );
        chains.rewire(|_| false);
        assert_eq!(value(head), 0xc);
        assert_eq!(chains.owners(0x10), [0, 1]);
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
        assert_eq!(
            chains.refused(),
            [
                Refusal {
                    target: 0x10,
                    owner: 1,
                    holder: Some(0)
                },
                Refusal {
                    target: 0x20,
                    owner: 1,
                    holder: None
                },
                Refusal {
                    target: 0x20,
                    owner: 2,
                    holder: Some(1)
                },
            ]
        );
    }

    #[test]
    fn timed_thunks_stand_in_front_of_links_and_the_original() {
        static COUNTER: Counter = Counter {
            calls: AtomicU64::new(0),
            samples: AtomicU64::new(0),
            ticks: AtomicU64::new(0),
        };
        let mut chains = Chains::default();
        let mut first = link(0, 0xa, 0, HookFlags::NONE);
        let first_slot = slot();
        first.hop = Some(Hop {
            entry: 0x1a,
            slot: first_slot,
            counter: &COUNTER,
        });
        assert_eq!(chains.add(0x30, first), Status::OK);
        assert_eq!(
            chains.add(0x30, link(1, 0xb, 1, HookFlags::NONE)),
            Status::OK
        );
        let (head, original_slot) = (slot(), slot());
        for chain in chains.iter_mut() {
            chain.head = Some(head);
            chain.original = 0xff;
            chain.original_hop = Some(Hop {
                entry: 0x1f,
                slot: original_slot,
                counter: &COUNTER,
            });
            chain.wire(|_| false);
        }
        let chain = &chains.by_target[&0x30];
        assert_eq!(value(head), 0x1a);
        assert_eq!(value(chain.links[0].next), 0xb);
        assert_eq!(value(chain.links[1].next), 0x1f);
        assert_eq!(value(original_slot), 0xff);
        assert_eq!(Counter::estimate(1000, 4, 400), Some(100_000.0));
        assert_eq!(Counter::estimate(1000, 0, 0), None);
    }
}
