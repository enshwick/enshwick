//! The notices a client waits to raise: how many wait and how many one step
//! raises, apart from the game so it is tested everywhere.

/// Text tags the loader cycles through, so a notice still on screen keeps
/// its text while the next one is written.
pub(crate) const SLOTS: usize = 8;
/// Notices waiting for the client's next step at most.
pub(crate) const MAX_WAITING: usize = 16;

/// Notices waiting for the client's next step: entity and text.
pub(crate) type Waiting = Vec<(u32, String)>;

/// What [`push`] did with a notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pushed {
    Queued,
    /// Dropped by a full queue, the first of a burst.
    FirstDrop,
    Dropped,
}

/// Queues a notice unless [`MAX_WAITING`] wait already; `dropping` holds
/// whether a drop was reported since the last notice queued.
#[must_use]
pub(crate) fn push(waiting: &mut Waiting, dropping: &mut bool, entity: u32, text: &str) -> Pushed {
    if waiting.len() < MAX_WAITING {
        waiting.push((entity, text.to_owned()));
        *dropping = false;
        Pushed::Queued
    } else if core::mem::replace(dropping, true) {
        Pushed::Dropped
    } else {
        Pushed::FirstDrop
    }
}

/// The notices one step raises: at most one per text slot, so no notice
/// writes over the text of another raised in the same step; the rest wait
/// for the next step.
pub(crate) fn step_batch(waiting: &mut Waiting) -> Waiting {
    waiting.drain(..waiting.len().min(SLOTS)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_raises_one_notice_per_slot_and_the_rest_wait_in_order() {
        let mut waiting: Waiting = (0..u32::try_from(MAX_WAITING).unwrap())
            .map(|n| (n, format!("notice {n}")))
            .collect();
        let first = step_batch(&mut waiting);
        assert_eq!(first.len(), SLOTS);
        assert_eq!(first[0].1, "notice 0");
        assert_eq!(waiting.len(), MAX_WAITING - SLOTS);
        assert_eq!(waiting[0].1, format!("notice {SLOTS}"));
        let second = step_batch(&mut waiting);
        assert_eq!(second.len(), MAX_WAITING - SLOTS);
        assert!(waiting.is_empty());
        assert!(step_batch(&mut waiting).is_empty());
    }

    #[test]
    fn a_full_queue_reports_the_first_drop_of_a_burst_only() {
        let mut waiting = Waiting::new();
        let mut dropping = false;
        for n in 0..u32::try_from(MAX_WAITING).unwrap() {
            assert_eq!(push(&mut waiting, &mut dropping, n, "x"), Pushed::Queued);
        }
        assert_eq!(
            push(&mut waiting, &mut dropping, 99, "x"),
            Pushed::FirstDrop
        );
        assert_eq!(push(&mut waiting, &mut dropping, 99, "x"), Pushed::Dropped);
        let _shown = step_batch(&mut waiting);
        assert_eq!(push(&mut waiting, &mut dropping, 7, "x"), Pushed::Queued);
        while waiting.len() < MAX_WAITING {
            let _queued = push(&mut waiting, &mut dropping, 7, "x");
        }
        assert_eq!(
            push(&mut waiting, &mut dropping, 99, "x"),
            Pushed::FirstDrop,
            "a new burst reports again"
        );
    }
}
