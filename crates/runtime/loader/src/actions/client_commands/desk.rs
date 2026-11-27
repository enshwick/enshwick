//! A desk for commands whose ask a draw of the loader's own interface acts
//! on: the ask is parked when the command is queued, handed to the draw one
//! command at a time, claimed by the draw before it acts, and answered with
//! a stage the command's own module names. A cancel takes the command back
//! at any point; once it did, the draw can no longer claim it.

use std::sync::Mutex;

use enshwick_base::Relock;

use super::Handoff;

/// A handed command waits for the draw.
pub(super) const WAITING: u8 = 1;
/// The draw claimed the command and acts on it in this frame.
const ACTING: u8 = u8::MAX - 1;

/// The asks of one kind of command, from their queue to their answer.
pub(super) struct Desk<A> {
    /// The command handed to the draw.
    handoff: Handoff,
    /// The handed command's ask, filled before its stage shows.
    handed: Mutex<Option<A>>,
    /// The asks of queued commands not yet handed, by command id.
    parked: Mutex<Vec<(u64, A)>>,
}

impl<A: Clone> Desk<A> {
    pub(super) const fn new() -> Self {
        Self {
            handoff: Handoff::new(),
            handed: Mutex::new(None),
            parked: Mutex::new(Vec::new()),
        }
    }

    /// Keeps command `id`'s ask until it is handed.
    pub(super) fn park(&self, id: u64, ask: A) {
        self.parked.relock().push((id, ask));
    }

    /// Hands command `id` to the draw; false while another is handed.
    pub(super) fn hand(&self, id: u64) -> bool {
        self.handoff.arm(id, WAITING, || {
            let mut parked = self.parked.relock();
            let ask = parked
                .iter()
                .position(|(parked, _)| *parked == id)
                .map(|at| parked.swap_remove(at).1);
            *self.handed.relock() = ask;
        })
    }

    /// The stage command `id` stands at: [`WAITING`], claimed, or the
    /// draw's answer; `None` while it is not handed.
    pub(super) fn stage(&self, id: u64) -> Option<u8> {
        self.handoff.stage(id)
    }

    /// Forgets command `id`, handed or still parked.
    pub(super) fn release(&self, id: u64) {
        self.parked.relock().retain(|(parked, _)| *parked != id);
        if self.handoff.release(id).is_some() {
            *self.handed.relock() = None;
        }
    }

    /// The command handed to the draw and its ask, while it waits for an
    /// answer; one atomic load while none is.
    pub(super) fn asked(&self) -> Option<(u64, A)> {
        let (id, WAITING) = self.handoff.current()? else {
            return None;
        };
        let ask = self.handed.relock().clone()?;
        Some((id, ask))
    }

    /// Claims command `id` for the draw before it acts; false once a cancel
    /// took it back, so a cancelled command never acts.
    pub(super) fn claim(&self, id: u64) -> bool {
        self.handoff.advance(id, WAITING, ACTING)
    }

    /// Gives claimed command `id` back to wait for the next frame.
    pub(super) fn retry(&self, id: u64) {
        let _waits = self.handoff.advance(id, ACTING, WAITING);
    }

    /// The draw's answer `stage` to claimed command `id`, neither
    /// [`WAITING`] nor the claim's own.
    pub(super) fn answer(&self, id: u64, stage: u8) {
        debug_assert!(stage != WAITING && stage != ACTING, "an answer's stage");
        let _moved = self.handoff.advance(id, ACTING, stage);
    }
}
