//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::config::Config;
use crate::{Load, Off};

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Calls `handler` with the mod's settings of type `T`: the defaults
    /// now, the world's as each world loads, before
    /// [`WorldLoaded`](crate::events::WorldLoaded), and the defaults again
    /// as it stops. A change to the world's file applies at its next load;
    /// [`on_settings_change`](Self::on_settings_change) follows it while
    /// the world runs.
    ///
    /// The world's file is `<stem>-mod-settings/<id>/<file>.toml` beside its
    /// save, [`Config::FILE`] naming it: the loader writes it from
    /// [`Config::template`] as the world first loads, and adds the keys a
    /// newer `T` has and removes the ones it lacks as later worlds load. On
    /// a client in another's world, the values come from that world's
    /// server. A value that does not read takes its default, and a file
    /// [`Config::validate`] refuses reads as the defaults, each with a line
    /// in the log; settings never turn the mod off.
    ///
    /// Settings never change what the mod registers in init: install hooks
    /// and content there and let `handler` set the values they read.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the settings type, such as a second
    /// type of the mod with the same file.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::{AtomicU32, Ordering};
    ///
    /// /// Altar settings.
    /// #[enshwick::config]
    /// #[derive(Default)]
    /// struct Settings {
    ///     /// Altars allowed per player.
    ///     #[config(range = 1..=16)]
    ///     altars: u32,
    /// }
    ///
    /// static ALTARS: AtomicU32 = AtomicU32::new(0);
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_settings(|settings: &Settings| ALTARS.store(settings.altars, Ordering::Relaxed))
    /// }
    /// ```
    pub fn on_settings<T, F>(&mut self, handler: F) -> Result<(), Off>
    where
        T: Config + 'static,
        F: Fn(&T) + Send + Sync + 'static;

    /// Calls `handler` as [`on_settings`](Self::on_settings) does, and also
    /// each time the world's settings change while it runs: a person
    /// edited the file, the console or the Mods screen set a value, or the
    /// server of another's world sent new values. A changed file
    /// [`Config::validate`] refuses is skipped with a line in the log, and
    /// the values the mod has stay. For a mod whose values may change live.
    ///
    /// # Errors
    ///
    /// [`Off`] as for [`on_settings`](Self::on_settings).
    ///
    /// # Examples
    ///
    /// ```standalone_crate
    /// use std::sync::atomic::{AtomicU32, Ordering};
    ///
    /// /// Comfort settings.
    /// #[enshwick::config]
    /// #[derive(Default)]
    /// struct Settings {
    ///     /// Comfort per piece of furniture, times this factor.
    ///     multiplier: f32,
    /// }
    ///
    /// static MULTIPLIER: AtomicU32 = AtomicU32::new(0);
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_settings_change(|settings: &Settings| {
    ///         MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    ///     })
    /// }
    /// ```
    pub fn on_settings_change<T, F>(&mut self, handler: F) -> Result<(), Off>
    where
        T: Config + 'static,
        F: Fn(&T) + Send + Sync + 'static;
}
