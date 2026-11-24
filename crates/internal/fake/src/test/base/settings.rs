//! Settings: the running world's settings files and the mod's checks.

use std::path::{Path, PathBuf};

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
    /// creates nothing there; with [`mod_file`](Self::mod_file) the mod gets
    /// a folder of the same last name under a `mods/` folder the install
    /// makes in the temp folder. Where the loader lists every mod, the calling
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
    /// developer tool keeps its `settings.toml` beside it; `name` may name a
    /// subfolder. With any such file each install makes a new folder,
    /// `mods/<last name of mod_dir>` under the temp folder, which
    /// [`mod_dir`](Self::mod_dir) no longer names; it is removed when the
    /// [`Installed`] drops, so tests on threads of one process never read
    /// each other's files.
    ///
    /// # Panics
    ///
    /// When `name` is empty or leaves the folder, as `..` or an absolute
    /// path does.
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
        assert!(
            !name.is_empty() && enshwick_base::mods::stays_inside(name),
            "a mod file's name stays inside the mod's folder: {name:?}"
        );
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
    /// call with a handle that is neither the mod's, a pack named here nor
    /// a mod [`other_mod`](Self::other_mod) named answers
    /// [`Status::BAD_ARGS`](enshwick_core::abi::Status::BAD_ARGS), as the
    /// loader answers a handle it never gave out.
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

    /// Another mod the loader runs beside the mod under test, with handle
    /// `handle`, so a test calls [`Installed::api`] with that handle to act
    /// as a second mod: one that holds what the mod under test asks for,
    /// say. It runs on both sides, and the loader's rules for a mod apply to
    /// its calls: what it asks for in init is its own, the events it sends
    /// must be its own, and it has no settings files.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Level, ModHandle, Status, Str};
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .other_mod(ModHandle(2))
    ///     .install();
    /// // SAFETY: the fake's table is a static; the message lives for the call.
    /// let api = unsafe { &*loader.api() };
    /// assert_eq!((api.log)(ModHandle(2), Level::INFO, Str::new("hi")), Status::OK);
    /// assert_eq!((api.log)(ModHandle(3), Level::INFO, Str::new("hi")), Status::BAD_ARGS);
    /// ```
    #[must_use]
    pub fn other_mod(mut self, handle: ModHandle) -> Self {
        if !self.base.other_mods.contains(&handle) {
            self.base.other_mods.push(handle);
        }
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

/// Writes the mod files of `base` into a new folder of the install's own,
/// `mods/<last name of mod_dir>` under the temp folder, and names it the
/// mod's folder; the folder to drop with the install, `None` without files.
///
/// # Panics
///
/// When the folder or a file cannot be written.
#[expect(
    clippy::panic,
    reason = "a test whose mod files cannot be written fails before the mod reads them"
)]
pub(crate) fn place_mod_files(base: &mut BasePresets) -> Option<OwnFolder> {
    if base.mod_files.is_empty() {
        return None;
    }
    let folder = Path::new(&base.mod_dir)
        .file_name()
        .map_or_default(ToOwned::to_owned);
    // Taken, so a copy of the host for each side check carries no contents.
    let files = core::mem::take(&mut base.mod_files);
    let written = OwnFolder::new().and_then(|root| {
        let dir = root.0.join("mods").join(folder);
        for (name, contents) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap_or(&dir))?;
            std::fs::write(path, contents)?;
        }
        Ok((root, dir))
    });
    match written {
        Ok((root, dir)) => {
            base.mod_dir = dir.to_string_lossy().into_owned();
            Some(root)
        }
        Err(why) => panic!("the fake cannot write the mod's files: {why}"),
    }
}

/// A new folder of one install in the temp folder, removed when it drops.
#[derive(Debug)]
pub(crate) struct OwnFolder(PathBuf);

impl OwnFolder {
    /// Claims the first free `enshwick-fake-<process>-<n>`: making a folder
    /// fails on one that exists, so no two installs share one.
    fn new() -> std::io::Result<Self> {
        let process = std::process::id();
        for n in 0u32.. {
            let path = std::env::temp_dir().join(format!("enshwick-fake-{process}-{n}"));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(why) if why.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(why) => return Err(why),
            }
        }
        Err(std::io::ErrorKind::StorageFull.into())
    }
}

impl Drop for OwnFolder {
    fn drop(&mut self) {
        let _removed = std::fs::remove_dir_all(&self.0);
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

    #[test]
    fn a_mod_file_may_sit_in_a_subfolder_of_a_folder_named_as_mod_dir_inside_mods() {
        let loader = FakeHost::new()
            .mod_dir("game/mods/Bonus")
            .mod_file("data/names.txt", "a")
            .install();
        let dir = loader.mod_dir();
        assert!(dir.ends_with("mods/Bonus"), "{}", dir.display());
        assert_eq!(
            std::fs::read_to_string(dir.join("data/names.txt")).unwrap(),
            "a"
        );
    }

    #[test]
    #[should_panic(expected = "stays inside the mod's folder")]
    fn a_mod_file_that_would_leave_the_folder_is_refused() {
        let _host = FakeHost::new().mod_file("../settings.toml", "");
    }
}
