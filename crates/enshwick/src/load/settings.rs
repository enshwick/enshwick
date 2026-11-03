use core::ffi::c_void;
use std::sync::Arc;

use crate::abi::{Event, Level, Status, Str};
use crate::config::{Config, ConfigError, reason};

use super::{CHECK_WHY, Load, Off};

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
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.follow_settings(handler, false)
    }

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
    /// ```
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
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.follow_settings(handler, true)
    }

    /// Declares `T` to the loader, hands `handler` the defaults, and follows
    /// world loads, and changes too when `live`.
    fn follow_settings<T, F>(&mut self, handler: F, live: bool) -> Result<(), Off>
    where
        T: Config + 'static,
        F: Fn(&T) + Send + Sync + 'static,
    {
        let host = self.host();
        host.config_schema(T::FILE, &T::schema().to_text(), &T::template())?;
        // A second type or handler on the file keeps the first check.
        let _checked = (host.api().config_check)(
            host.handle(),
            Str::new(T::FILE),
            check_text::<T>,
            core::ptr::null_mut(),
        );
        handler(&T::default());
        let handler = Arc::new(handler);
        let on_world = Arc::clone(&handler);
        self.on_raw_event(Event::SETTINGS_REPLACED, move |_| {
            if let Some(settings) = world_settings::<T>(true) {
                on_world(&settings);
            }
        })?;
        if live {
            self.on_raw_event(Event::SETTINGS_CHANGED, move |_| {
                if let Some(settings) = world_settings::<T>(false) {
                    handler(&settings);
                }
            })?;
        }
        Ok(())
    }
}

/// The settings of `T` in force, read as [`crate::config::read`] reads
/// them; when [`Config::validate`] refuses them, the defaults as a world
/// loads (`loading`) and `None` for a change, so the values the mod has
/// stay.
fn world_settings<T: Config>(loading: bool) -> Option<T> {
    let host = crate::host()?;
    let log = |level: Level, line: &str| host.log(level, line);
    let text = match host.settings_text(T::FILE) {
        Ok(text) => text,
        Err(off) => {
            log(Level::WARN, &format!("{off}; the defaults apply"));
            return loading.then(T::default);
        }
    };
    let settings: T = crate::config::read(&text, &log);
    match settings.validate() {
        Ok(()) => Some(settings),
        Err(why) if loading => {
            log(
                Level::WARN,
                &format!("{}.toml: {why}; the defaults apply", T::FILE),
            );
            Some(T::default())
        }
        Err(why) => {
            log(
                Level::WARN,
                &format!("{}.toml: {why}; the values in force stay", T::FILE),
            );
            None
        }
    }
}

/// Reads and validates `text` as the mod's settings `T`, for the loader's
/// check before it writes a change.
pub(super) unsafe extern "system" fn check_text<T: Config>(
    text: Str,
    why: *mut Str,
    _user: *mut c_void,
) -> Status {
    crate::host::guard(
        "panic in the settings check",
        || Status::REFUSED,
        || {
            // SAFETY: the loader passes the text for the call.
            let text = unsafe { text.as_str() }.unwrap_or("");
            let checked = toml::from_str::<T>(text)
                .map_err(|e| ConfigError::Parse(reason(Some(text), &e)))
                .and_then(|settings| settings.validate());
            let Err(reason) = checked else {
                return Status::OK;
            };
            CHECK_WHY.with_borrow_mut(|kept| {
                *kept = reason.to_reason();
                if !why.is_null() {
                    // SAFETY: the loader's out pointer; the reason stays in
                    // this thread's `CHECK_WHY` until its next check.
                    unsafe { why.write(Str::new(kept)) };
                }
            });
            Status::REFUSED
        },
    )
}
