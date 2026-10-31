//! `cargo enshwick new`: a mod crate that builds and loads as it is.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};

use crate::Side;

pub(crate) mod data;
pub(crate) mod luau;

/// Creates the crate for mod `id`, `<author>.<name>`, in `<dir>/<name>`, on
/// the crates.io release or on `commit` of the platform; what was made, for
/// the user.
///
/// Inside a Cargo workspace the crate joins it: its folder is added to the
/// workspace's `members` unless a member pattern already covers it, and the
/// release profile, the toolchain and the build flags are left to the
/// workspace root, the only place Cargo reads them. A folder the workspace
/// excludes gets a workspace of its own.
pub(crate) fn create(
    dir: &Path,
    id: &str,
    side: Side,
    commit: Option<&Commit>,
) -> anyhow::Result<String> {
    let name = crate_name(id)?;
    let root = std::path::absolute(dir.join(name)).with_context(|| dir.display().to_string())?;
    if root.exists() {
        bail!("{} already exists", root.display());
    }
    let plan = match enclosing_workspace(&root)? {
        Some(manifest) => plan_join(&manifest, &root)?,
        None => Place::Alone,
    };
    for (path, text) in files(id, side, commit, &plan)? {
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
                "; added to the members of {}, whose root holds the release profile, the toolchain and the build flags",
                manifest.display()
            )
        }
        Place::Member { manifest, .. } => format!(
            "; a member of {}, whose root holds the release profile, the toolchain and the build flags",
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
    /// A member pattern of the workspace at `manifest`, whose text is
    /// `root`, covers it.
    Member { manifest: PathBuf, root: String },
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
            root: text,
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
fn files(
    id: &str,
    side: Side,
    commit: Option<&Commit>,
    place: &Place,
) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let name = crate_name(id)?;
    let side = match side {
        Side::Server => "SERVER",
        Side::Client => "CLIENT",
        Side::Both => "BOTH",
    };
    let shown = display_name(name);
    let description = format!("What {shown} does, in one sentence");
    let enshwick = match commit {
        None => format!("enshwick = \"={}\"\n", enshwick::VERSION),
        Some(commit) => format!(
            "# The platform from git, at a commit of its main branch.\nenshwick = {{ git = \"{PLATFORM_GIT}\", rev = \"{}\" }}\n",
            commit.0
        ),
    };
    let fill = |template: &str| {
        template
            .replace("{id}", id)
            .replace("{crate}", name)
            .replace("{type}", &type_name(name))
            .replace("{name}", &shown)
            .replace("{side}", side)
            .replace("{description}", &description)
            .replace("{enshwick}", &enshwick)
    };
    let cargo = fill(CARGO_TOML);
    let mut files = vec![
        ("src/lib.rs".into(), fill(LIB_RS)),
        ("src/texts.toml".into(), fill(TEXTS_TOML)),
        ("README.md".into(), fill(README)),
        (".gitignore".into(), "/target/\n".into()),
    ];
    match place {
        Place::Alone | Place::Excluded => {
            files.push(("Cargo.toml".into(), cargo));
            files.push(("rust-toolchain.toml".into(), RUST_TOOLCHAIN.into()));
            files.push((".cargo/config.toml".into(), CARGO_CONFIG.into()));
        }
        Place::Member { root, .. } | Place::Join { text: root, .. } => {
            files.push(("Cargo.toml".into(), as_member(&cargo, root)?));
        }
    }
    Ok(files)
}

/// The crate's own `Cargo.toml` as a member of the workspace whose root
/// manifest is `root`: the workspace tables and the profiles go, since
/// Cargo reads them only at the root; lints and dependencies stay taken from
/// the workspace where it declares them and are written out where it does
/// not.
fn as_member(cargo: &str, root: &str) -> anyhow::Result<String> {
    let mut doc: toml_edit::DocumentMut = cargo.parse()?;
    let root: toml_edit::DocumentMut = root.parse()?;
    let declared = root.get("workspace");
    let ours = doc
        .remove("workspace")
        .context("the crate's manifest has no workspace")?;
    doc.remove("profile");
    if declared.and_then(|w| w.get("lints")).is_none() {
        doc.remove("lints");
    }
    for table in ["dependencies", "dev-dependencies"] {
        let Some(deps) = doc
            .get_mut(table)
            .and_then(toml_edit::Item::as_table_like_mut)
        else {
            continue;
        };
        for (dep, spec) in deps.iter_mut() {
            let from_workspace = spec
                .get("workspace")
                .and_then(toml_edit::Item::as_bool)
                .unwrap_or(false);
            let in_root = declared
                .and_then(|w| w.get("dependencies"))
                .and_then(|d| d.get(dep.get()))
                .is_some();
            if !from_workspace || in_root {
                continue;
            }
            let shared = ours
                .get("dependencies")
                .and_then(|d| d.get(dep.get()))
                .with_context(|| format!("the workspace declares no {}", dep.get()))?;
            let mut inline = match shared.as_str() {
                Some(version) => {
                    let mut inline = toml_edit::InlineTable::new();
                    inline.insert("version", version.into());
                    inline
                }
                None => shared
                    .as_inline_table()
                    .cloned()
                    .with_context(|| format!("{} is neither a version nor a table", dep.get()))?,
            };
            inline.decor_mut().clear();
            if let Some(rest) = spec.as_table_like() {
                for (key, value) in rest.iter().filter(|(key, _)| *key != "workspace") {
                    if let Some(value) = value.as_value() {
                        inline.insert(key, value.clone());
                    }
                }
            }
            *spec = if inline.len() == 1
                && let Some(version) = inline.get("version")
            {
                toml_edit::value(version.clone())
            } else {
                toml_edit::value(inline)
            };
        }
    }
    Ok(doc.to_string())
}

/// A full commit hash of the platform repository, to build a mod on its
/// `main` branch from git instead of the crates.io release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Commit(String);

impl std::str::FromStr for Commit {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        if text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self(text.to_owned()))
        } else {
            bail!("{text:?} is not a full 40-character commit hash in lowercase")
        }
    }
}

/// Where the platform's git repository is.
const PLATFORM_GIT: &str = "https://github.com/enshwick/enshwick";

/// `street-lanterns` as `Street Lanterns`, the name players read until the
/// author picks one.
fn display_name(id: &str) -> String {
    id.split('-')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
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

// The crate's files, with `{id}`, `{crate}`, `{type}`, `{name}`, `{side}`,
// `{description}` and `{enshwick}` to fill.
const CARGO_TOML: &str = include_str!("new/crate/Cargo.toml.in");
const LIB_RS: &str = include_str!("new/crate/lib.rs.in");
const TEXTS_TOML: &str = include_str!("new/crate/texts.toml");
const RUST_TOOLCHAIN: &str = include_str!("new/crate/rust-toolchain.toml");
const CARGO_CONFIG: &str = include_str!("new/crate/config.toml");

const README: &str = "# {id}

An Enshrouded mod built on [Enshwick](https://github.com/enshwick/enshwick).

`cargo enshwick install --game <game folder>` builds it and copies it into the
game's `mods/` folder. Its settings are `mod-configs/{id}.toml`, written on first
start; its log is `logs/enshwick/mods/{id}.log`.
";

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    /// The text of the file at `name` among `files`.
    fn text<'a>(files: &'a [(PathBuf, String)], name: &str) -> &'a str {
        &files
            .iter()
            .find(|(path, _)| path == Path::new(name))
            .unwrap_or_else(|| panic!("no {name}"))
            .1
    }

    #[test]
    fn ids_become_type_names() {
        assert_eq!(type_name("street-lanterns"), "StreetLanterns");
        assert_eq!(type_name("lockers"), "Lockers");
        assert_eq!(type_name("2x-loot"), "Mod2xLoot");
    }

    #[test]
    fn an_id_that_cannot_name_a_crate_is_refused() {
        let alone = |id| files(id, Side::Server, None, &Place::Alone);
        assert!(alone("you.2x-loot").is_err());
        assert!(alone("you.-loot").is_err());
        assert!(alone("you.loot-2x").is_ok());
        assert!(alone("lanterns").is_err());
        assert!(alone("Area Loot").is_err());
    }

    #[test]
    fn the_files_name_the_mod_everywhere() {
        let files = files("you.street-lanterns", Side::Both, None, &Place::Alone).unwrap();
        let cargo = text(&files, "Cargo.toml");
        assert!(cargo.contains("name = \"street-lanterns\""));
        assert!(cargo.contains(&format!(
            "[workspace.dependencies]\nenshwick = \"={}\"\n",
            enshwick::VERSION
        )));
        let lib = text(&files, "src/lib.rs");
        assert!(lib.contains("id: \"you.street-lanterns\",\n            side: BOTH,"));
        assert!(lib.contains("name: \"Street Lanterns\","));
        assert!(lib.contains("declare_mod!(StreetLanterns)"));
        assert!(text(&files, "src/texts.toml").contains("\"mod.name\" = \"Street Lanterns\""));
        for (path, text) in &files {
            for hole in [
                "{type}",
                "{id}",
                "{crate}",
                "{name}",
                "{side}",
                "{description}",
                "{enshwick}",
            ] {
                assert!(!text.contains(hole), "{} keeps {hole}", path.display());
            }
        }
    }

    #[test]
    fn a_crate_of_its_own_carries_the_toolchain_the_flags_and_the_profile() {
        for place in [Place::Alone, Place::Excluded] {
            let files = files("you.lockers", Side::Server, None, &place).unwrap();
            assert!(text(&files, "rust-toolchain.toml").contains("x86_64-pc-windows-msvc"));
            assert!(text(&files, ".cargo/config.toml").contains("+crt-static"));
            let cargo: toml::Table = text(&files, "Cargo.toml").parse().unwrap();
            assert_eq!(cargo["profile"]["release"]["lto"].as_str(), Some("fat"));
            assert!(cargo["workspace"]["lints"]["clippy"].is_table());
            assert_eq!(cargo["lints"]["workspace"].as_bool(), Some(true));
        }
    }

    #[test]
    fn a_commit_builds_on_the_platform_from_git() {
        let commit: Commit = COMMIT.parse().unwrap();
        let files = files("you.lockers", Side::Server, Some(&commit), &Place::Alone).unwrap();
        let cargo: toml::Table = text(&files, "Cargo.toml").parse().unwrap();
        let enshwick = &cargo["workspace"]["dependencies"]["enshwick"];
        assert_eq!(
            enshwick["git"].as_str(),
            Some("https://github.com/enshwick/enshwick")
        );
        assert_eq!(enshwick["rev"].as_str(), Some(COMMIT));
        assert!(enshwick.get("version").is_none());
    }

    #[test]
    fn only_a_full_lowercase_commit_hash_is_taken() {
        assert!(COMMIT.parse::<Commit>().is_ok());
        assert!("0123456".parse::<Commit>().is_err());
        assert!(COMMIT.to_uppercase().parse::<Commit>().is_err());
        assert!(format!("{COMMIT}0").parse::<Commit>().is_err());
        assert!("main".parse::<Commit>().is_err());
    }

    #[test]
    fn ids_become_names() {
        assert_eq!(display_name("street-lanterns"), "Street Lanterns");
        assert_eq!(display_name("loot-2x"), "Loot 2x");
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
        create(&dir.join("mods"), "you.street-lanterns", Side::Server, None).unwrap();
        let root = std::fs::read_to_string(&manifest).unwrap();
        assert!(root.contains("# kept"), "{root}");
        assert!(
            root.contains("members = [\"tools/x\", \"mods/street-lanterns\"]"),
            "{root}"
        );
        let member = dir.join("mods/street-lanterns");
        let cargo: toml::Table = std::fs::read_to_string(member.join("Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
        for table in ["profile", "workspace", "lints"] {
            assert!(cargo.get(table).is_none(), "the member has [{table}]");
        }
        let pin = format!("={}", enshwick::VERSION);
        assert_eq!(cargo["dependencies"]["enshwick"].as_str(), Some(&*pin));
        assert_eq!(
            cargo["dev-dependencies"]["enshwick"]["version"].as_str(),
            Some(&*pin)
        );
        assert!(member.join("src/texts.toml").is_file());
        assert!(!member.join("rust-toolchain.toml").exists());
        assert!(!member.join(".cargo").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_member_uses_what_the_workspace_declares() {
        let dir = scratch("declared");
        let root = "[workspace]\nmembers = [\"mods/*\"]\n\n[workspace.dependencies]\nenshwick = { path = \"crates/enshwick\" }\n\n[workspace.lints.rust]\nmissing_docs = \"deny\"\n";
        std::fs::write(dir.join("Cargo.toml"), root).unwrap();
        create(&dir.join("mods"), "you.lockers", Side::Server, None).unwrap();
        let cargo: toml::Table = std::fs::read_to_string(dir.join("mods/lockers/Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(cargo["lints"]["workspace"].as_bool(), Some(true));
        assert_eq!(
            cargo["dependencies"]["enshwick"]["workspace"].as_bool(),
            Some(true)
        );
        assert!(cargo["dependencies"]["serde"]["version"].is_str());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_member_pattern_or_an_exclude_leaves_the_workspace_as_it_is() {
        let dir = scratch("covered");
        let manifest = dir.join("Cargo.toml");
        let text = "[workspace]\nmembers = [\"mods/*\"]\nexclude = [\"scratch\"]\n";
        std::fs::write(&manifest, text).unwrap();
        create(&dir.join("mods"), "you.lockers", Side::Server, None).unwrap();
        create(&dir.join("scratch"), "you.my-mod", Side::Server, None).unwrap();
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), text);
        let created: toml::Table = std::fs::read_to_string(dir.join("scratch/my-mod/Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(created["profile"]["release"].is_table() && created["workspace"].is_table());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_exclude_written_with_slashes_still_excludes() {
        let dir = scratch("slashes");
        let manifest = dir.join("Cargo.toml");
        let text = "[workspace]\nmembers = []\nexclude = [\"./scratch/\"]\n";
        std::fs::write(&manifest, text).unwrap();
        create(&dir.join("scratch"), "you.my-mod", Side::Server, None).unwrap();
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), text);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bad_id_is_refused_before_the_folder_is_looked_at() {
        let dir = scratch("bad-id");
        std::fs::create_dir_all(dir.join("Area Loot")).unwrap();
        let refused = create(&dir, "Area Loot", Side::Server, None)
            .unwrap_err()
            .to_string();
        assert!(refused.contains("is not <author>.<name>"), "{refused}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The target folder this test binary runs from, which the checked crate
    /// shares so the API crate compiles once across runs.
    fn target_dir() -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .find(|dir| dir.file_name().is_some_and(|name| name == "target"))
            .map_or_else(
                || std::env::temp_dir().join("enshwick-new-target"),
                Path::to_path_buf,
            )
            .join("new-mod")
    }

    #[test]
    fn a_new_mod_compiles_against_this_api() {
        let dir = scratch("compiles");
        create(&dir, "you.street-lanterns", Side::Both, None).unwrap();
        let root = dir.join("street-lanterns");
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let api = workspace.join("crates/enshwick");
        let manifest = root.join("Cargo.toml");
        let pinned = format!("enshwick = \"={}\"", enshwick::VERSION);
        let cargo = std::fs::read_to_string(&manifest).unwrap();
        assert!(cargo.contains(&pinned), "{cargo}");
        let local = format!("enshwick = {{ path = {:?} }}", api.display().to_string());
        std::fs::write(&manifest, cargo.replace(&pinned, &local)).unwrap();
        // The workspace's lock holds every version the crate needs, so the
        // check runs offline.
        std::fs::copy(workspace.join("Cargo.lock"), root.join("Cargo.lock")).unwrap();
        let output = std::process::Command::new(env!("CARGO"))
            .args(["check", "--offline", "--quiet", "--manifest-path"])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(target_dir())
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_TARGET_DIR")
            .output();
        let Ok(output) = output else {
            eprintln!("skipped: cargo does not run in this process");
            std::fs::remove_dir_all(&dir).unwrap();
            return;
        };
        std::fs::remove_dir_all(&dir).unwrap();
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{errors}");
        assert!(!errors.contains("warning"), "{errors}");
    }
}
