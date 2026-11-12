//! The waiting patches held from arming and their tries on the game thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::Schedule;
use crate::engine::code::{CodePatch, Verdict};
use crate::engine::detour::{Bytes, Commit};
use crate::engine::loader::Loader;
use enshwick_base::Relock;

/// [`Retry::due`] with nothing held.
const IDLE: u64 = u64::MAX;

/// The code patches waiting to go in, and when the step next checks them.
#[derive(Debug)]
pub(crate) struct Retry {
    /// Where [`Retry::due`] counts from.
    start: Instant,
    /// Milliseconds from `start` to the next check, or [`IDLE`]: the step
    /// compares it without the lock.
    due: AtomicU64,
    held: Mutex<Option<Held>>,
}

/// What arming left for the tries.
#[derive(Debug)]
struct Held {
    /// Every code patch of arming.
    patches: Vec<CodePatch>,
    /// What became of each, `Wait` for the ones still to try.
    verdicts: Vec<Verdict>,
    /// The hook targets arming armed, which the patch features rest on.
    hooked: Vec<u64>,
    schedule: Schedule,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            due: AtomicU64::new(IDLE),
            held: Mutex::new(None),
        }
    }
}

impl Retry {
    fn millis(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.start).as_millis()).unwrap_or(IDLE - 1)
    }

    /// Publishes when the step next checks `held`.
    fn publish(&self, held: Option<&Held>) {
        let due = held
            .and_then(|h| h.schedule.next())
            .map_or(IDLE, |at| self.millis(at));
        self.due.store(due, Ordering::Release);
    }
}

/// Holds the patches arming left waiting, if any, for the tries.
pub(crate) fn hold(
    loader: &Loader,
    patches: Vec<CodePatch>,
    verdicts: Vec<Verdict>,
    hooked: Vec<u64>,
) {
    if !verdicts.contains(&Verdict::Wait) {
        return;
    }
    let held = Held {
        patches,
        verdicts,
        hooked,
        schedule: Schedule::new(Instant::now()),
    };
    let mut slot = loader.retry.held.relock();
    loader.retry.publish(Some(&held));
    *slot = Some(held);
}

/// Checks the waiting patches when a check is due, on the game thread after
/// its step: one atomic compare otherwise.
pub(crate) fn after_step(loader: &Loader, now: Instant) {
    if loader.retry.millis(now) >= loader.retry.due.load(Ordering::Acquire) {
        check(loader, now);
    }
}

/// Tries to write the waiting patches once, with no wait: a thread inside
/// one, this one among them, leaves them for a later check.
#[cold]
#[inline(never)]
fn check(loader: &Loader, now: Instant) {
    let mut slot = loader.retry.held.relock();
    let Some(held) = slot.as_mut() else {
        return loader.retry.publish(None);
    };
    if !held.schedule.due(now) {
        return loader.retry.publish(Some(held));
    }
    // A mod turned off while it waited gets none of its patches.
    for (patch, verdict) in held.patches.iter().zip(&mut held.verdicts) {
        if *verdict == Verdict::Wait
            && !patch.owner.is_loader()
            && crate::engine::loader::is_off(patch.owner)
        {
            *verdict = Verdict::Off;
            if let Some(record) = loader.mods.get(patch.owner.get()) {
                record.waiting.store(false, Ordering::Release);
            }
        }
    }
    let changed = held.patches.iter().zip(&held.verdicts).any(|(p, v)| {
        *v == Verdict::Wait
            && crate::engine::code::read(p.address, p.expected.len()).as_deref()
                != Some(&*p.expected)
    });
    if changed {
        let held = slot.take();
        loader.retry.publish(None);
        drop(slot);
        return give_up(loader, held, "the bytes changed while it waited");
    }
    let writes: Vec<Bytes<'_>> = held
        .patches
        .iter()
        .zip(&held.verdicts)
        .enumerate()
        .filter(|&(_, (_, v))| *v == Verdict::Wait)
        .map(|(id, (p, _))| Bytes {
            id,
            address: p.address,
            replacement: &p.replacement,
        })
        .collect();
    if writes.is_empty() {
        crate::engine::diag::record_patches(crate::engine::code::tally(&held.verdicts));
        *slot = None;
        return loader.retry.publish(None);
    }
    let marker = 0_u8;
    let chains = loader.chains.relock();
    // Nothing waiting is armed yet, so a busy round has nothing to disarm.
    match crate::engine::detour::commit_once(&writes, || {}, (&raw const marker).addr()) {
        // The threads already run the patch while its owner's hooks are
        // still bypassed until `rewire`; a bypassed hook forwards to the
        // original, so those few calls act as vanilla, never half-hooked.
        Commit::Written => {
            for (patch, verdict) in held.patches.iter().zip(&mut held.verdicts) {
                if *verdict != Verdict::Wait {
                    continue;
                }
                *verdict = Verdict::Write;
                if let Some(record) = loader.mods.get(patch.owner.get()) {
                    record.waiting.store(false, Ordering::Release);
                }
                crate::log!(
                    INFO,
                    "code patch of {} at {:#x} written after waiting, {} bytes",
                    crate::engine::code::owner_id(loader, patch.owner),
                    patch.address,
                    patch.replacement.len()
                );
            }
            chains.rewire(crate::engine::loader::bypassed);
            drop(chains);
            crate::engine::code::arming(&held.patches, &held.verdicts, &held.hooked);
            crate::engine::diag::record_patches(crate::engine::code::tally(&held.verdicts));
            *slot = None;
            loader.retry.publish(None);
        }
        // A thread that could not be suspended or read is as good as one
        // inside: a later check may get past it.
        Commit::Busy(_) | Commit::Unchecked => {
            drop(chains);
            if held.schedule.missed(now) {
                let held = slot.take();
                loader.retry.publish(None);
                drop(slot);
                give_up(loader, held, "a thread stayed inside it through every wait");
            } else {
                loader.retry.publish(Some(held));
            }
        }
        Commit::Failed => {
            drop(chains);
            let held = slot.take();
            loader.retry.publish(None);
            drop(slot);
            give_up(loader, held, "the code could not be written");
        }
    }
}

/// Skips every waiting patch of `held` for `reason`, turning off its owner,
/// with no lock held.
fn give_up(loader: &Loader, held: Option<Held>, reason: &str) {
    let Some(mut held) = held else {
        return;
    };
    let mut off = Vec::new();
    for (patch, verdict) in held.patches.iter().zip(&mut held.verdicts) {
        if *verdict != Verdict::Wait {
            continue;
        }
        *verdict = Verdict::Skip(reason.to_owned());
        crate::engine::code::skipped(loader, patch, reason, &mut off);
        // Off before it stops waiting, so its hooks are never let through.
        if let Some(record) = loader.mods.get(patch.owner.get()) {
            record.waiting.store(false, Ordering::Release);
        }
    }
    crate::engine::diag::record_patches(crate::engine::code::tally(&held.verdicts));
}
