//! `cargo enshwick new`: a mod crate that builds and loads as it is.

use std::path::{Path, PathBuf};

use crate::Side;

/// Creates the crate for mod `id` in `<dir>/<id>`; what was made, for the user.
pub(crate) fn create(dir: &Path, id: &str, side: Side) -> Result<String, String> {
    let root = dir.join(id);
    if root.exists() {
        return Err(format!("{} already exists", root.display()));
    }
    for (path, text) in files(id, side)? {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(format!(
        "created mod `{id}` in {}; `cargo enshwick install --game <game folder>` puts it into the game",
        root.display()
    ))
}

/// The crate's files for mod `id`, relative to its root.
fn files(id: &str, side: Side) -> Result<Vec<(PathBuf, String)>, String> {
    if !enshwick_loader_api::abi::is_valid_id(id) {
        return Err(format!(
            "the id {id:?} is not 1 to 64 characters of [a-z0-9-]"
        ));
    }
    let side = match side {
        Side::Server => "SERVER",
        Side::Client => "CLIENT",
        Side::Both => "BOTH",
    };
    let fill = |template: &str| {
        template
            .replace("{id}", id)
            .replace("{type}", &type_name(id))
            .replace("{name}", &display_name(id))
            .replace("{side}", side)
            .replace("{api}", enshwick_loader_api::VERSION)
    };
    Ok(vec![
        ("Cargo.toml".into(), fill(CARGO_TOML)),
        ("src/lib.rs".into(), fill(LIB_RS)),
        ("README.md".into(), fill(README)),
        (".gitignore".into(), "/target\n".into()),
    ])
}

/// `flame-altar-zones` as `Flame altar zones`, the name players read until
/// the author picks one.
fn display_name(id: &str) -> String {
    let words = id.replace('-', " ");
    let mut chars = words.trim().chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + chars.as_str()
    })
}

/// `flame-altar-zones` as `FlameAltarZones`; ids starting with a digit get `Mod`.
fn type_name(id: &str) -> String {
    let mut name: String = id
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect();
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        name.insert_str(0, "Mod");
    }
    name
}

const CARGO_TOML: &str = r#"[package]
name = "{id}"
version = "0.1.0"
edition = "2024"
# The loader shows these to players: in `/enshwick {id}`, the status summary
# and the updater's notes.
description = "What {name} does, in one sentence"
authors = ["Your Name"]
license = "MIT OR Apache-2.0"
homepage = "https://github.com/you/your-repo"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
enshwick-loader-api = { git = "https://github.com/themaiby/enshwick", tag = "enshwick-loader-api-v{api}" }
serde = { version = "1", features = ["derive"] }

[profile.release]
lto = "fat"
codegen-units = 1
# Unwinding stays on: every detour catches panics and falls back to vanilla.
panic = "unwind"
"#;

const LIB_RS: &str = r#"//! The {id} mod for Enshrouded.

use enshwick_loader_api::{Load, Mod, Off};

/// {type} settings, in `mod-configs/{id}.toml`.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Whether the mod does its work.
    // `startup` (needs a restart), `runtime` (applied by on_config_change),
    // `client` (each player's own) or `sync` (the server's value), and for
    // numbers `range = 1..=10`, for lists of values `choices = [..]`.
    #[config(runtime)]
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct {type};

impl Mod for {type} {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "{id}",
            side: {side},
            // What players read; description, authors, license and homepage
            // come from Cargo.toml.
            name: "{name}",
            // Mods this one works better with, and mods it does not work
            // beside, each with a version range ("" for any):
            // recommends: ["other-mod" => ">=0.2, <1.0"],
            // breaks: ["old-mod" => "<0.3"],
            // After a rename, the old ids, whose DLLs then never start:
            // replaces: ["old-id"],
            // Players get your releases from the updater once you name where
            // they come from and the minisign key you sign them with; see
            // "Releasing and updates" in the modding book.
            // updates: github("you/your-repo", "{id}"),
            // signed_by: ["RW...: the second line of your minisign .pub file"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        enshwick_loader_api::info!("enabled: {}", settings.enabled);
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!({type});
"#;

const README: &str = "# {id}

An Enshrouded mod for the [Enshwick](https://github.com/themaiby/enshwick) loader.

`cargo enshwick install --game <game folder>` builds it and copies it into the
game's `mods/` folder. Its settings are `mod-configs/{id}.toml`, written on first
start; its log is `logs/enshwick/mods/{id}.log`.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_become_type_names() {
        assert_eq!(type_name("flame-altar-zones"), "FlameAltarZones");
        assert_eq!(type_name("safes"), "Safes");
        assert_eq!(type_name("2x-loot"), "Mod2xLoot");
    }

    #[test]
    fn the_files_name_the_mod_everywhere() {
        let files = files("area-loot", Side::Both).unwrap();
        let text = |name: &str| {
            &files
                .iter()
                .find(|(path, _)| path == Path::new(name))
                .unwrap()
                .1
        };
        assert!(text("Cargo.toml").contains("name = \"area-loot\""));
        assert!(text("Cargo.toml").contains(&format!(
            "enshwick-loader-api-v{}",
            enshwick_loader_api::VERSION
        )));
        assert!(text("src/lib.rs").contains("id: \"area-loot\",\n            side: BOTH,"));
        assert!(text("src/lib.rs").contains("// signed_by: [\"RW..."));
        assert!(text("src/lib.rs").contains("declare_mod!(AreaLoot)"));
        assert!(!files.iter().any(|(_, text)| text.contains("{type}")
            || text.contains("{id}")
            || text.contains("{name}")));
    }

    #[test]
    fn ids_become_names() {
        assert_eq!(display_name("flame-altar-zones"), "Flame altar zones");
        assert_eq!(display_name("2x-loot"), "2x loot");
    }

    #[test]
    fn a_bad_id_is_refused() {
        assert!(files("Area Loot", Side::Server).is_err());
    }
}
