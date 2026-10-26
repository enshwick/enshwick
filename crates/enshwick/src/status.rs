//! Status lines: what a mod reports about itself in the loader's status
//! summary, `logs/enshwick/status.log`, written every few minutes and on
//! `enshwick status` on the [console](crate::console).
//!
//! The loader fills the summary with what it knows of every mod: version,
//! whether it runs, why it was turned off, its hooks and their calls. A mod
//! adds its own counters with [`Load::add_status`]: a closure returning a few
//! lines, called only when the summary is written, so keeping the numbers
//! (atomics, say) is the only cost while the game runs.

use core::ffi::c_void;
use std::cell::RefCell;

use crate::abi::Str;
use crate::{Load, Off};

thread_local! {
    /// The last status text, alive until the next source runs on this thread.
    static TEXT: RefCell<String> = const { RefCell::new(String::new()) };
}

unsafe extern "system" fn run_source<F>(user: *mut c_void, out: *mut Str)
where
    F: Fn() -> String,
{
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let source = unsafe { &*user.cast::<F>() };
    let text = crate::host::guard("panic in a status source", String::new, source);
    if out.is_null() {
        return;
    }
    TEXT.with_borrow_mut(|kept| {
        *kept = text;
        // SAFETY: the loader's out pointer; the text lives in this thread's
        // buffer until the next source runs here, after the loader copied it.
        unsafe { out.write(Str::new(kept)) };
    });
}

impl Load<'_> {
    /// Adds `source` to the loader's status summary: whenever the summary is
    /// written, the loader calls it and shows each line it returns under the
    /// mod. It runs on a loader thread or the game thread, never inside a
    /// hook, and should only read what the mod keeps, such as atomics.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the source, as after init.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::{AtomicU64, Ordering};
    ///
    /// static MERGED: AtomicU64 = AtomicU64::new(0);
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_status(|| format!("{} zones merged", MERGED.load(Ordering::Relaxed)))
    /// }
    /// ```
    pub fn add_status<F>(&mut self, source: F) -> Result<(), Off>
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            source,
            |user| (host.api().status)(host.handle(), run_source::<F>, user),
            "status source",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_writes_its_lines_and_keeps_them_until_it_runs_again() {
        let source = || "3 zones merged\n1 seam".to_owned();
        let user = core::ptr::from_ref(&source).cast_mut().cast::<c_void>();
        let mut out = Str::EMPTY;
        // SAFETY: the source as user and a writable out pointer.
        unsafe { run_source_of(&source, user, &raw mut out) };
        // SAFETY: valid until the next source on this thread.
        assert_eq!(unsafe { out.as_str() }, Some("3 zones merged\n1 seam"));
    }

    /// Calls `run_source` with the type of `source`.
    ///
    /// # Safety
    ///
    /// - `user` points at `source`, alive for the call.
    /// - `out` is null or points at a writable, aligned [`Str`] that no other
    ///   thread touches during the call.
    unsafe fn run_source_of<F: Fn() -> String>(_source: &F, user: *mut c_void, out: *mut Str) {
        // SAFETY: `run_source::<F>` needs `user` to be an `F` and `out` null
        // or a writable `Str`; this function's `# Safety` bullets give
        // `user` pointing at `source`, an `F`, and such an `out`.
        unsafe { run_source::<F>(user, out) };
    }
}
