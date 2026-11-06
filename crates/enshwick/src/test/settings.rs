//! Settings: the running world's settings files and the mod's checks.

use crate::abi::Level;
use crate::abi::Str;

use super::FakeHost;
use super::Installed;
use super::table;

impl FakeHost {
    /// The text of the running world's `settings.toml` for the mod, which
    /// the mod reads as each world loads: raise
    /// [`events::SettingsReplaced`](crate::events::SettingsReplaced) after
    /// [`install`](Self::install). Without it the world's file is empty
    /// and the mod runs on its defaults.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    #[must_use]
    pub fn settings(self, text: &str) -> Self {
        self.settings_file("settings", text)
    }

    /// The text of the running world's `<file>.toml` for the mod, as
    /// [`settings`](Self::settings) sets `settings.toml`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .settings_file("altars", "per_level = [2, 4]\n")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn settings_file(mut self, file: &str, text: &str) -> Self {
        self.settings.insert(file.to_owned(), text.to_owned());
        self
    }

    /// The path `Load::data_dir` and `World::data_dir` return while a world
    /// runs, that is once [`world_key`](Self::world_key) is set;
    /// `world0-mod-data/test` by default. The fake creates nothing there.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .world_key("world0")
    ///     .data_dir("target/world0-mod-data/bonus")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn data_dir(mut self, path: &str) -> Self {
        path.clone_into(&mut self.data_dir);
        self
    }

    /// The path `Load::mod_dir` returns; `mods/test` by default. The fake
    /// creates nothing there. Where the loader lists every mod, the calling
    /// one included, the fake lists only what [`FakeHost::loaded`] adds, so a
    /// test of `Load::export_dir` adds the mod with its file in this folder.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().mod_dir("target/mods/bonus").install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn mod_dir(mut self, path: &str) -> Self {
        path.clone_into(&mut self.mod_dir);
        self
    }

    /// The most detailed level the loader keeps for the mod's lines.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn log_level(mut self, level: Level) -> Self {
        self.log_level = level;
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Replaces the text of the running world's `<file>.toml` for the mod,
    /// as a person's edit or the next world would; raise
    /// [`events::SettingsChanged`](crate::events::SettingsChanged) for a
    /// change while the world runs, or
    /// [`events::SettingsReplaced`](crate::events::SettingsReplaced) for a
    /// world that loads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events;
    ///
    /// let loader = enshwick::test::FakeHost::new().settings("limit = 7\n").install();
    /// loader.set_settings("settings", "limit = 9\n");
    /// assert_eq!(loader.raise::<events::SettingsChanged>(&()), 0);
    /// ```
    pub fn set_settings(&self, file: &str, text: &str) {
        table::with((), |s| {
            s.host.settings.insert(file.to_owned(), text.to_owned());
        });
    }

    /// The schema of the mod's settings file `file` as the mod handed it
    /// the loader, which [`Load::on_settings`](crate::Load::on_settings)
    /// does; `None` before.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.config_schema("settings").is_none());
    /// ```
    #[must_use]
    pub fn config_schema(&self, file: &str) -> Option<crate::config::Schema> {
        table::with(None, |s| s.config_schemas.get(file).cloned())
            .and_then(|text| crate::config::Schema::parse(&text).ok())
    }

    /// Runs the mod's check of a new text of its settings file `file`, as
    /// the loader does before it writes a change: `Ok` when the mod accepts
    /// `text`, else its reason; `Ok` for a file without a check.
    ///
    /// # Errors
    ///
    /// [`SettingsRefused`] with the mod's reason for refusing `text`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.check_settings("settings", "x = 1"), Ok(()));
    /// ```
    pub fn check_settings(&self, file: &str, text: &str) -> Result<(), SettingsRefused> {
        let Some((callback, user)) = table::with(None, |s| s.config_checks.get(file).copied())
        else {
            return Ok(());
        };
        let mut why = Str::EMPTY;
        // SAFETY: the mod's check with its own pointer; the text lives for
        // the call.
        let status = unsafe {
            callback(
                Str::new(text),
                &raw mut why,
                core::ptr::with_exposed_provenance_mut(user),
            )
        };
        if status.is_ok() {
            return Ok(());
        }
        // SAFETY: the reason stays valid until the mod's next call.
        let reason = unsafe { why.as_str() }.unwrap_or("refused").to_owned();
        Err(SettingsRefused { reason })
    }
}

/// A mod's check refused a settings text, as [`Installed::check_settings`]
/// reports it.
///
/// # Examples
///
/// See [`Installed::check_settings`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct SettingsRefused {
    /// The mod's reason.
    pub reason: String,
}
