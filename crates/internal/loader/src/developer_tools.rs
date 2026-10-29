//! Developer tools, such as the interface demo: on only where the file
//! `mod-data/enshwick/developer-tools` exists, read once at start, so a
//! server or client the player installed never runs them.

use std::path::Path;

use enshwick::abi::ManifestFlags;

/// The file that turns developer tools on, in the loader's data folder.
pub(crate) const MARKER: &str = "developer-tools";

/// Whether the `mod-data` folder `data_dir` turns developer tools on.
#[must_use]
pub(crate) fn marked(data_dir: &Path) -> bool {
    data_dir
        .join(enshwick::mods::LOADER_ID)
        .join(MARKER)
        .is_file()
}

/// Whether players are warned that developer tools are on: the `marker`
/// file exists, or a mod that runs carries
/// [`ManifestFlags::DEVELOPER`], by the flags of the `running` mods.
#[must_use]
pub(crate) fn on(marker: bool, running: impl IntoIterator<Item = ManifestFlags>) -> bool {
    marker
        || running
            .into_iter()
            .any(|flags| flags.contains(ManifestFlags::DEVELOPER))
}

/// Whether the loaded game warns that developer tools are on.
#[cfg(windows)]
#[must_use]
pub(crate) fn warned(loader: &crate::host::Loader) -> bool {
    on(
        loader.developer_tools,
        loader
            .mods
            .iter()
            .filter(|record| !record.off.load(std::sync::atomic::Ordering::Acquire))
            .map(|record| record.manifest.flags),
    )
}

/// The warning players read while developer tools are on, upper case in
/// every language.
#[cfg(windows)]
#[must_use]
pub(crate) fn warning() -> &'static str {
    crate::texts::text(WARNING)
}

/// The key of the warning's text.
#[cfg(windows)]
const WARNING: &str = "developer.on";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_or_a_running_developer_mod_turns_the_warning_on() {
        let plain = [ManifestFlags::JOIN, ManifestFlags::RELOAD];
        assert!(!on(false, plain));
        assert!(on(true, plain));
        assert!(on(false, [ManifestFlags::JOIN, ManifestFlags(4 | 1)]));
        assert!(on(false, [ManifestFlags::DEVELOPER]));
        assert!(!on(false, []));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn developer_tools_are_on_only_with_the_marker_file() {
        let dir = std::env::temp_dir().join(format!("enshwick-devtools-{}", std::process::id()));
        let folder = dir.join(enshwick::mods::LOADER_ID);
        std::fs::create_dir_all(&folder).unwrap();
        assert!(!marked(&dir));
        std::fs::create_dir_all(folder.join(MARKER)).unwrap();
        assert!(!marked(&dir), "a folder of that name is no marker");
        std::fs::remove_dir(folder.join(MARKER)).unwrap();
        std::fs::write(folder.join(MARKER), "").unwrap();
        assert!(marked(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
