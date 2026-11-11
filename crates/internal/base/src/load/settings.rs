use core::ffi::c_void;
use std::sync::Arc;

use crate::abi::{Event, Level, Status, Str};
use crate::config::{Config, ConfigError, reason};

use super::{BaseLoad, CHECK_WHY, Off};

impl BaseLoad {
    /// As `Load::on_settings`.
    pub fn on_settings<T, F>(&mut self, handler: F) -> Result<(), Off>
    where
        T: Config + 'static,
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.follow_settings(handler, false)
    }

    /// As `Load::on_settings_change`.
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
