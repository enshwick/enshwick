//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::{Load, Off};

#[enshwick_macros::forward(base)]
#[crate::script(status)]
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
    /// # extern crate enshwick_core as enshwick;
    /// use std::sync::atomic::{AtomicU64, Ordering};
    ///
    /// static MERGED: AtomicU64 = AtomicU64::new(0);
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_status(|| format!("{} zones merged", MERGED.load(Ordering::Relaxed)))
    /// }
    /// ```
    #[script]
    pub fn add_status<F>(&mut self, source: F) -> Result<(), Off>
    where
        F: Fn() -> String + Send + Sync + 'static;

    /// Runs `init`, the setup of one optional part of the mod, and reports
    /// it to the loader by `name`: on, or off with the reason `init` gave.
    /// The log, the status summary and the Mods screen show it, a reason
    /// from [`Off::text`] in the reader's language, and the mod keeps
    /// running either way.
    ///
    /// Everything registered before `init` fails stays, so `init` looks up
    /// what the part needs before it links hooks or adds content.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     if let Some(glow) = load.feature("glow", |load| load.symbol("lantern.glow")) {
    ///         let _ = glow;
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn feature<T>(
        &mut self,
        name: &str,
        init: impl FnOnce(&mut Self) -> Result<T, Off>,
    ) -> Option<T>;
}
