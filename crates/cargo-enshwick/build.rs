//! Reads which build table ids each part of the loader names, from the
//! loader's sources beside this crate, for `cargo enshwick compat`. Without
//! those sources, as outside the repository, the list is empty.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let loader = root.join("../internal/loader/src");
    println!("cargo::rerun-if-changed={}", loader.display());
    let mut modules: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in sources(&loader) {
        println!("cargo::rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(&path)?;
        let Some(module) = path
            .strip_prefix(&loader)
            .ok()
            .and_then(|p| p.components().next())
            .and_then(|c| c.as_os_str().to_str())
            .map(|c| c.trim_end_matches(".rs").to_owned())
        else {
            continue;
        };
        modules.entry(module).or_default().extend(ids(&text));
    }
    let mut out = String::from("/// The ids each part of the loader names.\n");
    out.push_str("pub(crate) static MODULE_IDS: &[(&str, &[&str])] = &[\n");
    for (module, ids) in &modules {
        writeln!(
            out,
            "    ({module:?}, &{:?}),",
            ids.iter().collect::<Vec<_>>()
        )?;
    }
    out.push_str("];\n");
    std::fs::write(
        PathBuf::from(std::env::var("OUT_DIR")?).join("loader_map.rs"),
        out,
    )?;
    Ok(())
}

/// Every `.rs` file under `dir`, sorted.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The string literals in `text` shaped like table ids: `area.name`, lower
/// case, at least one dot.
fn ids(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for literal in text.split('"').skip(1).step_by(2) {
        let shaped = literal.contains('.')
            && literal.starts_with(|c: char| c.is_ascii_lowercase())
            && !literal.ends_with('.')
            && literal
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
        if shaped {
            out.insert(literal.to_owned());
        }
    }
    out
}
