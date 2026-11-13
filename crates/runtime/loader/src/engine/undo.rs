//! A change to game memory taken back when its work does not finish.

/// Runs its undo when dropped, also while a panic unwinds, unless kept: a
/// detour that changes game memory before work that may fail puts it back.
pub(crate) struct Undo<F: FnOnce()> {
    undo: Option<F>,
}

impl<F: FnOnce()> Undo<F> {
    /// A guard that runs `undo` when dropped.
    pub(crate) const fn new(undo: F) -> Self {
        Self { undo: Some(undo) }
    }

    /// Leaves the change in place: the undo never runs.
    pub(crate) fn keep(mut self) {
        self.undo = None;
    }
}

impl<F: FnOnce()> Drop for Undo<F> {
    fn drop(&mut self) {
        if let Some(undo) = self.undo.take() {
            undo();
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::Undo;

    #[test]
    fn work_that_ends_unkept_or_panics_is_undone_and_kept_work_is_not() {
        let undone = Cell::new(0);
        drop(Undo::new(|| undone.set(undone.get() + 1)));
        assert_eq!(undone.get(), 1);
        Undo::new(|| undone.set(undone.get() + 1)).keep();
        assert_eq!(undone.get(), 1, "kept");
        let panicked = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            let _undo = Undo::new(|| undone.set(undone.get() + 1));
            panic!("work that fails part way");
        }));
        assert!(panicked.is_err());
        assert_eq!(undone.get(), 2, "undone while the panic unwinds");
    }
}
