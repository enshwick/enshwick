//! State that outlives a session, in the mod's data folder.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::Off;
use crate::locks::Relock;

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

    /// Writes `value` through [`files::write_atomically`](crate::files::write_atomically),
    /// which keeps the old file as its backup.
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

/// Writes `text` to `path` through [`crate::files::write_atomically`].
fn write_atomically(path: &Path, text: &str) -> Result<(), StoreError> {
    crate::files::write_atomically(path, text).map_err(|e| StoreError::io(path, e))
}

/// The file a [`Hold`] keeps in its mod's data folder.
pub const HOLD_FILE: &str = "hold.txt";

/// The folder in a mod's data folder that keeps one folder per world, named
/// by the world's key.
pub const WORLDS_DIR: &str = "worlds";

/// A hold on the game for a world the vanilla game cannot open safely, such
/// as one with more altars than the game groups.
///
/// While a mod's hold is set, the loader refuses to let the game go on
/// without that mod: when the game build is unknown (the loader would leave
/// the game vanilla), or the mod is missing, skipped or off, it logs the
/// reason and stops the game before the world loads. The hold is a file,
/// `mod-data/<id>/hold.txt`, holding the reason on its first line and, from
/// [`Hold::set_text`], the same reason as a text of the mod's texts on the
/// second; a player who accepts the risk deletes it. A hold of one world, [`Hold::for_world`], stops the
/// game only as that world loads, where the loader runs; a build it leaves
/// vanilla stops for every hold, since no world is known there.
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

    /// The hold of the mod whose data folder is `dir` on the world whose key
    /// is `world`, from `World::key`:
    /// `mod-data/<id>/worlds/<key>/hold.txt`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::state::{Hold, world_holds};
    ///
    /// let data = std::env::temp_dir().join("enshwick-world-hold-doc");
    /// let hold = Hold::for_world(&data.join("you.lanterns"), "world0-aa");
    /// hold.set("the world has 20 lit lanterns")?;
    /// assert_eq!(world_holds(&data, "world0-aa").len(), 1);
    /// assert!(world_holds(&data, "world1-bb").is_empty());
    /// hold.clear()?;
    /// # Ok::<(), enshwick::state::StoreError>(())
    /// ```
    #[must_use]
    pub fn for_world(dir: &Path, world: &str) -> Self {
        Self::new(&dir.join(WORLDS_DIR).join(world))
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

/// Every hold on the world whose key is `world` under the `mod-data` folder
/// `mod_data`, as `(mod id, reason)`, ordered by id.
///
/// # Examples
///
/// See [`Hold::for_world`].
#[must_use]
pub fn world_holds(mod_data: &Path, world: &str) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(mod_data) else {
        return Vec::new();
    };
    let mut found: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = entry.file_name().into_string().ok()?;
            let reason = Hold::for_world(&entry.path(), world).reason()?;
            Some((id, reason))
        })
        .collect();
    found.sort();
    found
}

/// The key of every world some mod under `mod_data` holds, sorted, each
/// once.
///
/// # Examples
///
/// ```
/// let data = std::env::temp_dir().join("enshwick-no-world-holds");
/// assert!(enshwick::state::held_worlds(&data).is_empty());
/// ```
#[must_use]
pub fn held_worlds(mod_data: &Path) -> Vec<String> {
    let Ok(mods) = std::fs::read_dir(mod_data) else {
        return Vec::new();
    };
    let mut worlds: Vec<String> = mods
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_dir(entry.path().join(WORLDS_DIR)).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|world| world.path().join(HOLD_FILE).is_file())
        .filter_map(|world| world.file_name().into_string().ok())
        .collect();
    worlds.sort();
    worlds.dedup();
    worlds
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

/// A value kept per world, in `mod-data/<id>/worlds/<key>/<name>.toml`,
/// never inside the game's save: removing the mod leaves the world vanilla.
/// The value is a TOML table, such as a struct.
///
/// Get it from [`Load::world_store`](crate::Load::world_store). It reads the
/// running world's file when the world has loaded and writes it, when
/// changed, as the world saves and at shutdown, so the mod's state and the
/// world's save move together. The key names the world's save file and
/// folder (see `World::key`), so a hosted game
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
        let mut writing = self.writing.relock();
        if let Some(handle) = writing.take()
            && handle.join().is_err()
        {
            crate::error!("a world store write panicked");
        }
        writing
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, WorldSlot<T>> {
        self.slot.relock()
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
                worlds: dir.join(WORLDS_DIR),
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
    fn a_world_s_hold_holds_that_world_alone() {
        let data = std::env::temp_dir().join(format!("enshwick-world-hold-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let lanterns = data.join("you.lanterns");
        Hold::for_world(&lanterns, "world0-aa")
            .set("20 lit lanterns")
            .unwrap();
        Hold::for_world(&data.join("you.altars"), "world1-bb")
            .set("17 altars")
            .unwrap();
        Hold::new(&data.join("you.stacks"))
            .set("raised stacks")
            .unwrap();
        assert_eq!(
            world_holds(&data, "world0-aa"),
            vec![("you.lanterns".to_owned(), "20 lit lanterns".to_owned())]
        );
        assert!(world_holds(&data, "world2-cc").is_empty());
        assert_eq!(held_worlds(&data), ["world0-aa", "world1-bb"]);
        assert_eq!(
            holds(&data),
            vec![("you.stacks".to_owned(), "raised stacks".to_owned())]
        );
        Hold::for_world(&lanterns, "world0-aa").clear().unwrap();
        assert_eq!(held_worlds(&data), ["world1-bb"]);
        let _gone = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_text_hold_keeps_its_line_for_readers_without_the_mod_s_texts() {
        let data = std::env::temp_dir().join(format!("enshwick-text-hold-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let hold = Hold::new(&data.join("you.altars"));
        hold.set_text("17 altars\nand more", "hold.altars", &[("altars", "17")])
            .unwrap();
        assert_eq!(hold.reason().as_deref(), Some("17 altars"));
        assert_eq!(hold.text().as_deref(), Some("hold.altars\taltars=17"));
        assert_eq!(
            holds(&data),
            vec![("you.altars".to_owned(), "17 altars".to_owned())]
        );
        hold.set("17 altars").unwrap();
        assert_eq!(hold.text(), None);
        let _gone = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_hold_file_of_one_plain_line_still_holds() {
        let data = std::env::temp_dir().join(format!("enshwick-old-hold-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let dir = data.join("you.stacks");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(HOLD_FILE), "raised stacks\n").unwrap();
        let hold = Hold::new(&dir);
        assert_eq!(hold.reason().as_deref(), Some("raised stacks"));
        assert_eq!(hold.text(), None);
        assert_eq!(
            holds(&data),
            vec![("you.stacks".to_owned(), "raised stacks".to_owned())]
        );
        let _gone = std::fs::remove_dir_all(&data);
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
