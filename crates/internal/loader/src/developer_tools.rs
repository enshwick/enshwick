//! Developer tools, such as the interface demo: on only where the file
//! `mod-data/enshwick/developer-tools` exists, read once at start, so a
//! server or client the player installed never runs them.

use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;

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
