//! State that outlives a session, in the mod's data folder.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::Off;

/// Why a [`Store`], [`WorldStore`] or [`Hold`] could not read or write its
/// file at run time.
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

/// A value kept in a TOML file under the mod's data folder, written atomically
/// so a crash never leaves half a file.
///
/// # Examples
///
/// ```
/// use enshwick::state::Store;
///
/// #[derive(Default, serde::Deserialize, serde::Serialize, PartialEq, Debug)]
/// struct Progress {
///     highest_experience: u64,
/// }
///
/// # fn main() -> Result<(), enshwick::Off> {
/// let dir = std::env::temp_dir().join("enshwick-store-doc");
/// let store: Store<Progress> = Store::new(&dir, "progress");
/// store.save(&Progress { highest_experience: 42 })?;
/// assert_eq!(store.load()?.highest_experience, 42);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Store<T> {
    path: PathBuf,
    _value: PhantomData<T>,
}

impl<T: serde::de::DeserializeOwned + serde::Serialize + Default> Store<T> {
    /// A store for `<dir>/<name>.toml`; get `dir` from
    /// [`Load::data_dir`](crate::Load::data_dir).
    ///
    /// # Examples
    ///
    /// See [`Store`].
    #[must_use]
    pub fn new(dir: &Path, name: &str) -> Self {
        Self {
            path: dir.join(format!("{name}.toml")),
            _value: PhantomData,
        }
    }

    /// The stored value, or `T::default()` when nothing is stored yet.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file exists but cannot be read or does not
    /// parse into `T`.
    ///
    /// # Examples
    ///
    /// See [`Store`].
    pub fn load(&self) -> Result<T, StoreError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text).map_err(|e| StoreError::Parse {
                path: self.path.clone(),
                message: e.message().to_owned(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(StoreError::io(&self.path, e)),
        }
    }

    /// Writes `value`: to a temporary file first, then renamed over the old one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the value cannot be serialised or the file
    /// written.
    ///
    /// # Examples
    ///
    /// See [`Store`].
    pub fn save(&self, value: &T) -> Result<(), StoreError> {
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

/// Writes `text` to `path`: to a temporary file first, then renamed over the
/// old one.
fn write_atomically(path: &Path, text: &str) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| StoreError::io(dir, e))?;
    }
    let temporary = path.with_extension("toml.tmp");
    // On disk before the rename, so a crash leaves the old file or the new.
    let written = std::fs::File::create(&temporary).and_then(|mut file| {
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        file.sync_all()
    });
    written
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|e| StoreError::io(path, e))
}

/// The file a [`Hold`] keeps in its mod's data folder.
pub const HOLD_FILE: &str = "hold.txt";

/// A hold on the game for a world the vanilla game cannot open safely, such
/// as one with more altars than the game groups.
///
/// While a mod's hold is set, the loader refuses to let the game go on
/// without that mod: when the game build is unknown (the loader would leave
/// the game vanilla), or the mod is missing, skipped or off, it logs the
/// reason and stops the game before the world loads. The hold is a file,
/// `mod-data/<id>/hold.txt`, holding the reason; a player who accepts the
/// risk deletes it.
///
/// # Examples
///
/// ```
/// use enshwick::state::{Hold, holds};
///
/// let data = std::env::temp_dir().join("enshwick-hold-doc");
/// let hold = Hold::new(&data.join("you.lanterns"));
/// hold.set("the world has 20 lit lanterns")?;
/// assert_eq!(
///     holds(&data),
///     vec![("you.lanterns".to_owned(), "the world has 20 lit lanterns".to_owned())]
/// );
/// hold.clear()?;
/// assert!(holds(&data).is_empty());
/// # Ok::<(), enshwick::state::StoreError>(())
/// ```
#[derive(Debug, Clone)]
pub struct Hold {
    path: PathBuf,
}

impl Hold {
    /// The hold of the mod whose data folder is `dir`, from
    /// [`Load::data_dir`](crate::Load::data_dir).
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
    pub fn set(&self, reason: &str) -> Result<(), StoreError> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| StoreError::io(dir, e))?;
        }
        let temporary = self.path.with_extension("txt.tmp");
        std::fs::write(&temporary, reason.lines().next().unwrap_or_default())
            .and_then(|()| std::fs::rename(&temporary, &self.path))
            .map_err(|e| StoreError::io(&self.path, e))
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
    pub fn clear(&self) -> Result<(), StoreError> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(StoreError::io(&self.path, e))
            }
            _ => Ok(()),
        }
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
    pub fn reason(&self) -> Option<String> {
        std::fs::read_to_string(&self.path)
            .ok()
            .map(|reason| reason.trim().to_owned())
    }
}

/// Every hold under the `mod-data` folder `mod_data`, as `(mod id, reason)`,
/// ordered by id.
///
/// # Examples
///
/// See [`Hold`].
#[must_use]
pub fn holds(mod_data: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(mod_data) else {
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

/// A value kept per world, in `mod-data/<id>/worlds/<key>/<name>.toml`,
/// never inside the game's save: removing the mod leaves the world vanilla.
/// The value is a TOML table, such as a struct.
///
/// Get it from [`Load::world_store`](crate::Load::world_store). It reads the
/// running world's file when the world has loaded and writes it, when
/// changed, as the world saves and at shutdown, so the mod's state and the
/// world's save move together. The key names the world's save file and
/// folder (see [`World::key`](crate::world::World::key)), so a hosted game
/// switching worlds switches files.
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
}

impl<T> Clone for WorldStore<T> {
    fn clone(&self) -> Self {
        Self {
            slot: std::sync::Arc::clone(&self.slot),
            writing: std::sync::Arc::clone(&self.writing),
        }
    }
}

impl<T> WorldStore<T> {
    /// Waits for the background write under way, if any; the guard keeps
    /// the next one from starting.
    fn wait_for_writes(&self) -> std::sync::MutexGuard<'_, Option<std::thread::JoinHandle<()>>> {
        let mut writing = self
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(handle) = writing.take()
            && handle.join().is_err()
        {
            crate::error!("a world store write panicked");
        }
        writing
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, WorldSlot<T>> {
        self.slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<T> core::fmt::Debug for WorldStore<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let slot = self.lock();
        f.debug_struct("WorldStore")
            .field("name", &slot.name)
            .field("key", &slot.open.as_ref().map(|open| &open.key))
            .finish_non_exhaustive()
    }
}

/// What a [`WorldStore`] holds: where its files go and the open world's
/// value.
struct WorldSlot<T> {
    worlds: PathBuf,
    name: String,
    open: Option<OpenWorld<T>>,
}

/// The value of the world whose file is open.
struct OpenWorld<T> {
    key: String,
    store: Store<T>,
    value: T,
    changed: bool,
}

/// Whether `name` may name a store's file: 1 to 64 of `[a-z0-9_-]`.
fn is_store_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

impl<T> WorldStore<T>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Default + Send + 'static,
{
    /// A store named `name` under the mod's data folder `dir`; the file of
    /// the world with `key` is `<dir>/worlds/<key>/<name>.toml`.
    ///
    /// # Errors
    ///
    /// [`Off`] when `name` is not 1 to 64 of `[a-z0-9_-]`.
    pub(crate) fn new(dir: &Path, name: &str) -> Result<Self, Off> {
        if !is_store_name(name) {
            return Err(Off::new(format!(
                "world store {name:?}: a name is 1 to 64 of [a-z0-9_-]"
            )));
        }
        Ok(Self {
            slot: std::sync::Arc::new(std::sync::Mutex::new(WorldSlot {
                worlds: dir.join("worlds"),
                name: name.to_owned(),
                open: None,
            })),
            writing: std::sync::Arc::default(),
        })
    }

    /// Reads the file of the world with `key`, writing the open one first.
    /// A file that does not parse stays as it is and the store stays shut.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the open world's file cannot be written or the
    /// new one does not parse.
    pub(crate) fn open(&self, key: &str) -> Result<(), StoreError> {
        self.close()?;
        let mut slot = self.lock();
        let store = Store::new(&slot.worlds.join(key), &slot.name);
        let value = store.load()?;
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
        self.lock().open = None;
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
        let mut slot = self.lock();
        // A background write still running would land after this one.
        let _writing = self.wait_for_writes();
        if let Some(open) = slot.open.as_mut().filter(|open| open.changed) {
            open.store.save(&open.value)?;
            open.changed = false;
        }
        Ok(())
    }

    /// Writes the open world's value on a thread of its own when it changed;
    /// the game thread waits only for a previous write still under way.
    pub(crate) fn flush_in_background(&self) {
        let mut slot = self.lock();
        let Some(open) = slot.open.as_mut().filter(|open| open.changed) else {
            return;
        };
        let text = match open.store.text(&open.value) {
            Ok(text) => text,
            Err(error) => {
                crate::error!("{error}");
                return;
            }
        };
        open.changed = false;
        let path = open.store.path.clone();
        let mut writing = self.wait_for_writes();
        let spawned = std::thread::Builder::new()
            .name("world-store".to_owned())
            .spawn(move || {
                if let Err(error) = write_atomically(&path, &text) {
                    crate::error!("{error}");
                }
            });
        match spawned {
            Ok(handle) => *writing = Some(handle),
            Err(e) => {
                open.changed = true;
                crate::error!("world store kept for the next save: {e}");
            }
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
        self.lock().open.as_ref().map(|open| f(&open.value))
    }

    /// `f` on the running world's value, which is written when the world
    /// next saves; `None`, changing nothing, while no world is open. As for
    /// [`WorldStore::with`], `f` must not use the store.
    ///
    /// # Examples
    ///
    /// See [`WorldStore`].
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let mut slot = self.lock();
        let open = slot.open.as_mut()?;
        open.changed = true;
        Some(f(&mut open.value))
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
        self.lock().open.as_ref().map(|open| open.key.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
    struct Count {
        n: u32,
    }

    #[test]
    fn a_world_store_writes_its_world_s_file_and_switches_worlds() {
        let dir = std::env::temp_dir().join(format!("enshwick-world-store-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        let store: WorldStore<Count> = WorldStore::new(&dir, "count").unwrap();
        assert_eq!(store.update(|c| c.n += 1), None);
        store.open("world0-aa").unwrap();
        store.update(|c| c.n = 5);
        store.flush().unwrap();
        store.open("world1-bb").unwrap();
        assert_eq!(store.with(|c| c.n), Some(0));
        store.open("world0-aa").unwrap();
        assert_eq!(store.with(|c| c.n), Some(5));
        assert!(dir.join("worlds/world0-aa/count.toml").exists());
        store.update(|c| c.n = 6);
        store.flush_in_background();
        store.close().unwrap();
        let text = std::fs::read_to_string(dir.join("worlds/world0-aa/count.toml")).unwrap();
        assert!(text.contains('6'), "{text}");
        store.open("world0-aa").unwrap();
        store.close().unwrap();
        assert_eq!(store.key(), None);
        assert!(WorldStore::<Count>::new(&dir, "a/b").is_err());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_flush_lands_after_a_background_write() {
        let dir = std::env::temp_dir().join(format!("enshwick-world-flush-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        let store: WorldStore<Count> = WorldStore::new(&dir, "count").unwrap();
        store.open("world0-aa").unwrap();
        let file = dir.join("worlds/world0-aa/count.toml");
        for n in 0..20 {
            store.update(|c| c.n = n);
            store.flush_in_background();
            store.update(|c| c.n = n + 100);
            store.flush().unwrap();
            let saved: Count = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
            assert_eq!(saved.n, n + 100);
        }
        store.close().unwrap();
        let _gone = std::fs::remove_dir_all(&dir);
    }
}

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
