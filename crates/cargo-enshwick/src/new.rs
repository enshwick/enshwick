//! `cargo enshwick new`: a mod crate that builds and loads as it is.

use anyhow::{Context as _, bail};
use std::path::{Path, PathBuf};

use crate::Side;

/// Creates the crate for mod `id`, `<author>.<name>`, in `<dir>/<name>`;
/// what was made, for the user.
///
/// Inside a Cargo workspace the crate joins it: its folder is added to the
/// workspace's `members` unless a member pattern already covers it, and its
/// release profile is left to the workspace root, the only place Cargo reads
/// one. A folder the workspace excludes gets a workspace of its own.
pub(crate) fn create(dir: &Path, id: &str, side: Side) -> anyhow::Result<String> {
    let name = crate_name(id)?;
    let root = std::path::absolute(dir.join(name)).with_context(|| dir.display().to_string())?;
    if root.exists() {
        bail!("{} already exists", root.display());
    }
    let plan = match enclosing_workspace(&root)? {
        Some(manifest) => plan_join(&manifest, &root)?,
        None => Place::Alone,
    };
    for (path, text) in files(id, side, &plan)? {
        let path = root.join(path);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, text));
        if let Err(e) = written {
            let _gone = std::fs::remove_dir_all(&root);
            bail!("{}: {e}", path.display());
        }
    }
    let joined = match &plan {
        Place::Join { manifest, text } => {
            if let Err(e) = std::fs::write(manifest, text) {
                let _gone = std::fs::remove_dir_all(&root);
                bail!("{}: {e}", manifest.display());
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
fn enclosing_workspace(root: &Path) -> anyhow::Result<Option<PathBuf>> {
    for dir in root.ancestors().skip(1) {
        let manifest = dir.join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let doc: toml_edit::DocumentMut = text
            .parse()
            .with_context(|| manifest.display().to_string())?;
        if doc.contains_key("workspace") {
            return Ok(Some(manifest));
        }
    }
    Ok(None)
}

/// How the crate at `root` fits the workspace whose manifest is `manifest`.
fn plan_join(manifest: &Path, root: &Path) -> anyhow::Result<Place> {
    let text = std::fs::read_to_string(manifest).with_context(|| manifest.display().to_string())?;
    let base = manifest.parent().unwrap_or(Path::new(""));
    let member = relative(base, root);
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| manifest.display().to_string())?;
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
    if patterns("exclude").iter().any(|p| {
        let p = p.trim_start_matches("./").trim_end_matches('/');
        member.starts_with(&format!("{p}/")) || member == p
    }) {
        return Ok(Place::Excluded);
    }
    if patterns("members").iter().any(|p| matches(p, &member)) {
        return Ok(Place::Member {
            manifest: manifest.to_owned(),
        });
    }
    let workspace = doc["workspace"]
        .as_table_like_mut()
        .with_context(|| format!("{}: `workspace` is not a table", manifest.display()))?;
    let members = workspace
        .entry("members")
        .or_insert(toml_edit::value(toml_edit::Array::new()))
        .as_array_mut()
        .with_context(|| {
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

/// The crate's name for mod `id`: the id's name part, which must start with
/// a letter, as a crate's name does; an id no mod can take is refused.
fn crate_name(id: &str) -> anyhow::Result<&str> {
    match id.split_once('.') {
        Some((_, name))
            if enshwick::abi::is_valid_id(id)
                && name.starts_with(|c: char| c.is_ascii_lowercase()) =>
        {
            Ok(name)
        }
        _ => bail!(
            "the id {id:?} is not <author>.<name>, each of [a-z0-9-], the name starting with a letter"
        ),
    }
}

/// The crate's files for mod `id`, relative to its root.
fn files(id: &str, side: Side, place: &Place) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let name = crate_name(id)?;
    let side = match side {
        Side::Server => "SERVER",
        Side::Client => "CLIENT",
        Side::Both => "BOTH",
    };
    let fill = |template: &str| {
        template
            .replace("{id}", id)
            .replace("{crate}", name)
            .replace("{type}", &type_name(name))
            .replace("{name}", &display_name(name))
            .replace("{side}", side)
            .replace("{api}", enshwick::VERSION)
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

/// `street-lanterns` as `Street lanterns`, the name players read until
/// the author picks one.
fn display_name(id: &str) -> String {
    let words = id.replace('-', " ");
    let mut chars = words.trim().chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + chars.as_str()
    })
}

/// `street-lanterns` as `StreetLanterns`; names starting with a digit get `Mod`.
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
name = "{crate}"
version = "0.1.0"
edition = "2024"
# The loader shows these to players on the Mods screen and in the status
# summary; the homepage is where players are sent to get the mod.
description = "What {name} does, in one sentence"
authors = ["Your Name"]
license = "MIT OR Apache-2.0"
homepage = "https://github.com/you/your-repo"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
enshwick = "{api}"
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

use enshwick::{Load, Mod, Off};

/// Whether the mod does its work now; a settings change updates it.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// {type} settings, in `mod-configs/{id}.toml`.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
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
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(
            id: "{id}",
            side: {side},
            // What players read; description, authors, license and homepage
            // come from Cargo.toml.
            name: "{name}",
            // Mods this one works better with, and mods it does not work
            // beside, each with a version range ("" for any):
            // recommends: ["other-mod" => ">=0.2, <1.0"],
            // breaks: ["old-mod" => "<0.3"],
            // After a rename, the old ids: their DLLs never start, and
            // their settings and data carry over on the first start:
            // was: ["old-id"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        ENABLED.store(settings.enabled, Ordering::Relaxed);
        load.on_config_change(|settings: Settings| {
            ENABLED.store(settings.enabled, Ordering::Relaxed);
        })?;
        enshwick::info!("enabled: {}", settings.enabled);
        Ok(Self)
    }
}

enshwick::declare_mod!({type});
"#;

const README: &str = "# {id}

An Enshrouded mod for the [Enshwick](https://github.com/enshwick/enshwick) loader.

`cargo enshwick install --game <game folder>` builds it and copies it into the
game's `mods/` folder. Its settings are `mod-configs/{id}.toml`, written on first
start; its log is `logs/enshwick/mods/{id}.log`.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_become_type_names() {
        assert_eq!(type_name("street-lanterns"), "StreetLanterns");
        assert_eq!(type_name("lockers"), "Lockers");
        assert_eq!(type_name("2x-loot"), "Mod2xLoot");
    }

    #[test]
    fn an_id_that_cannot_name_a_crate_is_refused() {
        assert!(files("you.2x-loot", Side::Server, &Place::Alone).is_err());
        assert!(files("you.-loot", Side::Server, &Place::Alone).is_err());
        assert!(files("you.loot-2x", Side::Server, &Place::Alone).is_ok());
        assert!(files("lanterns", Side::Server, &Place::Alone).is_err());
    }

    #[test]
    fn the_files_name_the_mod_everywhere() {
        let files = files("you.street-lanterns", Side::Both, &Place::Alone).unwrap();
        let text = |name: &str| {
            &files
                .iter()
                .find(|(path, _)| path == Path::new(name))
                .unwrap()
                .1
        };
        assert!(text("Cargo.toml").contains("name = \"street-lanterns\""));
        assert!(text("Cargo.toml").contains(&format!("enshwick = \"{}\"", enshwick::VERSION)));
        assert!(
            text("src/lib.rs").contains("id: \"you.street-lanterns\",\n            side: BOTH,")
        );
        assert!(text("src/lib.rs").contains("// was: [\"old-id\"]"));
        assert!(text("src/lib.rs").contains("declare_mod!(StreetLanterns)"));
        assert!(!files.iter().any(|(_, text)| text.contains("{type}")
            || text.contains("{id}")
            || text.contains("{crate}")
            || text.contains("{name}")));
    }

    #[test]
    fn ids_become_names() {
        assert_eq!(display_name("street-lanterns"), "Street lanterns");
        assert_eq!(display_name("2x-loot"), "2x loot");
    }

    #[test]
    fn member_patterns_match_per_part() {
        assert!(matches("mods/*", "mods/street-lanterns"));
        assert!(matches("mods/*/", "mods/street-lanterns"));
        assert!(matches("./mods/street-?anterns", "mods/street-lanterns"));
        assert!(matches("mods/street-lanterns", "mods/street-lanterns"));
        assert!(!matches("mods/*", "mods/street-lanterns/inner"));
        assert!(!matches("crates/*", "mods/street-lanterns"));
        assert!(!matches("mods/a*z", "mods/street-lanterns"));
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
        create(&dir.join("mods"), "you.street-lanterns", Side::Server).unwrap();
        let root = std::fs::read_to_string(&manifest).unwrap();
        assert!(root.contains("# kept"), "{root}");
        assert!(
            root.contains("members = [\"tools/x\", \"mods/street-lanterns\"]"),
            "{root}"
        );
        let cargo = std::fs::read_to_string(dir.join("mods/street-lanterns/Cargo.toml")).unwrap();
        assert!(!cargo.contains("[profile.release]") && !cargo.contains("[workspace]"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_member_pattern_or_an_exclude_leaves_the_workspace_as_it_is() {
        let dir = scratch("covered");
        let manifest = dir.join("Cargo.toml");
        let text = "[workspace]\nmembers = [\"mods/*\"]\nexclude = [\"scratch\"]\n";
        std::fs::write(&manifest, text).unwrap();
        create(&dir.join("mods"), "you.lockers", Side::Server).unwrap();
        create(&dir.join("scratch"), "you.my-mod", Side::Server).unwrap();
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), text);
        let created = std::fs::read_to_string(dir.join("scratch/my-mod/Cargo.toml")).unwrap();
        assert!(created.contains("[profile.release]") && created.ends_with("\n[workspace]\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_exclude_written_with_slashes_still_excludes() {
        let dir = scratch("slashes");
        let manifest = dir.join("Cargo.toml");
        let text = "[workspace]\nmembers = []\nexclude = [\"./scratch/\"]\n";
        std::fs::write(&manifest, text).unwrap();
        create(&dir.join("scratch"), "you.my-mod", Side::Server).unwrap();
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), text);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bad_id_is_refused_before_the_folder_is_looked_at() {
        let dir = scratch("bad-id");
        std::fs::create_dir_all(dir.join("Area Loot")).unwrap();
        let refused = create(&dir, "Area Loot", Side::Server)
            .unwrap_err()
            .to_string();
        assert!(refused.contains("is not <author>.<name>"), "{refused}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_crate_outside_any_workspace_keeps_its_profile() {
        let place = Place::Alone;
        let files = files("you.lockers", Side::Server, &place).unwrap();
        assert!(files[0].1.contains("[profile.release]\nlto = \"fat\""));
        assert!(!files[0].1.contains("[workspace]"));
    }

    #[test]
    fn a_bad_id_is_refused() {
        assert!(files("Area Loot", Side::Server, &Place::Alone).is_err());
    }
}
