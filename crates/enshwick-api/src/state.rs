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
/// use enshwick_api::state::Store;
///
/// #[derive(Default, serde::Deserialize, serde::Serialize, PartialEq, Debug)]
/// struct Progress {
///     highest_xp: u64,
/// }
///
/// # fn main() -> Result<(), enshwick_api::Off> {
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
        let text = toml::to_string(value).map_err(|e| Off::new(format!("state: {e}")))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Off::new(format!("state: {e}")))?;
        }
        let temporary = self.path.with_extension("toml.tmp");
        std::fs::write(&temporary, text)
            .and_then(|()| std::fs::rename(&temporary, &self.path))
            .map_err(|e| Off::new(format!("state {}: {e}", self.path.display())))
    }
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
/// use enshwick_api::state::{Hold, holds};
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
/// # Ok::<(), enshwick_api::Off>(())
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
    /// let hold = enshwick_api::state::Hold::new(&std::env::temp_dir().join("enshwick-no-hold"));
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
