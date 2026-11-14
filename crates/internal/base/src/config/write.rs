use crate::abi::{Status, Str};
use crate::host::Host;

use super::Config;

/// Writes `value` as the setting `key` of the running world's settings file
/// of `T`, as the console and the Mods screen do, for a value the mod makes
/// itself, such as a password it generates at the first start.
///
/// The value passes the field's range and choices and the type's
/// [`validate`](Config::validate), goes into
/// `<stem>-mod-settings/<id>/<file>.toml` with the file's comments kept, and
/// raises a settings change as an edit does, so
/// [`Load::on_settings_change`](../struct.Load.html#method.on_settings_change)
/// hears it at once and [`Load::on_settings`](../struct.Load.html#method.on_settings)
/// at the world's next load. A `secret` field takes a value too, and the log
/// never shows it. `key` is the field's key in the file, dotted for a field
/// of a group (`rcon.password`). Any thread while a world runs; writes wait
/// for each other, and a write from a change handler raises the change
/// again. One from inside `validate`, or nested more than four deep through
/// change handlers, is refused.
///
/// # Errors
///
/// [`SetSettingError`]: no world runs here, `T` or `key` is not the mod's,
/// the setting does not take the value, or the check refused it or the file
/// did not write. The file then stays as it was, and the mod's log says why.
///
/// # Examples
///
/// ```standalone_crate
/// use enshwick::config::{self, SetSettingError};
///
/// /// Remote console settings.
/// #[enshwick::config]
/// #[derive(Default)]
/// struct Settings {
///     /// The password; the first start writes one when it is empty.
///     #[config(secret)]
///     password: String,
/// }
///
/// fn first_start(settings: &Settings, generated: &str) -> Result<(), SetSettingError> {
///     if settings.password.is_empty() {
///         config::set::<Settings>("password", generated)?;
///     }
///     Ok(())
/// }
/// ```
pub fn set<T: Config>(key: &str, value: impl Into<toml::Value>) -> Result<(), SetSettingError> {
    let host = crate::host().ok_or(SetSettingError::NoWorld)?;
    write(&host, T::FILE, key, &value.into())
}

/// The settings writes one thread may nest in one another through change
/// handlers; one more is refused, as a handler that writes each time it
/// hears a change would never end.
#[doc(hidden)]
pub const MAX_NESTED_WRITES: usize = 4;

/// Writes `value` as `key` of `host`'s settings file `file`.
pub(super) fn write(
    host: &Host,
    file: &str,
    key: &str,
    value: &toml::Value,
) -> Result<(), SetSettingError> {
    let value = value.to_string();
    let status = (host.api().config_set)(
        host.handle(),
        Str::new(file),
        Str::new(key),
        Str::new(&value),
    );
    crate::load::outcome::<SetSettingError>(status)
}

/// Why [`set`] wrote nothing; the file stays as it was and the mod's log
/// says why.
///
/// # Examples
///
/// ```
/// use enshwick::config::SetSettingError;
///
/// let why = SetSettingError::NoWorld;
/// assert_eq!(why.to_string(), "no world runs here, and settings belong to each world");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SetSettingError {
    /// No world runs in this process, as on a client in another's world or
    /// before a world loads: settings belong to each world.
    #[error("no world runs here, and settings belong to each world")]
    NoWorld,
    /// The mod's settings have no such file or key.
    #[error("the mod's settings have no such key")]
    NoSetting,
    /// The setting does not take the value: another type, or outside its
    /// range or choices.
    #[error("the setting does not take the value")]
    BadValue,
    /// The settings type's check refused the file with the value, the file
    /// did not read or write, or the write came from inside the check or
    /// nested too deep through change handlers.
    #[error("the settings were not written: the check refused them or the file did not write")]
    Refused,
}

impl crate::load::StatusError for SetSettingError {
    fn of(status: Status) -> Self {
        match status {
            Status::UNAVAILABLE => Self::NoWorld,
            Status::MISSING => Self::NoSetting,
            Status::BAD_ARGS => Self::BadValue,
            _ => Self::Refused,
        }
    }
}

impl From<SetSettingError> for crate::Off {
    /// The error's message as the reason.
    fn from(error: SetSettingError) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_status_of_the_write_reads_as_its_outcome() {
        assert_eq!(crate::load::outcome::<SetSettingError>(Status::OK), Ok(()));
        assert_eq!(
            crate::load::outcome::<SetSettingError>(Status::UNAVAILABLE),
            Err(SetSettingError::NoWorld)
        );
        assert_eq!(
            crate::load::outcome::<SetSettingError>(Status::MISSING),
            Err(SetSettingError::NoSetting)
        );
        assert_eq!(
            crate::load::outcome::<SetSettingError>(Status::BAD_ARGS),
            Err(SetSettingError::BadValue)
        );
        for status in [Status::REFUSED, Status::FAULTED, Status(99)] {
            assert_eq!(
                crate::load::outcome::<SetSettingError>(status),
                Err(SetSettingError::Refused)
            );
        }
    }
}
