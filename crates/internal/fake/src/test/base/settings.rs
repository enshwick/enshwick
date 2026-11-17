//! Settings: the running world's settings files and the mod's checks.

use std::path::PathBuf;

use enshwick_core::abi::Str;
use enshwick_core::abi::{Level, ModHandle};

use crate::test::FakeHost;
use crate::test::Installed;
use crate::test::base::BasePresets;
use crate::test::table;

impl FakeHost {
    /// The text of the running world's `settings.toml` for the mod, which
    /// the mod reads as each world loads: raise
    /// [`events::SettingsReplaced`](enshwick_core::events::SettingsReplaced) after
    /// [`install`](Self::install). Without it the world's file is empty
    /// and the mod runs on its defaults.
    ///
    /// # Examples
    ///
    /// See the [module](crate::test).
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
        self.base.settings.insert(file.to_owned(), text.to_owned());
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
        path.clone_into(&mut self.base.data_dir);
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
        path.clone_into(&mut self.base.mod_dir);
        self
    }

    /// A file `name` holding `contents` in the mod's own folder, as a
    /// developer tool keeps its `settings.toml` beside it. With any such
    /// file the folder is one the install makes in the temp folder, in place
    /// of [`mod_dir`](Self::mod_dir): the files are written once the install
    /// takes its turn and removed when the [`Installed`] drops, so tests on
    /// threads of one process never read each other's files.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .mod_file("settings.toml", "enabled = true\n")
    ///     .install();
    /// let text = std::fs::read_to_string(loader.mod_dir().join("settings.toml"))?;
    /// assert_eq!(text, "enabled = true\n");
    /// # Ok::<(), std::io::Error>(())
    /// ```
    #[must_use]
    pub fn mod_file(mut self, name: &str, contents: impl Into<Vec<u8>>) -> Self {
        self.base.mod_files.insert(name.to_owned(), contents.into());
        self
    }

    /// The most detailed level the loader keeps for the mod's lines.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn log_level(mut self, level: Level) -> Self {
        self.base.log_level = level;
        self
    }

    /// The most detailed level the loader keeps for the lines of the data
    /// pack `pack`, as `enshwick.toml` sets it per mod id; the other handles
    /// keep [`log_level`](Self::log_level).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Level, ModHandle};
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .pack_log_level(ModHandle(2), Level::DEBUG)
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn pack_log_level(mut self, pack: ModHandle, level: Level) -> Self {
        self.base
            .pack_log_levels
            .retain(|(other, _)| *other != pack);
        self.base.pack_log_levels.push((pack, level));
        self
    }

    /// A data pack the loader runs with handle `pack`, of `side` as its
    /// `pack.toml` says. Its calls are refused by that side as the loader
    /// refuses them: a rate or value clients read, a crafting prop and a
    /// net channel need [`Side::BOTH`](enshwick_core::abi::Side::BOTH). A
    /// handle that is neither the mod's nor a pack named here counts as a
    /// pack of side server.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ModHandle, Side};
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .pack(ModHandle(2), Side::BOTH)
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn pack(mut self, pack: ModHandle, side: enshwick_core::abi::Side) -> Self {
        self.base.pack_sides.retain(|(other, _)| *other != pack);
        self.base.pack_sides.push((pack, side));
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
    /// [`events::SettingsChanged`](enshwick_core::events::SettingsChanged) for a
    /// change while the world runs, or
    /// [`events::SettingsReplaced`](enshwick_core::events::SettingsReplaced) for a
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
            s.host
                .base
                .settings
                .insert(file.to_owned(), text.to_owned());
        });
    }

    /// The text of the running world's `<file>.toml` for the mod: what
    /// [`set_settings`](Self::set_settings) or the mod's own
    /// `config::set` wrote last; `None` before either.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().settings("limit = 7\n").install();
    /// assert_eq!(loader.settings_text("settings").as_deref(), Some("limit = 7\n"));
    /// ```
    #[must_use]
    pub fn settings_text(&self, file: &str) -> Option<String> {
        table::with(None, |s| s.host.base.settings.get(file).cloned())
    }

    /// The folder `Load::mod_dir` names to the mod: the install's own with
    /// [`FakeHost::mod_file`], else the one [`FakeHost::mod_dir`] set.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::mod_file`].
    #[must_use]
    pub fn mod_dir(&self) -> PathBuf {
        table::with(PathBuf::new(), |s| PathBuf::from(&s.host.base.mod_dir))
    }

    /// Makes `level` the data pack `pack`'s most detailed log level, as an
    /// edit of the loader's settings would; raise
    /// [`events::LoaderSettingsChanged`](enshwick_core::events::LoaderSettingsChanged)
    /// for the pack to read it, as the loader does after the edit.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Level, ModHandle};
    /// use enshwick::events;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_pack_log_level(ModHandle(2), Level::DEBUG);
    /// assert_eq!(loader.raise::<events::LoaderSettingsChanged>(&()), 0);
    /// ```
    pub fn set_pack_log_level(&self, pack: ModHandle, level: Level) {
        table::with((), |s| {
            s.host
                .base
                .pack_log_levels
                .retain(|(other, _)| *other != pack);
            s.host.base.pack_log_levels.push((pack, level));
        });
    }

    /// Makes `level` the most detailed log level of every handle without a
    /// level of its own, as an edit of the loader's settings would; raise
    /// [`events::LoaderSettingsChanged`](enshwick_core::events::LoaderSettingsChanged)
    /// for the mod to read it, as the loader does after the edit.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Level;
    /// use enshwick::events;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_log_level(Level::DEBUG);
    /// assert_eq!(loader.raise::<events::LoaderSettingsChanged>(&()), 0);
    /// ```
    pub fn set_log_level(&self, level: Level) {
        table::with((), |s| s.host.base.log_level = level);
    }

    /// The schema of the mod's settings file `file` as the mod handed it
    /// the loader, which [`Load::on_settings`](enshwick_core::Load::on_settings)
    /// does; `None` before.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.config_schema("settings").is_none());
    /// ```
    #[must_use]
    pub fn config_schema(&self, file: &str) -> Option<enshwick_core::config::Schema> {
        table::with(None, |s| s.base.config_schemas.get(file).cloned())
            .and_then(|text| enshwick_core::config::Schema::parse(&text).ok())
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
        let Some(check) = table::with(None, |s| s.base.config_checks.get(file).copied()) else {
            return Ok(());
        };
        let mut why = Str::EMPTY;
        // SAFETY: the mod's check with its own pointer; the text lives for
        // the call.
        let status =
            check.run(|callback, user| unsafe { callback(Str::new(text), &raw mut why, user) });
        if status.is_ok() {
            return Ok(());
        }
        // SAFETY: the reason stays valid until the mod's next call.
        let reason = unsafe { why.as_str() }.unwrap_or("refused").to_owned();
        Err(SettingsRefused { reason })
    }
}

/// The folder of the installed fake's mod files: one per process, since one
/// fake at a time is installed in it.
fn own_mod_dir() -> PathBuf {
    std::env::temp_dir().join(format!("enshwick-fake-mod-{}", std::process::id()))
}

/// Writes the mod files of `base` into the install's own folder, left
/// empty first, and names it the mod's folder; nothing without files.
///
/// # Panics
///
/// When the folder or a file cannot be written.
#[expect(
    clippy::panic,
    reason = "a test whose mod files cannot be written fails before the mod reads them"
)]
pub(crate) fn place_mod_files(base: &mut BasePresets) {
    if base.mod_files.is_empty() {
        return;
    }
    let dir = own_mod_dir();
    let _stale = std::fs::remove_dir_all(&dir);
    let written = std::fs::create_dir_all(&dir).and_then(|()| {
        base.mod_files
            .iter()
            .try_for_each(|(name, contents)| std::fs::write(dir.join(name), contents))
    });
    if let Err(why) = written {
        panic!(
            "the fake cannot write the mod's files into {}: {why}",
            dir.display()
        );
    }
    base.mod_dir = dir.to_string_lossy().into_owned();
}

/// Removes the install's own folder of mod files, when it made one.
pub(crate) fn remove_mod_files(base: &BasePresets) {
    if !base.mod_files.is_empty() {
        let _removed = std::fs::remove_dir_all(own_mod_dir());
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

#[cfg(test)]
mod tests {
    use enshwick_core::abi::{ModHandle, Status, Str};

    use crate::test::FakeHost;
    use crate::test::table::base::build::mod_dir;

    /// The `settings.toml` the mod reads in the folder the loader names it.
    fn read_by_the_mod() -> String {
        let mut dir = Str::EMPTY;
        assert_eq!(mod_dir(ModHandle(1), &raw mut dir), Status::OK);
        // SAFETY: the fake leaks the path it answers for the process.
        let dir = unsafe { dir.as_str() }.unwrap().to_owned();
        std::fs::read_to_string(std::path::Path::new(&dir).join("settings.toml")).unwrap()
    }

    #[test]
    fn installs_on_two_threads_of_one_process_each_read_their_own_mod_files() {
        let texts = ["enabled = true\n", "enabled = false\n"];
        let threads = texts.map(|text| {
            std::thread::spawn(move || {
                let loader = FakeHost::new().mod_file("settings.toml", text).install();
                let read = read_by_the_mod();
                let dir = loader.mod_dir();
                drop(loader);
                (read, dir.exists())
            })
        });
        for (thread, text) in threads.into_iter().zip(texts) {
            assert_eq!(thread.join().unwrap(), (text.to_owned(), false));
        }
    }
}
