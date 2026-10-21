//! `cargo enshwick new`: a mod crate that builds and loads as it is.

use std::path::{Path, PathBuf};

use crate::Side;

/// Creates the crate for mod `id` in `<dir>/<id>`; what was made, for the user.
///
/// Inside a Cargo workspace the crate joins it: its folder is added to the
/// workspace's `members` unless a member pattern already covers it, and its
/// release profile is left to the workspace root, the only place Cargo reads
/// one. A folder the workspace excludes gets a workspace of its own.
pub(crate) fn create(dir: &Path, id: &str, side: Side) -> Result<String, String> {
    let root = std::path::absolute(dir.join(id)).map_err(|e| format!("{}: {e}", dir.display()))?;
    if root.exists() {
        return Err(format!("{} already exists", root.display()));
    }
    let plan = match enclosing_workspace(&root)? {
        Some(manifest) => plan_join(&manifest, &root)?,
        None => Place::Alone,
    };
    for (path, text) in files(id, side, &plan)? {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let joined = match &plan {
        Place::Join { manifest, text } => {
            if let Err(e) = std::fs::write(manifest, text) {
                let _gone = std::fs::remove_dir_all(&root);
                return Err(format!("{}: {e}", manifest.display()));
            }
            format!(
                "; added to the members of {}, whose root holds the release profile",
                manifest.display()
            )
        }
        Place::Member { manifest } => format!(
            "; a member of {}, whose root holds the release profile",
            manifest.display()
        ),
        Place::Alone | Place::Excluded => String::new(),
    };
    Ok(format!(
        "created mod `{id}` in {}{joined}; `cargo enshwick install --game <game folder>` puts it into the game",
        root.display()
    ))
}

/// Where a new crate stands towards the workspace around it.
#[derive(Debug, PartialEq, Eq)]
enum Place {
    /// No workspace around it.
    Alone,
    /// A member pattern of the workspace at `manifest` covers it.
    Member { manifest: PathBuf },
    /// The workspace at `manifest` takes it once `text` is written there.
    Join { manifest: PathBuf, text: String },
    /// The workspace around it excludes it, so it is a workspace of its own.
    Excluded,
}

/// The `Cargo.toml` with a `[workspace]` table in the nearest folder above
/// `root`, as Cargo looks for it.
fn enclosing_workspace(root: &Path) -> Result<Option<PathBuf>, String> {
    for dir in root.ancestors().skip(1) {
        let manifest = dir.join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e| format!("{}: {e}", manifest.display()))?;
        if doc.contains_key("workspace") {
            return Ok(Some(manifest));
        }
    }
    Ok(None)
}

/// How the crate at `root` fits the workspace whose manifest is `manifest`.
fn plan_join(manifest: &Path, root: &Path) -> Result<Place, String> {
    let text =
        std::fs::read_to_string(manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let base = manifest.parent().unwrap_or(Path::new(""));
    let member = relative(base, root);
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| format!("{}: {e}", manifest.display()))?;
    let patterns = |key: &str| -> Vec<String> {
        doc.get("workspace")
            .and_then(|w| w.get(key))
            .and_then(toml_edit::Item::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    if patterns("exclude")
        .iter()
        .any(|p| member.starts_with(&format!("{p}/")) || member == *p)
    {
        return Ok(Place::Excluded);
    }
    if patterns("members").iter().any(|p| matches(p, &member)) {
        return Ok(Place::Member {
            manifest: manifest.to_owned(),
        });
    }
    let workspace = doc["workspace"]
        .as_table_like_mut()
        .ok_or_else(|| format!("{}: `workspace` is not a table", manifest.display()))?;
    let members = workspace
        .entry("members")
        .or_insert(toml_edit::value(toml_edit::Array::new()))
        .as_array_mut()
        .ok_or_else(|| {
            format!(
                "{}: `workspace.members` is not an array",
                manifest.display()
            )
        })?;
    members.push(member);
    Ok(Place::Join {
        manifest: manifest.to_owned(),
        text: doc.to_string(),
    })
}

/// `path` relative to `base`, with `/` between its parts, as Cargo writes
/// members.
fn relative(base: &Path, path: &Path) -> String {
    let rest = path.strip_prefix(base).unwrap_or(path);
    rest.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether the member pattern `pattern` names `path`, both relative to the
/// workspace root: per part, `*` for any run of characters and `?` for one.
fn matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern
        .trim_end_matches('/')
        .split('/')
        .filter(|p| *p != ".")
        .collect();
    let path: Vec<&str> = path.split('/').collect();
    pattern.len() == path.len()
        && pattern
            .iter()
            .zip(&path)
            .all(|(p, s)| part(p.as_bytes(), s.as_bytes()))
}

fn part(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.split_first(), text.split_first()) {
        (None, None) => true,
        (Some((b'*', rest)), _) => {
            part(rest, text) || (!text.is_empty() && part(pattern, &text[1..]))
        }
        (Some((b'?', rest)), Some((_, tail))) => part(rest, tail),
        (Some((p, rest)), Some((t, tail))) => p == t && part(rest, tail),
        _ => false,
    }
}

/// The crate's files for mod `id`, relative to its root.
fn files(id: &str, side: Side, place: &Place) -> Result<Vec<(PathBuf, String)>, String> {
    // The id names the crate too, and a crate's name starts with a letter.
    if !enshwick_loader_api::abi::is_valid_id(id)
        || !id.starts_with(|c: char| c.is_ascii_lowercase())
    {
        return Err(format!(
            "the id {id:?} is not 1 to 64 characters of [a-z0-9-] starting with a letter"
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
            .replace("{api}", &api_source())
    };
    let cargo = match place {
        Place::Alone => fill(CARGO_TOML) + PROFILE,
        Place::Excluded => fill(CARGO_TOML) + PROFILE + "\n[workspace]\n",
        Place::Member { .. } | Place::Join { .. } => fill(CARGO_TOML),
    };
    Ok(vec![
        ("Cargo.toml".into(), cargo),
        ("src/lib.rs".into(), fill(LIB_RS)),
        ("README.md".into(), fill(README)),
        (".gitignore".into(), "/target\n".into()),
    ])
}

/// Where a new mod takes the API from: the tag of this API's release, or
/// `main` for a development build, which has no release.
fn api_source() -> String {
    let version = enshwick_loader_api::VERSION;
    if version.contains('-') {
        "branch = \"main\"".to_owned()
    } else {
        format!("tag = \"enshwick-loader-api-v{version}\"")
    }
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
enshwick-loader-api = { git = "https://github.com/themaiby/enshwick", {api} }
serde = { version = "1", features = ["derive"] }
"#;

/// The release profile of a crate that is its own workspace root.
const PROFILE: &str = r#"
[profile.release]
lto = "fat"
codegen-units = 1
# Unwinding stays on: every detour catches panics and falls back to vanilla.
panic = "unwind"
"#;

const LIB_RS: &str = r#"//! The {id} mod for Enshrouded.

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_loader_api::{Load, Mod, Off};

/// Whether the mod does its work now; a settings change updates it.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// {type} settings, in `mod-configs/{id}.toml`.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Whether the mod does its work.
    // `startup` (applied when a world loads), `runtime` (applied by
    // on_config_change), `client` (each player's own), `sync` (the server's
    // value) or `restart` (read once as the game starts), and for numbers
    // `range = 1..=10`, for lists of values `choices = [..]`.
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
        ENABLED.store(settings.enabled, Ordering::Relaxed);
        load.on_config_change(|settings: Settings| {
            ENABLED.store(settings.enabled, Ordering::Relaxed);
        })?;
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
    fn an_id_that_cannot_name_a_crate_is_refused() {
        assert!(files("2x-loot", Side::Server, &Place::Alone).is_err());
        assert!(files("-loot", Side::Server, &Place::Alone).is_err());
        assert!(files("loot-2x", Side::Server, &Place::Alone).is_ok());
    }

    #[test]
    fn the_files_name_the_mod_everywhere() {
        let files = files("area-loot", Side::Both, &Place::Alone).unwrap();
        let text = |name: &str| {
            &files
                .iter()
                .find(|(path, _)| path == Path::new(name))
                .unwrap()
                .1
        };
        assert!(text("Cargo.toml").contains("name = \"area-loot\""));
        assert!(text("Cargo.toml").contains(&api_source()));
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
    fn member_patterns_match_per_part() {
        assert!(matches("mods/*", "mods/area-loot"));
        assert!(matches("mods/*/", "mods/area-loot"));
        assert!(matches("./mods/area-?oot", "mods/area-loot"));
        assert!(matches("mods/area-loot", "mods/area-loot"));
        assert!(!matches("mods/*", "mods/area-loot/inner"));
        assert!(!matches("crates/*", "mods/area-loot"));
        assert!(!matches("mods/a*z", "mods/area-loot"));
    }

    /// A fresh folder for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("enshwick-new-{name}-{}", std::process::id()));
        let _old = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_crate_inside_a_workspace_joins_its_members() {
        let dir = scratch("join");
        let manifest = dir.join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[workspace]\nresolver = \"3\"\n# kept\nmembers = [\"tools/x\"]\n",
        )
        .unwrap();
        create(&dir.join("mods"), "area-loot", Side::Server).unwrap();
        let root = std::fs::read_to_string(&manifest).unwrap();
        assert!(root.contains("# kept"), "{root}");
        assert!(
            root.contains("members = [\"tools/x\", \"mods/area-loot\"]"),
            "{root}"
        );
        let cargo = std::fs::read_to_string(dir.join("mods/area-loot/Cargo.toml")).unwrap();
        assert!(!cargo.contains("[profile.release]") && !cargo.contains("[workspace]"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_member_pattern_or_an_exclude_leaves_the_workspace_as_it_is() {
        let dir = scratch("covered");
        let manifest = dir.join("Cargo.toml");
        let text = "[workspace]\nmembers = [\"mods/*\"]\nexclude = [\"scratch\"]\n";
        std::fs::write(&manifest, text).unwrap();
        create(&dir.join("mods"), "safes", Side::Server).unwrap();
        create(&dir.join("scratch"), "probe", Side::Server).unwrap();
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), text);
        let probe = std::fs::read_to_string(dir.join("scratch/probe/Cargo.toml")).unwrap();
        assert!(probe.contains("[profile.release]") && probe.ends_with("\n[workspace]\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_crate_outside_any_workspace_keeps_its_profile() {
        let place = Place::Alone;
        let files = files("safes", Side::Server, &place).unwrap();
        assert!(files[0].1.contains("[profile.release]\nlto = \"fat\""));
        assert!(!files[0].1.contains("[workspace]"));
    }

    #[test]
    fn a_bad_id_is_refused() {
        assert!(files("Area Loot", Side::Server, &Place::Alone).is_err());
    }
}
