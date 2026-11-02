//! Developer tools: mods with `developer: true`, dev reload and the
//! interface demo. They are on only where the file
//! [`enshwick::mods::DEVELOPER_TOOLS`] exists, read once at start, on every
//! side, so neither a dedicated server nor a game a client hosts for others
//! runs them unless its owner made the file.

use std::path::Path;

use enshwick::abi::ManifestFlags;

/// Why a mod with `flags` does not run where developer tools are `on` or
/// not; `None` when it runs. Every path that runs a mod's `init` asks it.
#[must_use]
pub(crate) fn refused(on: bool, flags: ManifestFlags) -> Option<&'static str> {
    (!on && flags.contains(ManifestFlags::DEVELOPER)).then_some(enshwick::mods::DEVELOPER_TOOL_OFF)
}

/// The mods, by their index among the manifest `flags` in load order, that
/// developer tools being `on` or not keep from running, with why: the start
/// turns them off before any `init` runs.
pub(crate) fn off(
    on: bool,
    flags: impl IntoIterator<Item = ManifestFlags>,
) -> Vec<(usize, &'static str)> {
    flags
        .into_iter()
        .enumerate()
        .filter_map(|(index, flags)| refused(on, flags).map(|why| (index, why)))
        .collect()
}

/// Whether the game in the folder `game_dir` has developer tools on.
#[must_use]
pub(crate) fn marked(game_dir: &Path) -> bool {
    game_dir.join(enshwick::mods::DEVELOPER_TOOLS).is_file()
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
    fn a_developer_mod_runs_only_with_developer_tools_on() {
        let tool = ManifestFlags::DEVELOPER;
        assert_eq!(
            refused(false, tool),
            Some(enshwick::mods::DEVELOPER_TOOL_OFF)
        );
        assert_eq!(refused(true, tool), None);
        assert_eq!(refused(false, ManifestFlags::JOIN), None);
    }

    #[test]
    fn the_start_turns_off_only_the_developer_mods_before_init() {
        let order = [
            ManifestFlags::JOIN,
            ManifestFlags::DEVELOPER,
            ManifestFlags::RELOAD,
        ];
        assert_eq!(off(false, order), [(1, enshwick::mods::DEVELOPER_TOOL_OFF)]);
        assert!(off(true, order).is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn developer_tools_are_on_only_with_the_marker_file() {
        let dir = std::env::temp_dir().join(format!("enshwick-devtools-{}", std::process::id()));
        let marker = dir.join(enshwick::mods::DEVELOPER_TOOLS);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        assert!(!marked(&dir));
        std::fs::create_dir_all(&marker).unwrap();
        assert!(!marked(&dir), "a folder of that name is no marker");
        std::fs::remove_dir(&marker).unwrap();
        std::fs::write(&marker, "").unwrap();
        assert!(marked(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
