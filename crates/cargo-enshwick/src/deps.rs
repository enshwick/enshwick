//! `cargo enshwick deps`: what each dependency of a mod offers, written as
//! the declarations the mod compiles, from the owners' DLLs or schema files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail};
use cargo_enshwick::points::{Schema, from_dll, luau_deps, rust_deps};
use enshwick::abi::Relation;
use enshwick::packs::{PackDependency, PackManifest};

/// The schema texts `from` holds: of a schema file, of a DLL, or of every
/// DLL in a folder and in its `mods/`, each with where it came from. Each
/// mod DLL whose section does not read, which its owner must rebuild, adds
/// a line to `unread`.
fn schemas(from: &Path, unread: &mut Vec<String>) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let read = |path: &Path| std::fs::read(path).with_context(|| path.display().to_string());
    if from.extension().is_some_and(|e| e == "toml") {
        let text = String::from_utf8(read(from)?).with_context(|| from.display().to_string())?;
        return Ok(vec![(from.to_path_buf(), text)]);
    }
    let mut found = Vec::new();
    for dll in dlls(from)? {
        match from_dll(&read(&dll)?) {
            Ok(Some(text)) => found.push((dll, text)),
            Err(e @ cargo_enshwick::points::Error::Section(_)) => {
                unread.push(format!("{}: {e}", dll.display()));
            }
            // A DLL that is no mod's, such as the game's own, carries none.
            Ok(None) | Err(_) => {}
        }
    }
    Ok(found)
}

/// The DLLs `from` names: the file itself, or every DLL in the folder and in
/// its `mods/`.
fn dlls(from: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if from.is_file() {
        return Ok(vec![from.to_path_buf()]);
    }
    let mut found = Vec::new();
    for dir in [from.to_path_buf(), from.join("mods")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    if found.is_empty() {
        return Err(anyhow!("no mod DLL in {}", from.display()));
    }
    Ok(found)
}

/// The dependencies a mod's manifest names by alias in `requires`, `after`
/// and `recommends`, the ones its code reaches.
struct Dependencies {
    /// The manifest, as messages show it.
    shown: String,
    /// The folder the manifest is in, which the files go under.
    root: PathBuf,
    /// Each alias with its mod id.
    aliases: Vec<(String, String)>,
    /// The tables the aliases go in, as the manifest spells them.
    tables: &'static str,
}

/// The tables of a `Cargo.toml` that name dependencies by alias.
const CARGO_TABLES: &str = "[package.metadata.enshwick.requires], [package.metadata.enshwick.recommends] or [package.metadata.enshwick.after]";

/// The tables of a `pack.toml` that name dependencies by alias.
const PACK_TABLES: &str = "[requires], [recommends] or [after]";

impl Dependencies {
    /// Reads the `Cargo.toml` at `path`, or with `luau` the `pack.toml`.
    fn read(path: &Path, luau: bool) -> anyhow::Result<Self> {
        let shown = path.display().to_string();
        let text =
            std::fs::read_to_string(path).with_context(|| format!("{shown} does not read"))?;
        let root = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let aliases = if luau {
            PackManifest::parse(&text)
                .with_context(|| shown.clone())?
                .dependencies()
                .filter(|(relation, ..)| *relation != Relation::BREAKS)
                .map(|(_, alias, dependency)| (alias.to_owned(), dependency.id.clone()))
                .collect()
        } else {
            cargo_aliases(&text).with_context(|| shown.clone())?
        };
        Ok(Self {
            shown,
            root,
            aliases,
            tables: if luau { PACK_TABLES } else { CARGO_TABLES },
        })
    }

    /// The aliases `alias` asks for: that one, or every one when `None`.
    fn wanted(&self, alias: Option<&str>) -> anyhow::Result<Vec<&(String, String)>> {
        let wanted: Vec<_> = self
            .aliases
            .iter()
            .filter(|(known, _)| alias.is_none_or(|alias| alias == known))
            .collect();
        if wanted.is_empty() {
            let named = alias.map_or_else(
                || "no dependency".to_owned(),
                |alias| format!("no dependency `{alias}`"),
            );
            bail!(
                "{} names {named} in {}: add one there as `<alias> = \"<mod id>\"`",
                self.shown,
                self.tables
            );
        }
        Ok(wanted)
    }
}

/// The aliases of `requires`, `after` and `recommends` in the
/// `[package.metadata.enshwick]` table of the `Cargo.toml` text `text`.
fn cargo_aliases(text: &str) -> anyhow::Result<Vec<(String, String)>> {
    let cargo: toml::Table = toml::from_str(text)?;
    let table = cargo
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("enshwick"))
        .and_then(toml::Value::as_table)
        .context("no [package.metadata.enshwick] table: is this a mod's Cargo.toml?")?;
    let mut aliases = Vec::new();
    for relation in ["requires", "after", "recommends"] {
        let Some(value) = table.get(relation) else {
            continue;
        };
        let deps: BTreeMap<String, PackDependency> = value
            .clone()
            .try_into()
            .with_context(|| format!("[package.metadata.enshwick.{relation}]"))?;
        aliases.extend(deps.into_iter().map(|(alias, dep)| (alias, dep.id)));
    }
    Ok(aliases)
}

/// Writes what each dependency `alias` names, or every one, offers, read
/// from the owners' DLLs or schema file in `from`: into `src/deps/<alias>.rs`
/// beside the `Cargo.toml` at `manifest`, or with `luau` into
/// `deps/<alias>/<name>.luau` beside the `pack.toml`.
///
/// # Errors
///
/// When the manifest names no such dependency, no DLL there carries the
/// schema of the one asked for or of any, a schema does not read, or a file
/// does not write.
pub(crate) fn run(
    alias: Option<&str>,
    from: &Path,
    manifest: Option<&Path>,
    luau: bool,
) -> anyhow::Result<String> {
    let default = if luau { "pack.toml" } else { "Cargo.toml" };
    let deps = Dependencies::read(manifest.unwrap_or_else(|| Path::new(default)), luau)?;
    let wanted = deps.wanted(alias)?;
    let mut found = Vec::new();
    let mut unread = Vec::new();
    for (dll, text) in schemas(from, &mut unread)? {
        match Schema::parse(&text) {
            Ok(schema) => found.push(schema),
            // A mod DLL of another schema format: its owner or this tool
            // needs another release, which the line names.
            Err(e @ cargo_enshwick::points::Error::Format(_)) => {
                unread.push(format!("{}: {e}", dll.display()));
            }
            Err(e) => return Err(e).with_context(|| dll.display().to_string()),
        }
    }
    let mut lines: Vec<String> = unread
        .iter()
        .map(|why| format!("left out, rebuild its mod: {why}"))
        .collect();
    let mut written = 0;
    for (alias, id) in wanted {
        let Some(schema) = found.iter().find(|s| s.owner() == id) else {
            lines.push(format!(
                "{alias}: no DLL in {} carries what {id} offers",
                from.display()
            ));
            continue;
        };
        if luau {
            let dir = deps.root.join("deps").join(alias);
            let modules = luau_deps(schema, alias)?;
            std::fs::create_dir_all(&dir).with_context(|| dir.display().to_string())?;
            for (name, text) in &modules.modules {
                let path = dir.join(format!("{name}.luau"));
                std::fs::write(&path, text).with_context(|| path.display().to_string())?;
            }
            lines.push(format!(
                "wrote {} module(s) of {id} {} to {}",
                modules.modules.len(),
                schema.version(),
                dir.display()
            ));
            lines.extend(modules.skipped.iter().map(|why| format!("left out: {why}")));
        } else {
            let dir = deps.root.join("src").join("deps");
            let path = dir.join(format!("{alias}.rs"));
            let code = rust_deps(schema, alias)?;
            std::fs::create_dir_all(&dir).with_context(|| dir.display().to_string())?;
            std::fs::write(&path, code).with_context(|| path.display().to_string())?;
            lines.push(format!(
                "wrote {id} {} to {}",
                schema.version(),
                path.display()
            ));
        }
        written += 1;
    }
    let report = lines.join("\n");
    if written == 0 {
        return Err(anyhow!(report));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARGO: &str = r#"[package]
name = "haggling"

[package.metadata.enshwick]
id = "you.haggling"
side = "server"

[package.metadata.enshwick.requires]
lockers = { id = "you.lockers", version = ">=0.1" }

[package.metadata.enshwick.recommends]
trading_post = "you.trading-post"

[package.metadata.enshwick.breaks]
old = "you.old"
"#;

    /// A mod folder of its own with `manifest` at `file`, empty of anything
    /// written before.
    fn folder(name: &str, file: &str, manifest: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("enshwick-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file), manifest).unwrap();
        dir
    }

    fn schema(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn the_aliases_of_a_cargo_toml_leave_out_breaks() {
        assert_eq!(
            cargo_aliases(CARGO).unwrap(),
            [
                ("lockers".to_owned(), "you.lockers".to_owned()),
                ("trading_post".to_owned(), "you.trading-post".to_owned())
            ]
        );
    }

    #[test]
    fn a_dependency_is_written_by_its_alias_into_src_deps() {
        let dir = folder("deps-rust", "Cargo.toml", CARGO);
        let done = run(
            Some("lockers"),
            &schema("lockers.points.toml"),
            Some(&dir.join("Cargo.toml")),
            false,
        )
        .unwrap();
        assert!(done.starts_with("wrote you.lockers 1.2.0"), "{done}");
        let code = std::fs::read_to_string(dir.join("src/deps/lockers.rs")).unwrap();
        assert!(code.contains("name = \"price\", of = lockers"), "{code}");
    }

    #[test]
    fn a_name_without_an_alias_fails_and_names_the_tables_to_add_it_to() {
        let dir = folder("deps-unknown", "Cargo.toml", CARGO);
        let error = run(
            Some("stalls"),
            &schema("lockers.points.toml"),
            Some(&dir.join("Cargo.toml")),
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("names no dependency `stalls` in [package.metadata.enshwick.requires], [package.metadata.enshwick.recommends] or [package.metadata.enshwick.after]"),
            "{error}"
        );
        let old = run(
            Some("old"),
            &schema("lockers.points.toml"),
            Some(&dir.join("Cargo.toml")),
            false,
        );
        assert!(old.is_err(), "a mod it breaks is no dependency to name");
    }

    #[test]
    fn a_dependency_no_schema_describes_is_named() {
        let dir = folder("deps-missing", "Cargo.toml", CARGO);
        let manifest = dir.join("Cargo.toml");
        let error = run(
            Some("trading_post"),
            &schema("lockers.points.toml"),
            Some(&manifest),
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("trading_post: no DLL in") && error.contains("you.trading-post"),
            "{error}"
        );
        let every = run(None, &schema("lockers.points.toml"), Some(&manifest), false).unwrap();
        assert!(
            every.contains("wrote you.lockers") && every.contains("trading_post: no DLL"),
            "{every}"
        );
    }

    #[test]
    fn a_luau_mod_gets_a_module_per_name_beside_its_pack_toml() {
        let pack =
            "id = \"you.lamps\"\nversion = \"1.0.0\"\n\n[after]\nlockers = \"you.lockers\"\n";
        let dir = folder("deps-luau", "pack.toml", pack);
        let done = run(
            None,
            &schema("lockers.calls.toml"),
            Some(&dir.join("pack.toml")),
            true,
        )
        .unwrap();
        assert!(
            done.contains("left out: the call you.lockers:rent"),
            "{done}"
        );
        let fee = std::fs::read_to_string(dir.join("deps/lockers/fee.luau")).unwrap();
        assert!(
            fee.contains("`cargo enshwick deps lockers --luau`"),
            "{fee}"
        );
        assert!(!dir.join("deps/lockers/rent.luau").exists());
        let error = run(
            None,
            &schema("lockers.calls.toml"),
            Some(&dir.join("pack.toml")),
            false,
        )
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("[package.metadata.enshwick]"),
            "{error:#}"
        );
    }
}
