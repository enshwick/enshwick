//! The running game's archive, opened once for the process; the reader is
//! [`enshwick_gamedata::archive`].

#[cfg(windows)]
use std::path::Path;

pub(crate) use enshwick_gamedata::archive::Archive;
#[cfg(windows)]
pub(crate) use enshwick_gamedata::archive::ContentHash;

/// The running game's archive, opened on first use; `None` when the loader
/// is not installed or the archive does not read.
#[cfg(windows)]
pub(crate) fn game() -> Option<&'static Archive> {
    if let Some(archive) = GAME.get() {
        return archive.as_ref();
    }
    let loader = crate::engine::loader::loader()?;
    open_game(loader.mods_dir.parent()?, loader.side)
}

/// The archive of the game in `dir` on `side`, opened once for the process,
/// before the loader is installed.
#[cfg(windows)]
pub(crate) fn open_game(dir: &Path, side: enshwick_core::abi::Side) -> Option<&'static Archive> {
    GAME.get_or_init(|| {
        let name = if side == enshwick_core::abi::Side::CLIENT {
            "enshrouded"
        } else {
            "enshrouded_server"
        };
        let archive = Archive::open(dir, name);
        if archive.is_none() {
            crate::log!(
                WARN,
                "the game's archive {name}.kfc does not read: attribute slots are missing, so what needs them stays off, and vanilla names and texts are unknown"
            );
        }
        archive
    })
    .as_ref()
}

#[cfg(windows)]
static GAME: std::sync::OnceLock<Option<Archive>> = std::sync::OnceLock::new();

/// The game's vanilla content read through its archive, built once for the
/// process, so every reader shares its indices: the template names cost
/// about 139 ms to read on a dedicated server.
#[cfg(windows)]
pub(crate) fn vanilla(
    loader: &crate::engine::loader::Loader,
) -> Result<&'static Vanilla, &'static enshwick_core::Off> {
    VANILLA
        .get_or_init(|| {
            Vanilla::new(
                &crate::engine::catalogue::Reflected(&loader.reflection),
                |key| game()?.resource(&key),
            )
        })
        .as_ref()
}

#[cfg(windows)]
use enshwick_core::content::vanilla::Vanilla;

#[cfg(windows)]
static VANILLA: std::sync::OnceLock<Result<Vanilla, enshwick_core::Off>> =
    std::sync::OnceLock::new();

/// Lets go of the unpacked chunks of the game's archive, if it was opened.
#[cfg(windows)]
pub(crate) fn trim_game() {
    if let Some(Some(archive)) = GAME.get() {
        archive.trim();
    }
}
