//! The running game's archive, opened once for the process; the reader is
//! [`enshwick_gamedata::archive`].

use std::path::Path;

pub(crate) use enshwick_gamedata::archive::{Archive, ContentHash};

/// The running game's archive, opened on first use; `None` when the loader
/// is not installed or the archive does not read.
#[cfg(windows)]
pub(crate) fn game() -> Option<&'static Archive> {
    if let Some(archive) = GAME.get() {
        return archive.as_ref();
    }
    let loader = crate::host::loader()?;
    open_game(loader.mods_dir.parent()?, loader.side)
}

/// The archive of the game in `dir` on `side`, opened once for the process,
/// before the loader is installed.
#[cfg(windows)]
pub(crate) fn open_game(dir: &Path, side: enshwick::abi::Side) -> Option<&'static Archive> {
    GAME.get_or_init(|| {
        let name = if side == enshwick::abi::Side::CLIENT {
            "enshrouded"
        } else {
            "enshrouded_server"
        };
        let archive = Archive::open(dir, name);
        if archive.is_none() {
            crate::log!(
                WARN,
                "the game's archive {name}.kfc does not read: attribute slots come only from a build table, vanilla names and texts are unknown"
            );
        }
        archive
    })
    .as_ref()
}

#[cfg(windows)]
static GAME: std::sync::OnceLock<Option<Archive>> = std::sync::OnceLock::new();

/// Lets go of the unpacked chunks of the game's archive, if it was opened.
#[cfg(windows)]
pub(crate) fn trim_game() {
    if let Some(Some(archive)) = GAME.get() {
        archive.trim();
    }
}
