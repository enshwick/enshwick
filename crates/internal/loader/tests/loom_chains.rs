//! Model tests of the hook chains' slots under loom: a game thread walking a
//! chain while the loader rewires it past a faulted mod, or trades a
//! reloaded mod's link, only ever follows a slot to a link or the original.
//!
//! `RUSTFLAGS="--cfg loom" cargo test -p enshwick-loader --test loom_chains --release`

#![cfg(loom)]

#[cfg(test)]
mod tests {
    use enshwick::abi::{HookFlags, Status};
    use enshwick_loader::chains::{Chains, Link};
    use loom::sync::atomic::{AtomicUsize, Ordering};
    use loom::thread;

    const TARGET: u64 = 0x10;
    const ORIGINAL: usize = 0xff;

    /// A slot for one model run; loom tracks atomics per run, so each run
    /// makes its own.
    fn slot() -> &'static AtomicUsize {
        Box::leak(Box::new(AtomicUsize::new(0)))
    }

    fn link(owner: usize, detour: usize, priority: i32) -> Link {
        Link {
            owner,
            detour,
            next: slot(),
            priority,
            flags: HookFlags::NONE,
            hop: None,
        }
    }

    /// Where each entry calls on: a link's detour and its `next` slot.
    type Hops = Vec<(usize, &'static AtomicUsize)>;

    /// An armed chain of one link per owner, `(owner, detour)` in running
    /// order, wired with none faulted, and its head.
    fn armed(links: &[(usize, usize)]) -> (Chains, &'static AtomicUsize, Hops) {
        let mut chains = Chains::default();
        let mut hops = Vec::new();
        for (priority, &(owner, detour)) in (0..).zip(links) {
            let link = link(owner, detour, priority);
            hops.push((detour, link.next));
            assert_eq!(chains.add(TARGET, link), Status::OK);
        }
        let head = slot();
        for chain in chains.iter_mut() {
            chain.head = Some(head);
            chain.original = ORIGINAL;
        }
        chains.seal();
        chains.rewire(|_| false);
        (chains, head, hops)
    }

    /// The entries a call into the chain passes, as a game thread follows
    /// the slots: never an empty slot, always ending at the original.
    fn walk(head: &AtomicUsize, hops: &[(usize, &'static AtomicUsize)]) -> Vec<usize> {
        let mut passed = Vec::new();
        let mut at = head.load(Ordering::Acquire);
        while at != ORIGINAL {
            assert_ne!(at, 0, "a call went through an empty slot");
            assert!(passed.len() < hops.len(), "the chain loops: {passed:x?}");
            passed.push(at);
            let (_, next) = hops
                .iter()
                .find(|(detour, _)| *detour == at)
                .unwrap_or_else(|| panic!("a slot points at {at:#x}, no link"));
            at = next.load(Ordering::Acquire);
        }
        passed
    }

    #[test]
    fn a_call_during_a_rewire_never_meets_an_empty_slot_and_one_after_skips_the_faulted() {
        loom::model(|| {
            let (chains, head, hops) = armed(&[(1, 0xa), (2, 0xb), (3, 0xc)]);
            let walker = {
                let hops = hops.clone();
                thread::spawn(move || walk(head, &hops))
            };
            chains.rewire(|owner| owner == 2);
            let during = walker.join().unwrap();
            assert!(
                [vec![0xa, 0xb, 0xc], vec![0xa, 0xc]].contains(&during),
                "{during:x?}"
            );
            assert_eq!(walk(head, &hops), [0xa, 0xc]);
        });
    }

    #[test]
    fn a_call_during_a_reload_passes_the_old_link_the_new_or_neither() {
        loom::model(|| {
            let (mut chains, head, mut hops) = armed(&[(1, 0xa), (2, 0xb)]);
            let reloaded = link(2, 0xbb, 1);
            hops.push((reloaded.detour, reloaded.next));
            let walker = {
                let hops = hops.clone();
                thread::spawn(move || walk(head, &hops))
            };
            chains.rewire(|owner| owner == 2);
            chains.remove_owner(2);
            assert_eq!(chains.relink(TARGET, reloaded), Status::OK);
            chains.rewire(|_| false);
            let during = walker.join().unwrap();
            assert!(
                [vec![0xa, 0xb], vec![0xa], vec![0xa, 0xbb]].contains(&during),
                "{during:x?}"
            );
            assert_eq!(walk(head, &hops), [0xa, 0xbb]);
        });
    }

    #[test]
    fn two_calls_during_a_rewire_both_end_at_the_original() {
        loom::model(|| {
            let (chains, head, hops) = armed(&[(1, 0xa), (2, 0xb)]);
            let walkers: Vec<_> = (0..2)
                .map(|_| {
                    let hops = hops.clone();
                    thread::spawn(move || walk(head, &hops))
                })
                .collect();
            chains.rewire(|owner| owner == 1);
            for walker in walkers {
                let passed = walker.join().unwrap();
                assert!([vec![0xa, 0xb], vec![0xb]].contains(&passed), "{passed:x?}");
            }
        });
    }
}
