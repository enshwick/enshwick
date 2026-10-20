//! The move from the mod's old name: `dev-tools` became `admin-tools`, and
//! its settings file and data folder follow on the first start.

use std::path::Path;

/// The mod's id before it was renamed.
const OLD_ID: &str = "dev-tools";

/// Moves `mod-configs/dev-tools.toml` and `mod-data/dev-tools` beside the
/// game in `game` to the names of mod `id`, where nothing is in the way; one
/// line for the log per move.
pub(crate) fn migrate(game: &Path, id: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let configs = game.join("mod-configs");
    let old = configs.join(format!("{OLD_ID}.toml"));
    let new = configs.join(format!("{id}.toml"));
    if old.is_file() && !new.exists() {
        lines.push(rename(&old, &new));
    }
    let data = game.join("mod-data");
    let old = data.join(OLD_ID);
    let new = data.join(id);
    if old.is_dir() {
        // The loader makes the new folder when the mod asks for it; empty, it
        // is in nobody's way, and anything else stays.
        let _kept = std::fs::remove_dir(&new);
        if !new.exists() {
            lines.push(rename(&old, &new));
        }
    }
    lines
}

fn rename(old: &Path, new: &Path) -> String {
    match std::fs::rename(old, new) {
        Ok(()) => format!("moved {} to {}", old.display(), new.display()),
        Err(e) => format!("{} stays where it is: {e}", old.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str) -> std::path::PathBuf {
        let game = std::env::temp_dir().join(format!("admin-tools-{name}-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(game.join("mod-configs")).unwrap();
        std::fs::create_dir_all(game.join("mod-data")).unwrap();
        game
    }

    #[test]
    fn the_old_settings_and_data_move_to_the_new_id() {
        let game = game("move");
        std::fs::write(game.join("mod-configs/dev-tools.toml"), "enabled = true\n").unwrap();
        std::fs::create_dir_all(game.join("mod-data/dev-tools")).unwrap();
        std::fs::write(
            game.join("mod-data/dev-tools/commands.txt"),
            "/admin world\n",
        )
        .unwrap();
        std::fs::create_dir_all(game.join("mod-data/admin-tools")).unwrap();

        assert_eq!(migrate(&game, "admin-tools").len(), 2);
        assert_eq!(
            std::fs::read_to_string(game.join("mod-configs/admin-tools.toml")).unwrap(),
            "enabled = true\n"
        );
        assert!(game.join("mod-data/admin-tools/commands.txt").is_file());
        assert!(!game.join("mod-configs/dev-tools.toml").exists());
        assert!(!game.join("mod-data/dev-tools").exists());
        // Nothing left to move.
        assert!(migrate(&game, "admin-tools").is_empty());
        std::fs::remove_dir_all(&game).unwrap();
    }

    #[test]
    fn new_files_are_never_overwritten() {
        let game = game("keep");
        std::fs::write(game.join("mod-configs/dev-tools.toml"), "enabled = true\n").unwrap();
        std::fs::write(
            game.join("mod-configs/admin-tools.toml"),
            "enabled = false\n",
        )
        .unwrap();
        std::fs::create_dir_all(game.join("mod-data/dev-tools")).unwrap();
        std::fs::create_dir_all(game.join("mod-data/admin-tools")).unwrap();
        std::fs::write(game.join("mod-data/admin-tools/commands.txt"), "").unwrap();

        assert!(migrate(&game, "admin-tools").is_empty());
        assert_eq!(
            std::fs::read_to_string(game.join("mod-configs/admin-tools.toml")).unwrap(),
            "enabled = false\n"
        );
        assert!(game.join("mod-data/dev-tools").is_dir());
        std::fs::remove_dir_all(&game).unwrap();
    }
}
