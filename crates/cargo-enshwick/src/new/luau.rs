//! `cargo enshwick new --luau`: a Luau mod folder the loader runs as it is,
//! with the API's definitions for the editor.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use cargo_enshwick::luau::{
    DEFINITIONS_FILE, TYPED_DEFINITIONS_FILE, definitions, typed_definitions,
};
use enshwick::packs::{MANIFEST_FILE, SCRIPT_MAIN as MAIN_FILE};

use crate::Side;

/// Where a new mod keeps the API's definitions.
pub(crate) const DEFINITIONS_DIR: &str = "types";

/// The file of the game's own types beside the API's definitions.
pub(crate) const GAME_DEFINITIONS_FILE: &str = "game.d.luau";

/// Creates the Luau mod `id`, `<author>.<name>`, in `<dir>/<id>`, the folder
/// name the loader wants, with the game's own types read from `game`, the
/// game executable or its folder, when given; what was made, for the user.
pub(crate) fn create(
    dir: &Path,
    id: &str,
    side: Side,
    game: Option<&Path>,
) -> anyhow::Result<String> {
    if !enshwick::abi::is_valid_id(id) {
        bail!("{id:?} is not <author>.<name>, each part of [a-z0-9-]");
    }
    let root = std::path::absolute(dir.join(id)).with_context(|| dir.display().to_string())?;
    if root.exists() {
        bail!("{} already exists", root.display());
    }
    let game = game.map(crate::types::luau_definitions).transpose()?;
    for (path, text) in files(id, side, game) {
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
        "created Luau mod `{id}` in {}; copying the folder into the game's mods/ installs it",
        root.display()
    ))
}

/// The mod's files: its manifest, `main.luau`, the definitions, the game's
/// own types `game` when given, the editor settings that point luau-lsp at
/// them, and a README.
pub(crate) fn files(id: &str, side: Side, game: Option<String>) -> Vec<(PathBuf, String)> {
    let name = super::display_name(id.split_once('.').map_or(id, |(_, name)| name));
    let side = match side {
        Side::Server => "server",
        Side::Both => "both",
    };
    let (game_setting, game_analyze, game_step) = if game.is_some() {
        (
            format!(",\n\t\t\"@game\": \"{}\"", game_definitions_path()),
            format!(" --definitions:@game={}", game_definitions_path()),
            GAME_STEP_WRITTEN,
        )
    } else {
        (String::new(), String::new(), GAME_STEP_MISSING)
    };
    let fill = |template: &str| {
        template
            .replace("{game_step}", game_step)
            .replace("{game_setting}", &game_setting)
            .replace("{game_analyze}", &game_analyze)
            .replace("{game_definitions}", &game_definitions_path())
            .replace("{id}", id)
            .replace("{name}", &name)
            .replace("{side}", side)
            .replace("{definitions}", &definitions_path())
            .replace("{typed_definitions}", &typed_definitions_path())
    };
    let mut files = vec![
        (MANIFEST_FILE.into(), fill(PACK_TOML)),
        (MAIN_FILE.into(), fill(MAIN_LUAU)),
        (".luaurc".into(), LUAURC.to_owned()),
        (".vscode/settings.json".into(), fill(VSCODE_SETTINGS)),
        (definitions_path().into(), definitions()),
        (typed_definitions_path().into(), typed_definitions()),
        ("README.md".into(), fill(README)),
    ];
    if let Some(game) = game {
        files.push((game_definitions_path().into(), game));
    }
    files
}

/// Where the game's own types sit in a new mod, with `/` between folders.
pub(crate) fn game_definitions_path() -> String {
    format!("{DEFINITIONS_DIR}/{GAME_DEFINITIONS_FILE}")
}

/// Where the definitions sit in a new mod, with `/` between folders.
pub(crate) fn definitions_path() -> String {
    format!("{DEFINITIONS_DIR}/{DEFINITIONS_FILE}")
}

/// Where the typed API's types sit in a new mod, with `/` between folders.
pub(crate) fn typed_definitions_path() -> String {
    format!("{DEFINITIONS_DIR}/{TYPED_DEFINITIONS_FILE}")
}

const PACK_TOML: &str = r#"id = "{id}"
version = "0.1.0"
name = "{name}"
description = "What {name} does, in one sentence."
authors = ["Your Name"]
license = "MIT"
side = "{side}"

# Settings players change on the Mods screen; `enshwick.settings()` reads them.
# [[setting]]
# key = "greeting"
# default = "Welcome"
# doc = "What a player who joins reads."
"#;

const MAIN_LUAU: &str = r#"--!strict
-- {name}: the loader runs this file when the world's server starts.
-- An item of an items.toml beside it with on = { consume = "drink" } runs
-- the global function drink(used) of this file when a player uses it.

enshwick.on("PlayerJoined", function(player)
	local joined = enshwick.world.player(player.entity)
	if joined then
		enshwick.world.tell(joined, `Welcome, {joined.name}`)
	end
end)

-- Content is added while the mod starts, here a copy of the vanilla storage box:
-- enshwick.content.add_item({
-- 	id = "{id}:box",
-- 	like = enshwick.content.item("Prop_Storage_24_T1_Box"),
-- })
"#;

const LUAURC: &str = r#"{
	"languageMode": "strict"
}
"#;

const VSCODE_SETTINGS: &str = r#"{
	"luau-lsp.platform.type": "standard",
	"luau-lsp.types.definitionFiles": {
		"@enshwick": "{definitions}",
		"@typed": "{typed_definitions}"{game_setting}
	}
}
"#;

const README: &str = "# {name}

A Luau mod for Enshrouded, built on [Enshwick](https://github.com/enshwick/enshwick).

1. Edit `main.luau`. `{definitions}` declares the API for
   [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp) and
   `{typed_definitions}` the values of `enshwick.typed`;
   `.vscode/settings.json` points the editor at both.
2. {game_step}
3. Run `luau-lsp analyze --definitions:@enshwick={definitions} --definitions:@typed={typed_definitions}{game_analyze} main.luau`
   to check the types without the game.
4. Copy this folder into the game's `mods/` folder as `mods/{id}/`.
5. Run `cargo enshwick pack zip .` for the archive players install.
";

const GAME_STEP_WRITTEN: &str =
    "`{game_definitions}` declares the game's own types, such as the fields
   `enshwick.on_game_type` hands a handler. Run
   `cargo enshwick types --from <game folder> --luau {game_definitions}` again
   after a game update.";

const GAME_STEP_MISSING: &str =
    "Run `cargo enshwick types --from <game folder> --luau {game_definitions}`
   for the game's own types, such as the fields `enshwick.on_game_type` hands a
   handler, and add `\"@game\": \"{game_definitions}\"` beside `@enshwick` in
   `.vscode/settings.json`.";

#[cfg(test)]
mod tests {
    use super::*;

    use crate::scratch;

    #[test]
    fn a_new_luau_mod_reads_as_a_pack_with_its_main_script() {
        let dir = scratch("clean");
        create(&dir, "you.greeter", Side::Server, None).unwrap();
        let root = dir.join("you.greeter");
        let (_, manifest, files) = crate::pack::check(&root, None).unwrap();
        assert_eq!(manifest.id, "you.greeter");
        assert_eq!(manifest.side, enshwick::packs::PackSide::Server);
        let scripts: Vec<&str> = files.scripts().map(|(name, _)| name).collect();
        assert_eq!(scripts, [MAIN_FILE]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_mod_on_both_sides_says_so_and_no_join_flag() {
        let files = files("you.greeter", Side::Both, None);
        assert!(files[0].1.contains("side = \"both\""));
        assert!(!files[0].1.contains("join ="));
    }

    #[test]
    fn the_editor_finds_the_definitions_the_mod_carries() {
        let files = files("you.greeter", Side::Server, None);
        let settings = &files
            .iter()
            .find(|(p, _)| p.ends_with("settings.json"))
            .unwrap()
            .1;
        assert!(settings.contains(&format!("\"@enshwick\": \"{}\"", definitions_path())));
        assert!(settings.contains(&format!("\"@typed\": \"{}\"", typed_definitions_path())));
        let carries = |path: String, wanted: String| {
            files
                .iter()
                .any(|(p, text)| *p == Path::new(&path) && *text == wanted)
        };
        assert!(carries(definitions_path(), definitions()));
        assert!(carries(typed_definitions_path(), typed_definitions()));
    }

    #[test]
    fn the_game_s_types_sit_beside_the_api_s_when_given() {
        let game = "export type Health = { current: number }\n";
        let files = files("you.greeter", Side::Server, Some(game.to_owned()));
        let text = |name: &str| &files.iter().find(|(p, _)| p.ends_with(name)).unwrap().1;
        assert_eq!(text(GAME_DEFINITIONS_FILE), game);
        assert!(
            text("settings.json").contains(&format!("\"@game\": \"{}\"", game_definitions_path()))
        );
        assert!(
            text("README.md").contains(&format!("--definitions:@game={}", game_definitions_path()))
        );
        let settings: serde_json::Value = serde_json::from_str(text("settings.json")).unwrap();
        assert!(settings.is_object());
    }

    #[test]
    fn without_the_game_the_readme_says_how_to_add_its_types() {
        let files = files("you.greeter", Side::Server, None);
        assert!(
            !files
                .iter()
                .any(|(p, _)| p.ends_with(GAME_DEFINITIONS_FILE))
        );
        let readme = &files
            .iter()
            .find(|(p, _)| p.ends_with("README.md"))
            .unwrap()
            .1;
        assert!(readme.contains(&format!("--luau {}", game_definitions_path())));
        assert!(!readme.contains("--definitions:@game"));
    }

    #[test]
    fn an_existing_folder_or_a_bad_id_is_refused() {
        let dir = scratch("refused");
        create(&dir, "you.greeter", Side::Server, None).unwrap();
        assert!(create(&dir, "you.greeter", Side::Server, None).is_err());
        assert!(create(&dir, "Greeter", Side::Server, None).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_example_luau_mod_carries_the_current_definitions() {
        let examples: Vec<PathBuf> = crate::pack::tests::example_packs()
            .into_iter()
            .filter(|dir| dir.join(MAIN_FILE).is_file())
            .collect();
        assert!(!examples.is_empty());
        for dir in examples {
            let at = dir.display();
            for (path, _) in files("you.example", Side::Server, None) {
                // The typed API's types are large and change with each game
                // build: an example leaves them to `cargo enshwick luau --typed`.
                if !path.starts_with(".vscode") && path != Path::new(&typed_definitions_path()) {
                    assert!(dir.join(&path).is_file(), "{at}: {}", path.display());
                }
            }
            let carried = std::fs::read_to_string(dir.join(definitions_path())).unwrap();
            assert!(
                carried == definitions(),
                "{at}: run `cargo enshwick luau --out {}`",
                definitions_path()
            );
        }
    }
}
