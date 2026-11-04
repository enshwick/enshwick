//! State that outlives a session: a world's in the mod's data folder beside
//! the world's save, and holds on the game.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::Off;
use crate::locks::Relock;

mod script;
#[doc(hidden)]
pub use script::{DataStore, StateFns};

/// Why a [`WorldStore`] or [`Hold`] could not read or write its file at run
/// time.
///
/// # Examples
///
/// ```
/// use enshwick::state::StoreError;
///
/// fn report(error: &StoreError) -> String {
///     format!("kept in memory: {error}")
/// }
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// The file could not be read, written or removed.
    #[error("cannot read or write {}: {source}", path.display())]
    #[non_exhaustive]
    Io {
        /// The file.
        path: PathBuf,
        /// What the system answered.
        #[source]
        source: std::io::Error,
    },
    /// The file does not parse into the stored type.
    #[error("{} does not parse: {message}", path.display())]
    #[non_exhaustive]
    Parse {
        /// The file.
        path: PathBuf,
        /// What the parser said.
        message: String,
    },
    /// The value cannot be written as TOML.
    #[error("the value for {} does not serialise: {message}", path.display())]
    #[non_exhaustive]
    Serialize {
        /// The file.
        path: PathBuf,
        /// What the serialiser said.
        message: String,
    },
}

impl StoreError {
    fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

/// A store's error at init turns the mod off with its text.
impl From<StoreError> for Off {
    fn from(error: StoreError) -> Self {
        Self::new(error.to_string())
    }
}

/// A value kept in a TOML file, written atomically so a crash never leaves
/// half a file: one world's file of a [`WorldStore`].
#[derive(Debug, Clone)]
pub(crate) struct Store<T> {
    path: PathBuf,
    _value: PhantomData<T>,
}

impl<T: serde::de::DeserializeOwned + serde::Serialize + Default> Store<T> {
    /// A store for `<dir>/<name>.toml`.
    #[must_use]
    pub(crate) fn new(dir: &Path, name: &str) -> Self {
        Self {
            path: dir.join(format!("{name}.toml")),
            _value: PhantomData,
        }
    }

    /// The stored value, or `T::default()` when nothing is stored yet.
    ///
    /// A key the file lacks, such as a field the type gained in an update,
    /// comes from `T::default()`, in nested tables too. Arrays come from the file
    /// whole; a map whose default holds entries gets each one back while
    /// the file lacks it.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file exists but cannot be read or does not
    /// parse into `T`.
    pub(crate) fn load(&self) -> Result<T, StoreError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => with_defaults(&text).map_err(|e| StoreError::Parse {
                path: self.path.clone(),
                message: e.message().to_owned(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(StoreError::io(&self.path, e)),
        }
    }

    /// Writes `value` through [`files::write_atomically`](crate::files::write_atomically),
    /// which keeps the old file as its backup.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the value cannot be serialised or the file
    /// written.
    pub(crate) fn save(&self, value: &T) -> Result<(), StoreError> {
        write_atomically(&self.path, &self.text(value)?)
    }

    /// `value` as the file's text.
    fn text(&self, value: &T) -> Result<String, StoreError> {
        toml::to_string(value).map_err(|e| StoreError::Serialize {
            path: self.path.clone(),
            message: e.to_string(),
        })
    }
}

/// `text` read as `T`, each key it lacks taken from `T::default()`. Arrays
/// and their tables are taken whole, never filled.
fn with_defaults<T>(text: &str) -> Result<T, toml::de::Error>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Default,
{
    let mut table: toml::Table = toml::from_str(text)?;
    if let Ok(defaults) = toml::Table::try_from(T::default()) {
        fill(&mut table, defaults);
    }
    table.try_into()
}

/// Adds to `table` every key of `defaults` it lacks, tables within tables
/// too.
fn fill(table: &mut toml::Table, defaults: toml::Table) {
    for (key, default) in defaults {
        match (table.get_mut(&key), default) {
            (None, default) => {
                table.insert(key, default);
            }
            (Some(toml::Value::Table(inner)), toml::Value::Table(default)) => fill(inner, default),
            (Some(_), _) => {}
        }
    }
}

/// Writes `text` to `path` through [`crate::files::write_atomically`].
fn write_atomically(path: &Path, text: &str) -> Result<(), StoreError> {
    crate::files::write_atomically(path, text).map_err(|e| StoreError::io(path, e))
}

/// The file a [`Hold`] keeps in the folder it holds from.
pub const HOLD_FILE: &str = "hold.txt";

/// A hold on the game for a world the vanilla game cannot open safely, such
/// as one with more altars than the game groups.
///
/// While a mod's hold is set, the loader refuses to let the game go on
/// without that mod: when the game build is unknown (the loader would leave
/// the game vanilla), or the mod is missing, skipped or off, it logs the
/// reason and stops the game before the world loads. The hold is a file,
/// `hold.txt`, holding the reason on its first line and, from
/// [`Hold::set_text`], the same reason as a text of the mod's texts on the
/// second; a player who accepts the risk deletes it.
///
/// A hold in the mod's folder, [`Load::mod_dir`](crate::Load::mod_dir),
/// holds every world and counts as the loader starts, on every build. A
/// hold in the mod's data folder of one world,
/// `World::data_dir`, stops the game only
/// as that world loads, where the loader runs; a build it leaves vanilla
/// learns no world, so it does not stop for such a hold.
///
/// # Examples
///
/// ```
/// use enshwick::state::{Hold, holds};
///
/// let mods = std::env::temp_dir().join("enshwick-hold-doc");
/// let hold = Hold::new(&mods.join("you.lanterns"));
/// hold.set("the world has 20 lit lanterns")?;
/// assert_eq!(
///     holds(&mods),
///     vec![("you.lanterns".to_owned(), "the world has 20 lit lanterns".to_owned())]
/// );
/// hold.clear()?;
/// assert!(holds(&mods).is_empty());
/// # Ok::<(), enshwick::state::StoreError>(())
/// ```
#[derive(Debug, Clone)]
pub struct Hold {
    path: PathBuf,
}

#[crate::script(hold)]
impl Hold {
    /// The hold kept in `dir`: the mod's folder from
    /// [`Load::mod_dir`](crate::Load::mod_dir) for every world, or its data
    /// folder of one world from
    /// `World::data_dir`.
    ///
    /// # Examples
    ///
    /// See [`Hold`].
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join(HOLD_FILE),
        }
    }

    /// Sets the hold with `reason`, one line the loader logs; written to a
    /// temporary file first, then renamed.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file cannot be written.
    ///
    /// # Examples
    ///
    /// See [`Hold`].
    #[script]
    pub fn set(&self, reason: &str) -> Result<(), StoreError> {
        self.write(first_line(reason))
    }

    /// Sets the hold with the text of `key` in the mod's texts, each
    /// `{name}` filled from `args`, which the loader logs in the reader's
    /// language while the mod's texts are loaded, and with `reason`, one
    /// line it logs when they are not, as when the mod is missing.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file cannot be written.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::state::Hold;
    ///
    /// let hold = Hold::new(&std::env::temp_dir().join("enshwick-text-hold-doc"));
    /// hold.set_text("the world has 20 altars", "hold.altars", &[("altars", "20")])?;
    /// assert_eq!(hold.reason().as_deref(), Some("the world has 20 altars"));
    /// hold.clear()?;
    /// # Ok::<(), enshwick::state::StoreError>(())
    /// ```
    pub fn set_text(
        &self,
        reason: &str,
        key: &str,
        args: &[(&str, &str)],
    ) -> Result<(), StoreError> {
        let text = crate::i18n::reason::encode(key, args);
        self.write(&format!("{}\n{text}", first_line(reason)))
    }

    fn write(&self, contents: &str) -> Result<(), StoreError> {
        write_atomically(&self.path, contents)
    }

    /// Releases the hold; nothing happens when it is not set.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file exists and cannot be removed.
    ///
    /// # Examples
    ///
    /// See [`Hold`].
    #[script]
    pub fn clear(&self) -> Result<(), StoreError> {
        let backup = crate::files::backup_path(&self.path);
        for path in [&self.path, &backup] {
            match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(StoreError::io(path, e));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The reason, while the hold is set.
    ///
    /// # Examples
    ///
    /// ```
    /// let hold = enshwick::state::Hold::new(&std::env::temp_dir().join("enshwick-no-hold"));
    /// assert_eq!(hold.reason(), None);
    /// ```
    #[must_use]
    #[script]
    pub fn reason(&self) -> Option<String> {
        std::fs::read_to_string(&self.path)
            .ok()
            .map(|held| first_line(held.trim_start()).trim().to_owned())
    }

    /// The reason [`Hold::set_text`] gave as a text, as the loader reads it
    /// with [`Texts::reason`](crate::i18n::Texts::reason); `None` while the
    /// hold is not set or was set with one line.
    #[doc(hidden)]
    #[must_use]
    #[script]
    pub fn text(&self) -> Option<String> {
        let held = std::fs::read_to_string(&self.path).ok()?;
        let text = held.trim_start().lines().nth(1)?;
        (!text.is_empty()).then(|| text.to_owned())
    }
}

/// The first line of `text`.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// Every hold in the folders directly in `dir`, as `(folder, reason)`,
/// ordered by folder: in `mods/`, by the mods' folders; in a world's data
/// folder, by the mods' ids.
///
/// # Examples
///
/// See [`Hold`].
#[must_use]
pub fn holds(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = entry.file_name().into_string().ok()?;
            let reason = Hold::new(&entry.path()).reason()?;
            Some((id, reason))
        })
        .collect();
    found.sort();
    found
}

/// The mod's data folder in the world that runs, from the loader; `None`
/// while no world runs.
pub(crate) fn data_dir(host: crate::Host) -> Option<PathBuf> {
    let mut out = crate::abi::Str::EMPTY;
    if !(host.api().data_dir)(host.handle(), &raw mut out).is_ok() {
        return None;
    }
    // SAFETY: `data_dir` returned OK, and the loader keeps the path's bytes
    // alive and unchanged for the process, which outlives the copy here.
    unsafe { out.as_str() }.map(PathBuf::from)
}

/// The key of the world the server runs, from the loader.
pub(crate) fn world_key(host: crate::Host) -> Option<String> {
    let api = host.api();
    let mut out = crate::abi::Str::EMPTY;
    if !(api.world_key)(host.handle(), &raw mut out).is_ok() {
        return None;
    }
    // SAFETY: `world_key` returned OK, and the loader keeps the key's bytes
    // alive and unchanged for the process, which outlives the copy here.
    unsafe { out.as_str() }.map(str::to_owned)
}

/// A value kept per world, in `<stem>-mod-data/<id>/<name>.toml` beside the
/// world's save.
///
/// The file is never inside the save: removing the mod leaves the world
/// vanilla, and copying the save folder takes the value along. The value is
/// a TOML table, such as a struct.
///
/// A field the type gains in an update reads from `Default` while a world's
/// file lacks it, in nested tables too.
///
/// Get it from [`Load::world_store`](crate::Load::world_store). It reads the
/// running world's file when the world has loaded and writes it, when
/// changed, as the world saves and at shutdown, so the mod's state and the
/// world's save move together. A hosted game switching worlds switches
/// files.
///
/// # Examples
///
/// ```
/// #[derive(Default, serde::Deserialize, serde::Serialize)]
/// struct Bounties {
///     paid: u64,
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let bounties = load.world_store::<Bounties>("bounties")?;
///     load.on::<enshwick::events::PlayerJoined>(move |_| {
///         let _paid = bounties.update(|b| b.paid += 1);
///     })
/// }
/// ```
pub struct WorldStore<T> {
    slot: std::sync::Arc<std::sync::Mutex<WorldSlot<T>>>,
    /// The background write under way, joined before the next one.
    writing: std::sync::Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
    /// A background write failed: the next save writes the value again.
    unwritten: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl<T> Clone for WorldStore<T> {
    fn clone(&self) -> Self {
        Self {
            slot: std::sync::Arc::clone(&self.slot),
            writing: std::sync::Arc::clone(&self.writing),
            unwritten: std::sync::Arc::clone(&self.unwritten),
        }
    }
}

impl<T> WorldStore<T> {
    /// Waits for the background write under way, if any; the guard keeps
    /// the next one from starting.
    fn wait_for_writes(&self) -> std::sync::MutexGuard<'_, Option<std::thread::JoinHandle<()>>> {
        let mut writing = self.writing.relock();
        if let Some(handle) = writing.take()
            && handle.join().is_err()
        {
            crate::error!("a world store write panicked");
        }
        writing
    }

    fn relock(&self) -> std::sync::MutexGuard<'_, WorldSlot<T>> {
        self.slot.relock()
    }
}

impl<T> core::fmt::Debug for WorldStore<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let slot = self.relock();
        f.debug_struct("WorldStore")
            .field("name", &slot.name)
            .field("key", &slot.open.as_ref().map(|open| &open.key))
            .finish_non_exhaustive()
    }
}

/// What a [`WorldStore`] holds: its file's name and the open world's value.
struct WorldSlot<T> {
    name: String,
    open: Option<OpenWorld<T>>,
    /// Why the running world's file did not read.
    shut: Option<String>,
}

/// The value of the world whose file is open.
struct OpenWorld<T> {
    key: String,
    store: Store<T>,
    value: T,
    changed: bool,
}

/// Checks that `name` may name a store's file: 1 to 64 of `[a-z0-9_-]`.
fn check_store_name(name: &str) -> Result<(), Off> {
    let well_formed = (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if !well_formed {
        return Err(Off::new(format!(
            "world store {name:?}: a name is 1 to 64 of [a-z0-9_-]"
        )));
    }
    Ok(())
}

impl<T> WorldStore<T>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Default + Send + 'static,
{
    /// A store named `name`; a world's file is `<name>.toml` in the mod's
    /// data folder of that world.
    ///
    /// # Errors
    ///
    /// [`Off`] when `name` is not 1 to 64 of `[a-z0-9_-]`.
    pub(crate) fn new(name: &str) -> Result<Self, Off> {
        check_store_name(name)?;
        Ok(Self {
            slot: std::sync::Arc::new(std::sync::Mutex::new(WorldSlot {
                name: name.to_owned(),
                open: None,
                shut: None,
            })),
            writing: std::sync::Arc::default(),
            unwritten: std::sync::Arc::default(),
        })
    }

    /// Reads the file of the world with `key` in the mod's data folder `dir`
    /// of that world, writing the open one first. A file that does not parse
    /// stays as it is and the store stays shut.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the open world's file cannot be written or the
    /// new one does not parse.
    pub(crate) fn open(&self, key: &str, dir: &Path) -> Result<(), StoreError> {
        self.close()?;
        let mut slot = self.relock();
        let store = Store::new(dir, &slot.name);
        let value = store
            .load()
            .inspect_err(|error| slot.shut = Some(error.to_string()))?;
        slot.open = Some(OpenWorld {
            key: key.to_owned(),
            store,
            value,
            changed: false,
        });
        Ok(())
    }

    /// Writes the open world's value when it changed and shuts the store.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file cannot be written; the store stays open.
    pub(crate) fn close(&self) -> Result<(), StoreError> {
        drop(self.wait_for_writes());
        self.flush()?;
        let mut slot = self.relock();
        slot.open = None;
        slot.shut = None;
        Ok(())
    }

    /// Writes the open world's value now, when it changed since the last
    /// write; the store does so itself when the world saves.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the value cannot be written.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[derive(Default, serde::Deserialize, serde::Serialize)]
    /// # struct Bounties { paid: u64 }
    /// fn now(store: &enshwick::state::WorldStore<Bounties>) {
    ///     if let Err(error) = store.flush() {
    ///         enshwick::warn!("{error}");
    ///     }
    /// }
    /// ```
    pub fn flush(&self) -> Result<(), StoreError> {
        let mut slot = self.relock();
        // A background write still running would land after this one.
        let _writing = self.wait_for_writes();
        if let Some(open) = slot.open.as_mut() {
            open.changed |= self.unwritten.swap(false, Ordering::Relaxed);
            if open.changed {
                open.store.save(&open.value)?;
                open.changed = false;
            }
        }
        Ok(())
    }

    /// Writes the open world's value on a thread of its own when it changed;
    /// the game thread waits only for a previous write still under way. A
    /// write that fails leaves the value changed for the next save.
    pub(crate) fn flush_in_background(&self) {
        let mut slot = self.relock();
        let mut writing = self.wait_for_writes();
        let Some(open) = slot.open.as_mut() else {
            return;
        };
        open.changed |= self.unwritten.swap(false, Ordering::Relaxed);
        if !open.changed {
            return;
        }
        let text = match open.store.text(&open.value) {
            Ok(text) => text,
            Err(error) => {
                crate::error!("{error}");
                return;
            }
        };
        let path = open.store.path.clone();
        let unwritten = std::sync::Arc::clone(&self.unwritten);
        let spawned = std::thread::Builder::new()
            .name("world-store".to_owned())
            .spawn(move || {
                if let Err(error) = write_atomically(&path, &text) {
                    unwritten.store(true, Ordering::Relaxed);
                    crate::error!("{error}; kept for the next save");
                }
            });
        match spawned {
            Ok(handle) => {
                open.changed = false;
                *writing = Some(handle);
            }
            Err(e) => crate::error!("world store kept for the next save: {e}"),
        }
    }

    /// `f` on the running world's value; `None` while no world is open. The
    /// store stays locked while `f` runs, so `f` must not use it.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[derive(Default, serde::Deserialize, serde::Serialize)]
    /// # struct Bounties { paid: u64 }
    /// fn paid(store: &enshwick::state::WorldStore<Bounties>) -> u64 {
    ///     store.with(|b| b.paid).unwrap_or(0)
    /// }
    /// ```
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.relock().open.as_ref().map(|open| f(&open.value))
    }

    /// `f` on the running world's value, which is written when the world
    /// next saves; `None`, changing nothing, while no world is open. As for
    /// [`WorldStore::with`], `f` must not use the store.
    ///
    /// # Examples
    ///
    /// See [`WorldStore`].
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let mut slot = self.relock();
        let open = slot.open.as_mut()?;
        open.changed = true;
        Some(f(&mut open.value))
    }

    /// Why the running world's file did not read, while the store stays shut
    /// for that world: a mod that guards something with the value keeps it
    /// guarded meanwhile. The loader's status summary shows it too.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[derive(Default, serde::Deserialize, serde::Serialize)]
    /// # struct Owners { names: Vec<String> }
    /// fn may_open(store: &enshwick::state::WorldStore<Owners>, name: &str) -> bool {
    ///     if store.shut_reason().is_some() {
    ///         return false;
    ///     }
    ///     store.with(|o| o.names.iter().any(|n| n == name)).unwrap_or(true)
    /// }
    /// ```
    #[must_use]
    pub fn shut_reason(&self) -> Option<String> {
        self.relock().shut.clone()
    }

    /// The key of the world whose value is open.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[derive(Default, serde::Deserialize, serde::Serialize)]
    /// # struct Bounties { paid: u64 }
    /// fn which(store: &enshwick::state::WorldStore<Bounties>) -> String {
    ///     store.key().unwrap_or_else(|| "no world".to_owned())
    /// }
    /// ```
    #[must_use]
    pub fn key(&self) -> Option<String> {
        self.relock().open.as_ref().map(|open| open.key.clone())
    }
}

#[cfg(test)]
mod tests;

/// Process-wide state of a mod that starts afresh with every mod life: once
/// per load in the game, once per install under the fake host.
///
/// A `static` keeps mod state outside the mod's own type, such as an
/// extension point's handle or a world store. In a `ModLocal` it reads as
/// empty again when the next test installs the fake host, so tests that
/// share a process under `cargo test` each see their own mod. A value is
/// never dropped, so a reference to it lives for the process.
///
/// # Examples
///
/// ```
/// use enshwick::state::ModLocal;
///
/// static LIMIT: ModLocal<u32> = ModLocal::new();
///
/// assert_eq!(LIMIT.get(), None);
/// assert_eq!(LIMIT.get_or_init(|| 4), &4);
/// assert_eq!(LIMIT.set(5), Err(5));
/// ```
#[derive(Debug)]
pub struct ModLocal<T>(crate::fresh::Fresh<T>);

impl<T> ModLocal<T> {
    /// An empty cell, for a `static`.
    ///
    /// # Examples
    ///
    /// See [`ModLocal`].
    #[must_use]
    pub const fn new() -> Self {
        Self(crate::fresh::Fresh::new())
    }

    /// The value of this mod life, once set.
    ///
    /// # Examples
    ///
    /// See [`ModLocal`].
    #[must_use]
    pub fn get(&self) -> Option<&T> {
        self.0.get()
    }

    /// Sets the value of this mod life.
    ///
    /// # Errors
    ///
    /// Hands `value` back when this life has a value already.
    ///
    /// # Examples
    ///
    /// See [`ModLocal`].
    pub fn set(&self, value: T) -> Result<(), T> {
        self.0.set(value)
    }

    /// The value of this mod life, made by `init` when there is none yet.
    ///
    /// # Examples
    ///
    /// See [`ModLocal`].
    pub fn get_or_init(&self, init: impl FnOnce() -> T) -> &T {
        self.0.get_or_init(init)
    }
}

impl<T> Default for ModLocal<T> {
    fn default() -> Self {
        Self::new()
    }
}
