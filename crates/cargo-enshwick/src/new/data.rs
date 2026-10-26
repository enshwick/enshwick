//! `cargo enshwick new --data`: a data pack folder that the loader runs as
//! it is, with every file commented for the first change.

use anyhow::{Context as _, bail};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use enshwick::i18n::Language;
use enshwick::packs::{
    ITEMS_FILE, LOOT_FILE, MANIFEST_FILE, RECIPES_FILE, TEXTS_FILE, WORKSHOPS_FILE,
};

use crate::Side;

/// Creates the data pack `id`, `<author>.<name>`, in `<dir>/<id>`, the
/// folder name the loader wants; what was made, for the user.
pub(crate) fn create(dir: &Path, id: &str, side: Side) -> anyhow::Result<String> {
    if !enshwick::abi::is_valid_id(id) {
        bail!("{id:?} is not <author>.<name>, each part of [a-z0-9-]");
    }
    let root = std::path::absolute(dir.join(id)).with_context(|| dir.display().to_string())?;
    if root.exists() {
        bail!("{} already exists", root.display());
    }
    for (path, text) in files(id, side) {
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
    Ok(format!(
        "created data pack `{id}` in {}; `cargo enshwick pack check {} --game <game folder>` checks it, and copying the folder into the game's mods/ installs it",
        root.display(),
        root.display()
    ))
}

/// The pack's files: its manifest, one commented file per kind of data, the
/// texts with every game language, the editors' schemas and a README.
fn files(id: &str, side: Side) -> Vec<(PathBuf, String)> {
    let name = super::display_name(id.split_once('.').map_or(id, |(_, name)| name));
    let side = match side {
        Side::Server => "server",
        Side::Client => "client",
        Side::Both => "both",
    };
    let fill = |template: &str| {
        template
            .replace("{id}", id)
            .replace("{name}", &name)
            .replace("{side}", side)
    };
    let mut files = vec![
        (MANIFEST_FILE.into(), fill(PACK_TOML)),
        (ITEMS_FILE.into(), fill(ITEMS_TOML)),
        (RECIPES_FILE.into(), fill(RECIPES_TOML)),
        (WORKSHOPS_FILE.into(), fill(WORKSHOPS_TOML)),
        (LOOT_FILE.into(), fill(LOOT_TOML)),
        (TEXTS_FILE.into(), texts()),
        ("README.md".into(), fill(README)),
    ];
    for (file, schema) in enshwick::packs::schemas() {
        files.push((file.replace(".toml", ".schema.json").into(), schema));
    }
    files
}

/// The texts file: the fallback language's table, and every other game
/// language commented, ready for a translation.
fn texts() -> String {
    let mut text = String::from(
        "# The pack's texts per language; the first is the fallback for the others.\n",
    );
    for (index, language) in Language::ALL.iter().enumerate() {
        if index == 0 {
            let _ = writeln!(
                text,
                "[{}]\n# \"lantern.name\" = \"Lantern\"",
                language.code()
            );
        } else {
            let _ = writeln!(text, "\n# [{}]", language.code());
        }
    }
    text
}

const PACK_TOML: &str = r#"id = "{id}"
version = "0.1.0"
name = "{name}"
description = "What {name} changes, in one sentence."
authors = ["Your Name"]
license = "MIT"
side = "{side}"
"#;

const ITEMS_TOML: &str = r#"#:schema ./items.schema.json
# Items of this pack, and changes of items the game or other mods have.
# A number sets a value; { times = x } multiplies it.

# [[change]]
# items = ["Material_T1_Wood_Logs", "Material_T1_Resin"]
# max_stack = { times = 4 }
"#;

const RECIPES_TOML: &str = r#"#:schema ./recipes.schema.json
# Recipes of this pack, and changes of the game's recipes, picked by their
# debug name (recipes) or by what they make (output).

# [[change]]
# output = ["Material_T2_Wood_Planks"]
# time = { times = 0.5 }
"#;

const WORKSHOPS_TOML: &str = r#"#:schema ./workshops.schema.json
# Crafting stations of this pack, each a copy of a vanilla one.

# [[workshop]]
# id = "my-bench"
# like = "workbench"
"#;

const LOOT_TOML: &str = r#"#:schema ./loot.schema.json
# Drops this pack adds to the game's loot.

# [[drop]]
# items = ["Tool_Torch01"]
# chance = 0.05
# from = "chests"
"#;

const README: &str = "# {name}

A data pack for Enshrouded, run by the [Enshwick](https://github.com/enshwick/enshwick) loader.

1. Edit the `.toml` files: each one shows its first change, commented out.
2. Run `cargo enshwick pack check . --game <game folder>` to check every file
   against the game's data, with the line of each problem.
3. Copy this folder into the game's `mods/` folder as `mods/{id}/`, on the
   server and on every player's game.
4. Run `cargo enshwick pack zip .` for the archive players install.
";

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-new-data-{name}-{}", std::process::id()));
        let _old = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_new_data_pack_checks_clean_as_it_is() {
        let dir = scratch("clean");
        create(&dir, "you.bigger-stacks", Side::Both).unwrap();
        let (summary, manifest, _) =
            crate::pack::check(&dir.join("you.bigger-stacks"), None).unwrap();
        assert_eq!(manifest.id, "you.bigger-stacks");
        assert_eq!(manifest.name, "Bigger stacks");
        assert!(summary.contains("0 changes"), "{summary}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_pack_file_names_its_schema_and_the_schema_is_there() {
        let files = files("you.bigger-stacks", Side::Server);
        for (path, text) in &files {
            let Some(name) = path
                .to_str()
                .filter(|_| path.extension().is_some_and(|e| e == "toml"))
            else {
                continue;
            };
            if name == MANIFEST_FILE || name == TEXTS_FILE {
                continue;
            }
            let schema = name.replace(".toml", ".schema.json");
            assert!(text.starts_with(&format!("#:schema ./{schema}")), "{name}");
            assert!(
                files.iter().any(|(p, _)| p == Path::new(&schema)),
                "{schema}"
            );
        }
        let manifest = &files[0].1;
        assert!(manifest.contains("side = \"server\""));
    }

    #[test]
    fn the_texts_name_every_game_language() {
        let text = texts();
        for language in Language::ALL {
            assert!(
                text.contains(&format!("[{}]", language.code())),
                "{}",
                language.code()
            );
        }
    }

    #[test]
    fn an_existing_folder_or_a_bad_id_is_refused() {
        let dir = scratch("refused");
        create(&dir, "you.stacks", Side::Both).unwrap();
        assert!(create(&dir, "you.stacks", Side::Both).is_err());
        assert!(create(&dir, "Stacks", Side::Both).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
