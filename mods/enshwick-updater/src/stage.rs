//! Staging: a verified archive's DLLs and content pack files written to
//! `mods/.staged/`, which the `powrprof.dll` stub moves into place at the
//! next start.
//!
//! Only files Enshwick reads are taken, and only where it keeps them:
//! `powrprof.dll` beside the game, `*.dll` under `mods/`, and a content
//! pack's `.toml` and `.png` files in its folder `mods/<id>/`, never in the
//! loader's. A name that is absolute, goes up with `..`, uses `\`, a drive
//! or a stream, or points into the staging folder itself refuses the whole
//! archive.

use std::io::Read as _;
use std::path::{Path, PathBuf};

/// The staging folder, relative to the game's folder.
pub const STAGED: &str = "mods/.staged";
/// The most entries an archive may hold.
const MAX_ENTRIES: usize = 64;
/// The most bytes one file may unpack to.
const MAX_FILE: u64 = 64 << 20;
/// How deep under `mods/` a DLL may sit.
const MAX_DEPTH: usize = 4;

/// Whether `name` is a path inside the game folder that an update may write:
/// `powrprof.dll`, a `.dll` under `mods/` outside the staging folder, or a
/// pack's `.toml` or `.png` in a folder under `mods/` other than the
/// loader's, with `/` separators and plain names only.
///
/// # Examples
///
/// ```
/// use enshwick_updater::stage::allowed;
///
/// assert!(allowed("mods/enshwick-safes.dll"));
/// assert!(allowed("powrprof.dll"));
/// assert!(allowed("mods/lanterns/items.toml"));
/// assert!(allowed("mods/lanterns/icons/lantern.png"));
/// assert!(!allowed("mods/enshwick/mods.toml"));
/// assert!(!allowed("mods/lanterns.toml"));
/// assert!(!allowed("mods/../powrprof.dll"));
/// assert!(!allowed("LICENSE"));
/// ```
#[must_use]
pub fn allowed(name: &str) -> bool {
    if name == "powrprof.dll" {
        return true;
    }
    let Some(rest) = name.strip_prefix("mods/") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('/').collect();
    let plain = |part: &&str| {
        !part.is_empty()
            && *part != "."
            && *part != ".."
            && !part.starts_with('.')
            && !part.ends_with('.')
            && !part.ends_with(' ')
            && part.chars().all(|c| {
                !c.is_control() && !matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            })
    };
    let extension = |wanted: &str| {
        parts
            .last()
            .and_then(|file| Path::new(file).extension())
            .is_some_and(|e| e.eq_ignore_ascii_case(wanted))
    };
    let pack_data = parts.len() >= 2
        && !parts[0].eq_ignore_ascii_case(LOADER_DIR)
        && (extension("toml") || extension("png"));
    parts.len() <= MAX_DEPTH && parts.iter().all(plain) && (extension("dll") || pack_data)
}

/// The loader's folder under `mods/`, which only its own update writes DLLs
/// into and no pack.
const LOADER_DIR: &str = "enshwick";

/// Whether an archive entry `name` is unsafe and refuses the archive: a path
/// that could leave the game folder or reach the staging folder.
fn hostile(name: &str) -> bool {
    name.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name.split('/').any(|part| part == "..")
        || name.starts_with(STAGED)
}

/// Where each entry of an archive goes, relative to the game's folder:
/// `(entry index, path)`. With `installed`, the mod's DLL as the loader
/// lists it, an archive with one DLL directly under `mods/` writes that DLL
/// to `installed`, so a renamed DLL is replaced rather than doubled.
///
/// # Errors
///
/// Why the archive is refused: a hostile name, too many entries or no DLL.
///
/// # Examples
///
/// ```
/// use enshwick_updater::stage::plan;
///
/// let names = ["LICENSE".to_owned(), "mods/enshwick-safes.dll".to_owned()];
/// let to = plan(&names, Some("mods/enshwick_safes.dll")).unwrap();
/// assert_eq!(to, [(1, "mods/enshwick_safes.dll".to_owned())]);
/// ```
pub fn plan(names: &[String], installed: Option<&str>) -> Result<Vec<(usize, String)>, String> {
    if names.len() > MAX_ENTRIES {
        return Err(format!("{} entries, more than {MAX_ENTRIES}", names.len()));
    }
    if let Some(name) = names.iter().find(|name| hostile(name)) {
        return Err(format!("the entry {name:?} points outside the game folder"));
    }
    let mut files: Vec<(usize, String)> = names
        .iter()
        .enumerate()
        .filter(|(_, name)| allowed(name))
        .map(|(index, name)| (index, name.clone()))
        .collect();
    if files.is_empty() {
        return Err("nothing to install: no DLL and no pack files".to_owned());
    }
    let direct: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, (_, name))| name.matches('/').count() == 1)
        .map(|(at, _)| at)
        .collect();
    if let (Some(installed), [only]) = (installed.filter(|path| !path.is_empty()), &direct[..]) {
        if !allowed(installed) {
            return Err(format!("the installed path {installed:?} is not a mod's"));
        }
        if let Some(file) = files.get_mut(*only) {
            installed.clone_into(&mut file.1);
        }
    }
    Ok(files)
}

/// Refuses a plan that writes any of `taken`, files of other mods or of the
/// loader, compared without case as Windows does; a taken `pack.toml`
/// takes its whole folder.
///
/// # Errors
///
/// The first path that belongs to something else.
///
/// # Examples
///
/// ```
/// use enshwick_updater::stage::refuse_taken;
///
/// let files = [(0, "mods/enshwick/enshwick.dll".to_owned())];
/// let taken = ["mods/Enshwick/enshwick.dll".to_owned()];
/// assert!(refuse_taken(&files, &taken).is_err());
/// assert!(refuse_taken(&files, &[]).is_ok());
/// let pack = [(0, "mods/lanterns/loot.toml".to_owned())];
/// assert!(refuse_taken(&pack, &["mods/lanterns/pack.toml".to_owned()]).is_err());
/// ```
pub fn refuse_taken(files: &[(usize, String)], taken: &[String]) -> Result<(), String> {
    let holds = |taken: &str, to: &str| {
        taken.eq_ignore_ascii_case(to)
            || taken.strip_suffix("/pack.toml").is_some_and(|dir| {
                to.len() > dir.len()
                    && to.as_bytes()[dir.len()] == b'/'
                    && to[..dir.len()].eq_ignore_ascii_case(dir)
            })
    };
    match files
        .iter()
        .find(|(_, to)| taken.iter().any(|t| holds(t, to)))
    {
        Some((_, to)) => Err(format!("{to} belongs to another mod or the loader")),
        None => Ok(()),
    }
}

/// Unpacks a verified `archive` into `game`'s staging folder as [`plan`]
/// says. Every file is unpacked under `scratch` first and only then moved
/// into the staging folder, so a failure leaves nothing half-staged.
///
/// Returns the staged paths, relative to the game's folder.
///
/// # Errors
///
/// Why the archive was not staged.
pub fn stage(
    archive: &[u8],
    installed: Option<&str>,
    taken: &[String],
    game: &Path,
    scratch: &Path,
) -> Result<Vec<String>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive))
        .map_err(|e| format!("not a zip archive: {e}"))?;
    let names: Vec<String> = (0..zip.len())
        .map(|index| {
            zip.name_for_index(index)
                .and_then(Result::ok)
                .map(std::borrow::Cow::into_owned)
        })
        .collect::<Option<_>>()
        .ok_or("an entry name is unreadable")?;
    let files = plan(&names, installed)?;
    refuse_taken(&files, taken)?;
    // Scratch space of earlier runs goes first.
    let _gone = std::fs::remove_dir_all(scratch);
    let mut unpacked: Vec<(PathBuf, String)> = Vec::new();
    for (index, (entry, to)) in files.iter().enumerate() {
        let name = names.get(*entry).map_or("", String::as_str);
        let mut file = zip
            .by_index(*entry)
            .map_err(|e| format!("cannot read {name}: {e}"))?;
        if file.size() > MAX_FILE {
            return Err(format!("{name} is larger than {MAX_FILE} bytes"));
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot unpack {name}: {e}"))?;
        if bytes.len() as u64 > MAX_FILE {
            return Err(format!("{name} is larger than {MAX_FILE} bytes"));
        }
        let temporary = scratch.join(index.to_string());
        write(&temporary, &bytes)?;
        unpacked.push((temporary, to.clone()));
    }
    let mut moved: Vec<PathBuf> = Vec::new();
    for (temporary, to) in &unpacked {
        let target = game.join(STAGED).join(to);
        let placed = target
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::rename(temporary, &target));
        if let Err(e) = placed {
            for staged in &moved {
                let _gone = std::fs::remove_file(staged);
            }
            return Err(format!("cannot stage {to}: {e}"));
        }
        moved.push(target);
    }
    let _gone = std::fs::remove_dir_all(scratch);
    Ok(unpacked.into_iter().map(|(_, to)| to).collect())
}

/// Whether an update of `file` already waits in `game`'s staging folder.
#[must_use]
pub fn waiting(game: &Path, file: &str) -> bool {
    !file.is_empty() && allowed(file) && game.join(STAGED).join(file).is_file()
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, bytes))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            out.start_file(*name, options).unwrap();
            out.write_all(bytes).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-updater-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn only_dlls_where_enshwick_keeps_them_are_allowed() {
        for good in [
            "powrprof.dll",
            "mods/enshwick-safes.dll",
            "mods/enshwick/enshwick.dll",
            "mods/a/b/c/d.DLL",
        ] {
            assert!(allowed(good), "{good}");
        }
        for bad in [
            "LICENSE",
            "mods/readme.txt",
            "mods/",
            "mods//a.dll",
            "mods/./a.dll",
            "mods/../powrprof.dll",
            "mods/.staged/a.dll",
            "mods/a:stream.dll",
            "mods/a\\b.dll",
            "mods/a/b/c/d/e.dll",
            "enshrouded.exe",
            "other.dll",
            "/mods/a.dll",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[test]
    fn paths_that_escape_the_game_folder_refuse_the_archive() {
        for bad in [
            "../powrprof.dll",
            "mods/../../x.dll",
            "/etc/x.dll",
            "C:/Windows/x.dll",
            "mods\\..\\x.dll",
            "mods/.staged/powrprof.dll",
        ] {
            let list = names(&["mods/a.dll", bad]);
            assert!(plan(&list, None).is_err(), "{bad}");
        }
        assert!(plan(&names(&["LICENSE"]), None).is_err());
        let many: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("mods/{i}.dll")).collect();
        assert!(plan(&many, None).is_err());
    }

    #[test]
    fn archive_paths_are_kept_and_a_mods_dll_follows_its_installed_name() {
        let loader = names(&["LICENSE", "powrprof.dll", "mods/enshwick/enshwick.dll"]);
        assert_eq!(
            plan(&loader, Some("mods/enshwick/enshwick.dll")).unwrap(),
            [
                (1, "powrprof.dll".to_owned()),
                (2, "mods/enshwick/enshwick.dll".to_owned())
            ]
        );
        let safes = names(&["mods/", "mods/enshwick-safes.dll", "LICENSE"]);
        assert_eq!(
            plan(&safes, None).unwrap(),
            [(1, "mods/enshwick-safes.dll".to_owned())]
        );
        assert_eq!(
            plan(&safes, Some("mods/enshwick_safes.dll")).unwrap(),
            [(1, "mods/enshwick_safes.dll".to_owned())]
        );
        assert!(plan(&safes, Some("../evil.dll")).is_err());
        let two = names(&["mods/a.dll", "mods/b.dll"]);
        assert_eq!(
            plan(&two, Some("mods/c.dll")).unwrap(),
            [(0, "mods/a.dll".to_owned()), (1, "mods/b.dll".to_owned())]
        );
    }

    #[test]
    fn an_archive_is_staged_under_the_game_folder() {
        let game = scratch("game");
        let archive = zip(&[
            ("LICENSE", b"MIT"),
            ("powrprof.dll", b"stub"),
            ("mods/enshwick/enshwick.dll", b"loader"),
        ]);
        let staged = stage(&archive, None, &[], &game, &game.join("scratch")).unwrap();
        assert_eq!(staged, ["powrprof.dll", "mods/enshwick/enshwick.dll"]);
        let root = game.join(STAGED);
        assert_eq!(std::fs::read(root.join("powrprof.dll")).unwrap(), b"stub");
        assert_eq!(
            std::fs::read(root.join("mods/enshwick/enshwick.dll")).unwrap(),
            b"loader"
        );
        assert!(!root.join("LICENSE").exists());
        assert!(!game.join("scratch").exists());
        assert!(waiting(&game, "powrprof.dll"));
        assert!(!waiting(&game, "mods/other.dll"));
        let _gone = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_hostile_archive_stages_nothing() {
        let game = scratch("hostile");
        let archive = zip(&[("mods/a.dll", b"a"), ("../../x.dll", b"x")]);
        assert!(stage(&archive, None, &[], &game, &game.join("scratch")).is_err());
        assert!(!game.join(STAGED).exists());
        assert!(stage(b"not a zip", None, &[], &game, &game.join("scratch")).is_err());
        let _gone = std::fs::remove_dir_all(&game);
    }
}
