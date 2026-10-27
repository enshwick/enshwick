use core::ffi::c_void;

use crate::abi::{Event, Status, Str};
use crate::config::{Config, ConfigError, Stage, reason};
use crate::host::Host;
use crate::locks::Relock;

use super::{CHECK_WHY, Kept, Load, Off, RESTART, STARTED, keep, with_server};

impl Load<'_> {
    /// The mod's typed settings: `mod-configs/<id>.toml`, written from
    /// [`Config::template`] when missing, parsed and checked with
    /// [`Config::validate`].
    ///
    /// On a server, the fields marked `#[config(sync)]` are reported to the
    /// loader, which advertises them to players; see
    /// [`on_config_change`](Self::on_config_change) for the client's side.
    ///
    /// # Errors
    ///
    /// [`Off`] when the file does not parse into `T` or a value is refused.
    ///
    /// # Examples
    ///
    /// ```
    /// /// Altar settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
    /// struct Settings {
    ///     /// Altars allowed at each Flame level.
    ///     altars: Vec<u8>,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<Settings, enshwick::Off> {
    ///     load.config()
    /// }
    /// ```
    pub fn config<T: Config>(&mut self) -> Result<T, Off> {
        self.host.config_schema(&T::schema().to_text());
        self.config_check::<T>();
        read_config(self.host, |settings: &mut T| {
            keep(&STARTED, settings, Stage::Startup);
            keep(&RESTART, settings, Stage::Restart);
        })
    }

    /// Calls `handler` with the mod's settings each time `mod-configs/<id>.toml`
    /// changes, after the console or the Mods screen sets one of them, and as a
    /// world loads or stops; the thread is not fixed. A file that no longer
    /// parses or validates is logged and skipped, and the running values stay.
    /// `startup` values change only as a world loads or stops, and `restart`
    /// values never: `handler` gets the ones in force.
    ///
    /// On a client, it also runs when the join values change: on joining a
    /// server, `handler` gets the settings with the server's
    /// [`synced`](Config::synced) fields over the file's (the file stays as
    /// it is), and on joining a server without them, the file's values again.
    /// A mod with join values applies them here; without a handler it runs
    /// with the file's values until the next start.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader cannot watch settings.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::{AtomicU32, Ordering};
    ///
    /// /// Comfort settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
    /// struct Settings {
    ///     /// Comfort per piece of furniture, times this factor.
    ///     multiplier: f32,
    /// }
    ///
    /// static MULTIPLIER: AtomicU32 = AtomicU32::new(0);
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let settings: Settings = load.config()?;
    ///     MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    ///     load.on_config_change(|settings: Settings| {
    ///         MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    ///     })
    /// }
    /// ```
    pub fn on_config_change<T, F>(&mut self, handler: F) -> Result<(), Off>
    where
        T: Config,
        F: Fn(T) + Send + Sync + 'static,
    {
        let handler = std::sync::Arc::new(handler);
        let on_world = std::sync::Arc::clone(&handler);
        self.on_raw_event(Event::SETTINGS_REPLACED, move |_| {
            let Some(host) = crate::host() else {
                return;
            };
            let world = |settings: &mut T| {
                hold(&RESTART, settings, Stage::Restart);
                keep(&STARTED, settings, Stage::Startup);
            };
            match read_config::<T>(host, world) {
                Ok(settings) => {
                    on_world(settings);
                    crate::info!("the world's settings apply");
                }
                Err(off) => crate::warn!("the world's settings are skipped: {off}"),
            }
        })?;
        self.on_raw_event(Event::SETTINGS_CHANGED, move |_| {
            let Some(host) = crate::host() else {
                return;
            };
            let running = |settings: &mut T| {
                hold(&RESTART, settings, Stage::Restart);
                hold(&STARTED, settings, Stage::Startup);
            };
            match read_config::<T>(host, running) {
                Ok(settings) => {
                    handler(settings);
                    crate::info!("settings reloaded");
                }
                Err(off) => crate::warn!("settings change skipped, the running values stay: {off}"),
            }
        })
    }
}

/// Puts the values at `stage` in force back over `settings`, noting the
/// changes that wait.
pub(super) fn hold<T: Config>(kept: &Kept, settings: &mut T, stage: Stage) {
    let kept = kept.get_or_init(std::sync::Mutex::default).relock().clone();
    let (Some(kept), Some(now)) = (kept, settings.values(stage)) else {
        return;
    };
    if kept == now {
        return;
    }
    let waits = if stage == Stage::Restart {
        "the game starts again"
    } else {
        "the world loads"
    };
    match settings.merge_values(stage, &kept) {
        Ok(()) => crate::info!(
            "settings that take effect when {waits} changed: {}",
            crate::config::changed_keys(&kept, &now)
        ),
        Err(why) => crate::warn!("{} settings not kept: {why}", stage.name()),
    }
}

/// Parses and validates `text` as the mod's settings, for the loader's
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
                *kept = reason.to_string();
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

/// Reads the file's settings, puts the values in force over them with
/// `stages`, reports the join values in force and, while this client is
/// joined, puts the server's over them: a `startup, sync` value comes from
/// the server even where `stages` held this side's.
pub(super) fn read_config<T: Config>(host: Host, stages: impl FnOnce(&mut T)) -> Result<T, Off> {
    let text = host.settings_text(&T::template())?;
    let mut table: toml::Table = toml::from_str(&text)
        .map_err(|e| Off::new(format!("settings: {}", reason(Some(&text), &e))))?;
    for held in T::schema().clamp(&mut table) {
        crate::warn!("settings: {held}");
    }
    let mut settings: T = toml::Value::Table(table)
        .try_into()
        .map_err(|e: toml::de::Error| Off::new(format!("settings: {}", reason(None, &e))))?;
    settings
        .validate()
        .map_err(|why| Off::new(format!("settings: {why}")))?;
    stages(&mut settings);
    let Some(server) = settings.synced().and_then(|local| host.sync(&local)) else {
        return Ok(settings);
    };
    match with_server(&mut settings, &server) {
        Ok(()) => crate::info!("the server's join values apply over the settings file: {server}"),
        Err(why) => crate::warn!("the server's join values are refused, the file's apply: {why}"),
    }
    Ok(settings)
}
