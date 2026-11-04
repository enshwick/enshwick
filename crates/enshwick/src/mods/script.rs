//! The loaded mods and the mod's own folder as the script namespace `mods`.

use std::path::{Component, Path, PathBuf};

use crate::abi::Side;
use crate::resources::Bytes;
use crate::{Host, Off};

/// The script view of the loaded mods: which mods run, whether this game
/// runs the server, the side the mod runs on, and the files of its own
/// folder, read-only.
///
/// The Rust API offers the same through [`is_running`](super::is_running),
/// [`Host::hosting`], [`Host::build`] and
/// [`Load::mod_dir`](crate::Load::mod_dir).
///
/// # Examples
///
/// ```lua
/// if enshwick.mods.hosting() and enshwick.mods.is_running("you.lockers") then
///     local names = buffer.tostring(enshwick.mods.read("data/names.txt"))
/// end
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Mods {
    host: Host,
    dir: &'static Path,
}

impl Mods {
    /// `relative` inside the mod's folder; `None` for a path that leaves it.
    fn inside(self, relative: &str) -> Option<PathBuf> {
        let relative = Path::new(relative);
        relative
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
            .then(|| self.dir.join(relative))
    }
}

#[crate::script(folder)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though is_running reads nothing of it"
)]
impl Mods {
    /// Whether the mod `id`, such as `you.lockers`, runs in this game.
    #[script]
    #[must_use]
    pub fn is_running(self, id: &str) -> bool {
        super::is_running(id)
    }

    /// Every loaded mod, the loader first, in load order, as
    /// [`all`](super::all) lists them.
    #[script]
    #[must_use]
    pub fn all(self) -> Vec<super::ModEntry> {
        super::all()
    }

    /// The server's mods that the last refused join found missing on this
    /// client or in another version, as
    /// [`join_missing`](super::join_missing) lists them.
    #[script]
    #[must_use]
    pub fn join_missing(self) -> Vec<super::ModEntry> {
        super::join_missing()
    }

    /// Whether this game runs the server now, so the mod's server logic
    /// acts: always on the dedicated server; on a client while it hosts a
    /// game, single player included, as mods run with server behaviour
    /// there.
    #[script]
    #[must_use]
    pub fn hosting(self) -> bool {
        self.host.hosting()
    }

    /// The game executable the mod runs in: `server` for the dedicated
    /// server, `client` for the game, which may also host one (`hosting`).
    #[script]
    #[must_use]
    pub fn side(self) -> String {
        if self.host.build().side == Side::CLIENT {
            "client".to_owned()
        } else {
            "server".to_owned()
        }
    }

    /// The bytes of the file `path`, relative to the mod's folder.
    ///
    /// # Errors
    ///
    /// [`Off`] for a path that leaves the folder and a file that does not
    /// read.
    #[script]
    pub fn read(self, path: &str) -> Result<Bytes, Off> {
        let file = self
            .inside(path)
            .ok_or_else(|| Off::new(format!("{path} leaves the mod's folder")))?;
        std::fs::read(&file)
            .map(Bytes)
            .map_err(|e| Off::new(format!("{path}: {e}")))
    }

    /// The names of the files and folders in `path`, relative to the mod's
    /// folder (`.` for the folder itself), sorted.
    ///
    /// # Errors
    ///
    /// [`Off`] for a path that leaves the folder and a folder that does not
    /// read.
    #[script]
    pub fn list(self, path: &str) -> Result<Vec<String>, Off> {
        let dir = self
            .inside(path)
            .ok_or_else(|| Off::new(format!("{path} leaves the mod's folder")))?;
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .map_err(|e| Off::new(format!("{path}: {e}")))?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        names.sort();
        Ok(names)
    }
}

impl crate::script::Bound for Mods {
    const NAME: &'static str = "mods";
    const DOC: &'static str =
        "The loaded mods, the side the mod runs on, and its own folder, read-only.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_FOLDER];

    fn resolve(load: &mut crate::Load<'_>) -> Result<Self, Off> {
        let dir: &'static Path = Box::leak(load.mod_dir()?.into_boxed_path());
        Ok(Self {
            host: load.host(),
            dir,
        })
    }
}
