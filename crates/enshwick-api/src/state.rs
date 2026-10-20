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
