//! The internal crates this crate is built from, every folder of
//! `crates/internal`, so a guard over their sources follows each new crate.

use std::path::{Path, PathBuf};

/// The folder name and the source folder of each internal crate, in name
/// order.
pub(crate) fn internal_crates() -> Vec<(String, PathBuf)> {
    let internal = Path::new(env!("CARGO_MANIFEST_DIR")).join("../internal");
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(&internal)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|dir| dir.join("Cargo.toml").is_file())
        .map(|dir| {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            (name, dir.join("src"))
        })
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::internal_crates;

    #[test]
    fn every_internal_crate_is_found_with_its_sources() {
        let found = internal_crates();
        for name in ["abi", "base", "content", "core", "fake", "values"] {
            assert!(
                found.iter().any(|(dir, _)| dir == name),
                "{name}: {found:?}"
            );
        }
        assert!(found.iter().all(|(_, src)| src.is_dir()), "{found:?}");
    }
}
