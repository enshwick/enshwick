//! State that outlives a session, in the mod's data folder.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::Off;

/// A value kept in a TOML file under the mod's data folder, written atomically
/// so a crash never leaves half a file.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::state::Store;
///
/// #[derive(Default, serde::Deserialize, serde::Serialize, PartialEq, Debug)]
/// struct Progress {
///     highest_xp: u64,
/// }
///
/// # fn main() -> Result<(), enshwick_loader_api::Off> {
/// let dir = std::env::temp_dir().join("enshwick-store-doc");
/// let store: Store<Progress> = Store::new(&dir, "progress");
/// store.save(&Progress { highest_xp: 42 })?;
/// assert_eq!(store.load()?.highest_xp, 42);
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
    /// [`Off`] when the file exists but does not parse into `T`.
    ///
    /// # Examples
    ///
    /// See [`Store`].
    pub fn load(&self) -> Result<T, Off> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| Off::new(format!("state {}: {}", self.path.display(), e.message()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(Off::new(format!("state {}: {e}", self.path.display()))),
        }
    }

    /// Writes `value`: to a temporary file first, then renamed over the old one.
    ///
    /// # Errors
    ///
    /// [`Off`] when the value cannot be serialised or the file written.
    ///
    /// # Examples
    ///
    /// See [`Store`].
    pub fn save(&self, value: &T) -> Result<(), Off> {
        write_atomically(&self.path, &self.text(value)?)
    }

    /// `value` as the file's text.
    fn text(&self, value: &T) -> Result<String, Off> {
        toml::to_string(value).map_err(|e| Off::new(format!("state {}: {e}", self.path.display())))
    }
}

/// Writes `text` to `path`: to a temporary file first, then renamed over the
/// old one.
fn write_atomically(path: &Path, text: &str) -> Result<(), Off> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Off::new(format!("state: {e}")))?;
    }
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, text)
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|e| Off::new(format!("state {}: {e}", path.display())))
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
/// use enshwick_loader_api::state::{Hold, holds};
///
/// let data = std::env::temp_dir().join("enshwick-hold-doc");
/// let hold = Hold::new(&data.join("flame-altar-count"));
/// hold.set("the world has 20 altar zones")?;
/// assert_eq!(
///     holds(&data),
///     vec![("flame-altar-count".to_owned(), "the world has 20 altar zones".to_owned())]
/// );
/// hold.clear()?;
/// assert!(holds(&data).is_empty());
/// # Ok::<(), enshwick_loader_api::Off>(())
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
    /// [`Off`] when the file cannot be written.
    ///
    /// # Examples
    ///
    /// See [`Hold`].
    pub fn set(&self, reason: &str) -> Result<(), Off> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Off::new(format!("hold: {e}")))?;
        }
        let temporary = self.path.with_extension("txt.tmp");
        std::fs::write(&temporary, reason.lines().next().unwrap_or_default())
            .and_then(|()| std::fs::rename(&temporary, &self.path))
            .map_err(|e| Off::new(format!("hold {}: {e}", self.path.display())))
    }

    /// Releases the hold; nothing happens when it is not set.
    ///
    /// # Errors
    ///
    /// [`Off`] when the file exists and cannot be removed.
    ///
    /// # Examples
    ///
    /// See [`Hold`].
    pub fn clear(&self) -> Result<(), Off> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(Off::new(format!("hold {}: {e}", self.path.display())))
            }
            _ => Ok(()),
        }
    }

    /// The reason, while the hold is set.
    ///
    /// # Examples
    ///
    /// ```
    /// let hold = enshwick_loader_api::state::Hold::new(&std::env::temp_dir().join("enshwick-no-hold"));
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
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let bounties = load.world_store::<Bounties>("bounties")?;
///     load.on::<enshwick_loader_api::events::PlayerJoined>(move |_| {
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
    /// [`Off`] when the open world's file cannot be written or the new one
    /// does not parse.
    pub(crate) fn open(&self, key: &str) -> Result<(), Off> {
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
    /// [`Off`] when the file cannot be written; the store stays open.
    pub(crate) fn close(&self) -> Result<(), Off> {
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
    /// [`Off`] when the value cannot be written.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[derive(Default, serde::Deserialize, serde::Serialize)]
    /// # struct Bounties { paid: u64 }
    /// fn now(store: &enshwick_loader_api::state::WorldStore<Bounties>) {
    ///     if let Err(off) = store.flush() {
    ///         enshwick_loader_api::warn!("{off}");
    ///     }
    /// }
    /// ```
    pub fn flush(&self) -> Result<(), Off> {
        let mut slot = self.lock();
        if let Some(open) = slot.open.as_mut().filter(|open| open.changed) {
            open.store.save(&open.value)?;
            open.changed = false;
        }
        Ok(())
    }

    /// Writes the open world's value on a thread of its own when it changed,
    /// so the game thread never waits on the disk.
    pub(crate) fn flush_in_background(&self) {
        let mut slot = self.lock();
        let Some(open) = slot.open.as_mut().filter(|open| open.changed) else {
            return;
        };
        let text = match open.store.text(&open.value) {
            Ok(text) => text,
            Err(off) => {
                crate::error!("{off}");
                return;
            }
        };
        open.changed = false;
        let path = open.store.path.clone();
        let mut writing = self.wait_for_writes();
        let spawned = std::thread::Builder::new()
            .name("world-store".to_owned())
            .spawn(move || {
                if let Err(off) = write_atomically(&path, &text) {
                    crate::error!("{off}");
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

    /// `f` on the running world's value; `None` while no world is open.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[derive(Default, serde::Deserialize, serde::Serialize)]
    /// # struct Bounties { paid: u64 }
    /// fn paid(store: &enshwick_loader_api::state::WorldStore<Bounties>) -> u64 {
    ///     store.with(|b| b.paid).unwrap_or(0)
    /// }
    /// ```
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.lock().open.as_ref().map(|open| f(&open.value))
    }

    /// `f` on the running world's value, which is written when the world
    /// next saves; `None`, changing nothing, while no world is open.
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
    /// fn which(store: &enshwick_loader_api::state::WorldStore<Bounties>) -> String {
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
}
