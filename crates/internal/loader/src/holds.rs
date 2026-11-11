//! Holds on the game: a mod that keeps a world the vanilla game cannot open
//! safely sets one (`enshwick_core::state::Hold`), and the loader stops the
//! game before the world loads when that mod would not run.
//!
//! A hold in a mod's folder, `mods/<mod>/hold.txt`, counts as the loader
//! starts; a hold in a world's data folder, `<stem>-mod-data/<mod>/hold.txt`,
//! counts as that world's key appears, before its save is read. A build the
//! loader leaves vanilla learns no world, so only the first kind holds it;
//! the loader's own hold on kept stack sizes is of that kind.

use std::path::Path;

use enshwick_core::i18n::Texts;
use enshwick_core::state::{HOLD_FILE, Hold};

/// How long the log gets to write the reasons before the game stops.
#[cfg(windows)]
const LOG_GRACE: std::time::Duration = std::time::Duration::from_secs(1);
/// The exit code of a game stopped by a hold.
#[cfg(windows)]
const EXIT_HELD: u32 = 3;

/// One hold found in a mod's folder or a world's data folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    /// The folder it is in: the mod's folder in `mods/`, or the mod's id in a
    /// world's data folder.
    pub id: String,
    /// Its reason in the line the mod wrote for readers without its texts.
    pub reason: String,
    /// Its reason as a text of the mod's texts, when the mod gave one.
    pub text: Option<String>,
    /// The key of the world it holds; `None` for a hold of every world.
    pub world: Option<String>,
    /// Its file.
    pub file: String,
}

impl Held {
    /// The reason as the log shows it: in the reader's language from the
    /// mod's `texts` while they are loaded, else the mod's line, with the
    /// world for a hold of one world.
    #[must_use]
    pub fn shown(&self, texts: Option<&Texts>) -> String {
        let reason = texts
            .zip(self.text.as_deref())
            .and_then(|(texts, text)| texts.reason(text, crate::texts::language()))
            .unwrap_or_else(|| self.reason.clone());
        match &self.world {
            Some(world) => format!("{reason}, in world {world}"),
            None => reason,
        }
    }
}

/// The holds in the mods' folders under `mods_dir`, which count as the
/// loader starts.
#[must_use]
pub fn at_start(mods_dir: &Path) -> Vec<Held> {
    found(mods_dir, None)
}

/// The holds on the world `key` in its data folder.
#[must_use]
pub fn of_world(key: &str) -> Vec<Held> {
    found(&crate::world_folders::data_of(key), Some(key))
}

/// The holds in the folders directly in `dir`, of the world `world`.
fn found(dir: &Path, world: Option<&str>) -> Vec<Held> {
    enshwick_core::state::holds(dir)
        .into_iter()
        .map(|(id, reason)| Held {
            file: dir.join(&id).join(HOLD_FILE).display().to_string(),
            text: Hold::new(&dir.join(&id)).text(),
            world: world.map(|key| {
                Path::new(key).file_name().map_or_else(
                    || key.to_owned(),
                    |stem| stem.to_string_lossy().into_owned(),
                )
            }),
            id,
            reason,
        })
        .collect()
}

/// The holds that stop the game: every one when the build is unknown, else
/// those of mods that do not run, named by their ids or folders.
#[must_use]
pub fn unmet(holds: Vec<Held>, running: &[&str], unknown_build: bool) -> Vec<Held> {
    holds
        .into_iter()
        .filter(|held| unknown_build || !running.contains(&held.id.as_str()))
        .collect()
}

/// Stops the game when a hold in a mod's folder under `mods_dir` is unmet;
/// see [`at_start`] and [`unmet`]. The reasons read in the reader's
/// language from the texts of the mods `loader` loaded, when there is one.
#[cfg(windows)]
pub(crate) fn enforce(
    mods_dir: &Path,
    running: &[&str],
    unknown_build: bool,
    loader: Option<&crate::host::Loader>,
) {
    let why = if unknown_build {
        "the game build is unknown, so it would run vanilla"
    } else {
        "the mod does not run"
    };
    stop(
        &unmet(at_start(mods_dir), running, unknown_build),
        why,
        loader,
    );
}

/// The ids and folders of the mods `loader` runs, and its own id.
#[cfg(windows)]
pub(crate) fn running(loader: &crate::host::Loader) -> Vec<&str> {
    loader
        .mods
        .iter()
        .filter(|m| !m.off.load(std::sync::atomic::Ordering::Acquire))
        .flat_map(|m| [m.id.as_str(), m.folder()])
        .chain([crate::settings::LOADER_ID])
        .collect()
}

/// Stops the game when a hold on the world `key` is unmet, as its key
/// appears, before its save is read.
#[cfg(windows)]
pub(crate) fn enforce_world(loader: &crate::host::Loader, key: &str) {
    stop(
        &unmet(of_world(key), &running(loader), false),
        "the mod does not run",
        Some(loader),
    );
}

/// Logs each hold of `held` with `why` it is unmet, its reason from the
/// texts of its mod if `loader` loaded them, and stops the game; does
/// nothing when `held` is empty.
#[cfg(windows)]
fn stop(held: &[Held], why: &str, loader: Option<&crate::host::Loader>) {
    use crate::locks::Relock;
    if held.is_empty() {
        return;
    }
    for hold in held {
        let record = loader.and_then(|loader| {
            loader
                .mods
                .iter()
                .find(|m| m.id == hold.id || m.folder() == hold.id)
        });
        let reason = match record {
            Some(record) => hold.shown(record.texts.relock().as_ref()),
            None => hold.shown(None),
        };
        let Held { id, file, .. } = hold;
        crate::log!(
            ERROR,
            "{id} holds this game ({reason}) and {why}: the game stops before the world loads; \
             update Enshwick and the mod, or delete {file} to open the world anyway, \
             at your own risk ({})",
            crate::errors::HELD
        );
    }
    std::thread::sleep(LOG_GRACE);
    // SAFETY: no preconditions; the pseudo handle of this process.
    let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
    // SAFETY: a valid process handle; nothing of the game's world is open yet.
    unsafe { windows_sys::Win32::System::Threading::TerminateProcess(me, EXIT_HELD) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(id: &str, reason: &str) -> Held {
        Held {
            id: id.to_owned(),
            reason: reason.to_owned(),
            text: None,
            world: None,
            file: format!("{id}/{HOLD_FILE}"),
        }
    }

    fn holds() -> Vec<Held> {
        vec![held("you.lanterns", "20 zones"), held("other", "why")]
    }

    #[test]
    fn a_running_mod_meets_its_hold() {
        let left = unmet(holds(), &["you.lanterns"], false);
        assert_eq!(left, vec![held("other", "why")]);
        assert!(unmet(holds(), &["you.lanterns", "other"], false).is_empty());
    }

    #[test]
    fn an_unknown_build_meets_no_hold() {
        assert_eq!(unmet(holds(), &["you.lanterns", "other"], true).len(), 2);
        assert!(unmet(Vec::new(), &[], true).is_empty());
        // A running mod counts under every id it had: the start lists them.
        let renamed = vec![held("lanterns", "a world")];
        assert!(unmet(renamed, &["you.lanterns", "lanterns"], false).is_empty());
    }

    #[test]
    fn a_hold_in_a_mod_folder_counts_at_start_and_a_world_hold_with_its_world() {
        use enshwick_core::state::Hold;
        let root = std::env::temp_dir().join(format!("enshwick-holds-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let mods = root.join("mods");
        let key = root.join("world0").to_string_lossy().into_owned();
        Hold::new(&mods.join("stacks"))
            .set("raised stacks")
            .unwrap();
        Hold::new(&crate::world_folders::data_of(&key).join("you.altars"))
            .set("17 altars")
            .unwrap();
        let start = at_start(&mods);
        assert_eq!(start.len(), 1);
        assert!(start[0].file.ends_with("hold.txt"), "{}", start[0].file);
        assert!(
            unmet(start, &["you.stacks", "stacks"], false).is_empty(),
            "met by its folder"
        );
        let world = of_world(&key);
        assert_eq!(world[0].id, "you.altars");
        assert_eq!(world[0].shown(None), "17 altars, in world world0");
        assert!(of_world(&root.join("world1").to_string_lossy()).is_empty());
        assert!(unmet(world, &["you.altars"], false).is_empty());
        let _gone = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_text_hold_reads_in_the_reader_s_language_and_in_its_line_without_texts() {
        use enshwick_core::state::Hold;
        let data = std::env::temp_dir().join(format!("enshwick-text-holds-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let key = data.join("world0").to_string_lossy().into_owned();
        Hold::new(&crate::world_folders::data_of(&key).join("you.altars"))
            .set_text("17 altars", "hold.altars", &[("altars", "17")])
            .unwrap();
        let texts = Texts::parse(
            "[en]\n\"hold.altars\" = \"{altars} altars\"\n[uk]\n\"hold.altars\" = \"{altars} вівтарів\"\n",
        )
        .unwrap();
        let world = of_world(&key);
        assert_eq!(world[0].reason, "17 altars");
        assert_eq!(world[0].shown(None), "17 altars, in world world0");
        let uk = Some(enshwick_core::i18n::Language::UK_UA);
        assert_eq!(
            crate::texts::for_reader(uk, || world[0].shown(Some(&texts))),
            "17 вівтарів, in world world0"
        );
        let other = Texts::parse("[en]\nelse = \"Else\"\n").unwrap();
        assert_eq!(
            crate::texts::for_reader(uk, || world[0].shown(Some(&other))),
            "17 altars, in world world0"
        );
        let _gone = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn an_old_hold_file_of_one_plain_line_still_holds() {
        let data = std::env::temp_dir().join(format!("enshwick-old-holds-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let dir = data.join("you.stacks");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(HOLD_FILE), "raised stacks").unwrap();
        let held = at_start(&data);
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].text, None);
        assert_eq!(held[0].shown(None), "raised stacks");
        assert_eq!(unmet(held, &[], false).len(), 1);
        let _gone = std::fs::remove_dir_all(&data);
    }
}
