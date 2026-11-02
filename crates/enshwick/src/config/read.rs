//! A world's settings file read as its type: what does not read takes its
//! default, and the file never turns a mod off.

use crate::abi::Level;
use crate::config::{Config, reason};

use super::reason::key_path;
use super::schema::remove_at;

/// Reads `text`, a settings file of `T`, logging through `log` each value
/// that takes its default or is held to its range; empty text, as while no
/// world runs, is the defaults. A value that does not read as its field's
/// type is taken out so its default applies, and a file that is no TOML
/// reads as the defaults.
pub(crate) fn read<T: Config>(text: &str, log: &dyn Fn(Level, &str)) -> T {
    if text.trim().is_empty() {
        return T::default();
    }
    let file = T::FILE;
    let mut table: toml::Table = match toml::from_str(text) {
        Ok(table) => table,
        Err(e) => {
            log(
                Level::WARN,
                &format!(
                    "{file}.toml: {}; the defaults apply",
                    reason(Some(text), &e)
                ),
            );
            return T::default();
        }
    };
    for line in T::schema().settle(&mut table) {
        log(Level::WARN, &format!("{file}.toml: {line}"));
    }
    // Each round takes one key out; a key it cannot name ends the reading.
    loop {
        let error = match toml::Value::Table(table.clone()).try_into::<T>() {
            Ok(settings) => return settings,
            Err(error) => error,
        };
        let why = reason(None, &error);
        let key = key_path(&error).map(|key| key.split('[').next().unwrap_or_default().to_owned());
        match key.filter(|key| !key.is_empty() && super::value_at(&table, key).is_some()) {
            Some(key) => {
                log(
                    Level::WARN,
                    &format!("{file}.toml: {why}; the default applies"),
                );
                remove_at(&mut table, &key);
            }
            None => {
                log(
                    Level::WARN,
                    &format!("{file}.toml: {why}; the defaults apply"),
                );
                return T::default();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::read;
    use crate::abi::Level;

    /// Comfort rules.
    #[crate::config]
    #[derive(Debug, PartialEq)]
    struct Comfort {
        /// The factor.
        #[config(range = 0.0..=4.0)]
        multiplier: f32,
        /// Shown or not.
        shown: bool,
    }

    impl Default for Comfort {
        fn default() -> Self {
            Self {
                multiplier: 1.0,
                shown: true,
            }
        }
    }

    /// Settings.
    #[crate::config]
    #[derive(Debug, PartialEq)]
    struct Settings {
        /// How many.
        #[config(range = 1..=16)]
        limit: u32,
        /// The mode.
        #[config(choices = ["auto", "on"])]
        mode: String,
        /// Per level.
        #[config(range = 1..=32)]
        per_level: Vec<u8>,
        /// The pin.
        #[config(secret)]
        pin: u32,
        /// A table of its own.
        #[config(group)]
        comfort: Comfort,
    }

    impl Default for Settings {
        fn default() -> Self {
            Self {
                limit: 4,
                mode: "auto".into(),
                per_level: vec![2, 4],
                pin: 0,
                comfort: Comfort::default(),
            }
        }
    }

    fn reading(text: &str) -> (Settings, Vec<String>) {
        let lines = RefCell::new(Vec::new());
        let settings = read::<Settings>(text, &|level, line| {
            assert_eq!(level, Level::WARN);
            lines.borrow_mut().push(line.to_owned());
        });
        (settings, lines.into_inner())
    }

    #[test]
    fn no_text_is_the_defaults_and_a_missing_key_its_default() {
        assert_eq!(reading(""), (Settings::default(), Vec::new()));
        let (settings, lines) = reading("limit = 9\n");
        assert_eq!(
            settings,
            Settings {
                limit: 9,
                ..Settings::default()
            }
        );
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn a_value_that_does_not_read_takes_its_default_with_a_line() {
        let (settings, lines) = reading(
            "limit = \"nine\"\nmode = \"off\"\nper_level = [2, 64]\npin = \"4815162342\"\n\
             [comfort]\nmultiplier = 9.0\nshown = 3\n",
        );
        assert_eq!(
            settings,
            Settings {
                per_level: vec![2, 32],
                comfort: Comfort {
                    multiplier: 4.0,
                    shown: true,
                },
                ..Settings::default()
            }
        );
        assert_eq!(
            lines,
            [
                "settings.toml: limit takes a whole number; the default applies",
                "settings.toml: mode is one of \"auto\", \"on\"; the default applies",
                "settings.toml: per_level[1] is outside 1 to 32; 32 applies",
                "settings.toml: pin takes a whole number; the default applies",
                "settings.toml: comfort.multiplier is outside 0 to 4; 4.0 applies",
                "settings.toml: comfort.shown takes true or false; the default applies",
            ]
        );
        assert!(lines.iter().all(|line| !line.contains("4815162342")));
    }

    #[test]
    fn a_value_only_serde_refuses_takes_its_default_too() {
        /// Odd.
        #[crate::config]
        #[derive(Debug, Default, PartialEq)]
        struct Odd {
            /// Small.
            small: u8,
            /// Kept.
            kept: u8,
        }
        let lines = RefCell::new(Vec::new());
        let odd = read::<Odd>("small = 300\nkept = 2\n", &|_, line| {
            lines.borrow_mut().push(line.to_owned());
        });
        assert_eq!(odd, Odd { small: 0, kept: 2 });
        assert_eq!(lines.borrow().len(), 1, "{lines:?}");
        assert!(lines.borrow()[0].starts_with("settings.toml: key small: "));
    }

    #[test]
    fn a_file_that_is_no_toml_reads_as_the_defaults() {
        let (settings, lines) = reading("limit = [\n");
        assert_eq!(settings, Settings::default());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("; the defaults apply"), "{lines:?}");
    }
}
