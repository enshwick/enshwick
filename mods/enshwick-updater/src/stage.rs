//! Staging: a downloaded archive's DLLs and content pack files, or a
//! downloaded DLL, written to `mods/.staged/`, which the `powrprof.dll` stub
//! moves into place at the next start.
//!
//! Each update is one folder `mods/.staged/<unit>/`, its name all digits so
//! the stub applies the units oldest first, holding the files as they go
//! from the game's folder. A unit is written in full apart and renamed into
//! the staging folder in one step: a crash while staging leaves no unit, and
//! the stub takes nothing else.
//!
//! An archive holds the mod's DLL at `mods/<file>.dll`, at its root, or
//! either of those inside one top-level folder. Only files Enshwick reads
//! are taken, and only where it keeps them: `powrprof.dll` beside the game,
//! `*.dll` under `mods/`, and a content pack's `.toml` and `.png` files in
//! its folder `mods/<id>/`, never in the loader's; an unsigned update takes
//! the mod's own DLL alone. A name that is absolute, goes up with `..`, uses
//! `\`, a drive or a stream, or points into the staging folder itself
//! refuses the whole archive.

use std::io::Read as _;
use std::path::{Path, PathBuf};

/// The staging folder, relative to the game's folder.
pub const STAGED: &str = "mods/.staged";
/// The most digits of a unit's name.
const UNIT_DIGITS: usize = 20;
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
/// assert!(allowed("mods/enshwick/enshwick.dll"));
/// assert!(!allowed("mods/enshwick/dbghelp.dll"));
/// assert!(!allowed("mods/ENSHWI~1.DLL"));
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
            // Windows reads `~` in an 8.3 short name and folds letters past
            // ASCII without case: either could name another file than the
            // one the checks compared.
            && part.chars().all(|c| {
                c.is_ascii()
                    && !c.is_ascii_control()
                    && !matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '~')
            })
    };
    let extension = |wanted: &str| {
        parts
            .last()
            .and_then(|file| Path::new(file).extension())
            .is_some_and(|e| e.eq_ignore_ascii_case(wanted))
    };
    let in_loader_dir = parts.len() >= 2 && parts[0].eq_ignore_ascii_case(LOADER_DIR);
    let pack_data = parts.len() >= 2 && !in_loader_dir && (extension("toml") || extension("png"));
    // The stub loads the loader with its folder searched first, so no other
    // DLL may land there.
    let dll = extension("dll") && (!in_loader_dir || name.eq_ignore_ascii_case(LOADER_FILE));
    parts.len() <= MAX_DEPTH && parts.iter().all(plain) && (dll || pack_data)
}

/// The loader's folder under `mods/`, which only its own update writes DLLs
/// into and no pack.
const LOADER_DIR: &str = "enshwick";
/// The loader's DLL, the one DLL its folder takes.
const LOADER_FILE: &str = enshwick_loader_api::mods::LOADER_FILE;

/// Whether an archive entry `name` is unsafe and refuses the archive: a path
/// that could leave the game folder or reach the staging folder.
fn hostile(name: &str) -> bool {
    name.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name.split('/').any(|part| part == "..")
        || name.starts_with(STAGED)
}

/// Whether `name`'s extension is `.dll`, in any case.
fn is_dll(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
}

/// The archive's names as paths from the game's folder: one top-level
/// folder that holds every entry, other than `mods/`, is dropped, and a DLL
/// at the root other than `powrprof.dll` goes under `mods/`.
fn laid_out(names: &[String]) -> Vec<String> {
    let top = names
        .first()
        .and_then(|name| name.split_once('/'))
        .map(|(top, _)| top)
        .filter(|top| !top.eq_ignore_ascii_case("mods"))
        .filter(|top| {
            names
                .iter()
                .all(|name| name.split_once('/').is_some_and(|(first, _)| first == *top))
        });
    names
        .iter()
        .map(|name| {
            let name = top.map_or(name.as_str(), |top| &name[top.len() + 1..]);
            if !name.contains('/') && is_dll(name) && !name.eq_ignore_ascii_case("powrprof.dll") {
                format!("mods/{name}")
            } else {
                name.to_owned()
            }
        })
        .collect()
}

/// Where each entry of an archive goes, relative to the game's folder:
/// `(entry index, path)`. With `installed`, the mod's DLL as the loader
/// lists it, an archive with one DLL directly under `mods/` writes that DLL
/// to `installed`, so a renamed DLL is replaced rather than doubled. With
/// `own`, for an unsigned update, only the mod's DLL is taken: the one with
/// the installed DLL's file name, or the archive's only DLL.
///
/// # Errors
///
/// Why the archive is refused: a hostile name, too many entries, no DLL, or
/// with `own` no single DLL that is the mod's.
///
/// # Examples
///
/// ```
/// use enshwick_updater::stage::plan;
///
/// let names = ["LICENSE".to_owned(), "mods/enshwick-safes.dll".to_owned()];
/// let to = plan(&names, Some("mods/enshwick_safes.dll"), false).unwrap();
/// assert_eq!(to, [(1, "mods/enshwick_safes.dll".to_owned())]);
/// let root = ["my-mod-1.2.0/".to_owned(), "my-mod-1.2.0/my_mod.dll".to_owned()];
/// assert_eq!(plan(&root, None, true).unwrap(), [(1, "mods/my_mod.dll".to_owned())]);
/// ```
pub fn plan(
    names: &[String],
    installed: Option<&str>,
    own: bool,
) -> Result<Vec<(usize, String)>, String> {
    if names.len() > MAX_ENTRIES {
        return Err(format!("{} entries, more than {MAX_ENTRIES}", names.len()));
    }
    if let Some(name) = names.iter().find(|name| hostile(name)) {
        return Err(format!("the entry {name:?} points outside the game folder"));
    }
    let installed = installed.filter(|path| !path.is_empty());
    if let Some(installed) = installed.filter(|path| !allowed(path)) {
        return Err(format!("the installed path {installed:?} is not a mod's"));
    }
    let mut files: Vec<(usize, String)> = laid_out(names)
        .into_iter()
        .enumerate()
        .filter(|(_, name)| allowed(name))
        .collect();
    if own {
        return own_dll(files, installed).map(|file| vec![file]);
    }
    if files.is_empty() {
        return Err("nothing to install: no DLL and no pack files".to_owned());
    }
    let direct: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, (_, name))| name.matches('/').count() == 1)
        .map(|(at, _)| at)
        .collect();
    if let (Some(installed), [only]) = (installed, &direct[..])
        && let Some(file) = files.get_mut(*only)
    {
        installed.clone_into(&mut file.1);
    }
    Ok(files)
}

/// The mod's own DLL among `files`, written to `installed` when the game
/// has the mod.
fn own_dll(
    files: Vec<(usize, String)>,
    installed: Option<&str>,
) -> Result<(usize, String), String> {
    let file_name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let dlls: Vec<(usize, String)> = files
        .into_iter()
        .filter(|(_, name)| {
            is_dll(name) && name.starts_with("mods/") && name.matches('/').count() == 1
        })
        .collect();
    let wanted = installed.map(file_name);
    let same = dlls
        .iter()
        .find(|(_, name)| Some(file_name(name)) == wanted);
    let (index, path) = match (same, &dlls[..]) {
        (Some(found), _) | (None, [found]) => found.clone(),
        (None, []) => return Err("nothing to install: no DLL".to_owned()),
        (None, _) => {
            return Err(format!(
                "{} DLLs, and none has the installed one's name",
                dlls.len()
            ));
        }
    };
    Ok((index, installed.map_or(path, str::to_owned)))
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

/// What an update downloaded: its file name, a `.zip` archive or a `.dll`,
/// and its bytes.
#[derive(Clone, Copy, Debug)]
pub struct Download<'a> {
    /// The file's name, such as `my-mod.zip` or `my_mod.dll`.
    pub name: &'a str,
    /// The file's bytes.
    pub bytes: &'a [u8],
}

/// Unpacks a verified `download` into `game`'s staging folder as [`plan`]
/// says, with `installed` and `own`; a `.dll` is staged as an archive that
/// holds it at its root would be. Every file is unpacked under `scratch`
/// first, and the unit they make is then renamed into the staging folder in
/// one step, so a failure or a crash leaves nothing half-staged.
///
/// Returns the staged paths, relative to the game's folder.
///
/// # Errors
///
/// Why the download was not staged.
pub fn stage(
    download: Download<'_>,
    installed: Option<&str>,
    own: bool,
    taken: &[String],
    game: &Path,
    scratch: &Path,
) -> Result<Vec<String>, String> {
    let mut zip = if is_dll(download.name) {
        None
    } else {
        Some(
            zip::ZipArchive::new(std::io::Cursor::new(download.bytes))
                .map_err(|e| format!("not a zip archive: {e}"))?,
        )
    };
    let names: Vec<String> = match &zip {
        None => vec![download.name.to_owned()],
        Some(zip) => (0..zip.len())
            .map(|index| {
                zip.name_for_index(index)
                    .and_then(Result::ok)
                    .map(std::borrow::Cow::into_owned)
            })
            .collect::<Option<_>>()
            .ok_or("an entry name is unreadable")?,
    };
    let files = plan(&names, installed, own)?;
    refuse_taken(&files, taken)?;
    // A DLL in `mods/` that is not the mod's own belongs to a mod switched
    // off or skipped, which the loader does not list.
    let others: Vec<String> = dlls_on_disk(game)
        .into_iter()
        .filter(|file| installed.is_none_or(|own| !own.eq_ignore_ascii_case(file)))
        .collect();
    refuse_taken(&files, &others)?;
    // Scratch space of earlier runs goes first.
    let _gone = std::fs::remove_dir_all(scratch);
    let unit = scratch.join("unit");
    let mut unpacked: Vec<String> = Vec::new();
    for (entry, to) in &files {
        let name = names.get(*entry).map_or("", String::as_str);
        let bytes = match zip.as_mut() {
            None => download.bytes.to_vec(),
            Some(zip) => unpack(zip, *entry, name)?,
        };
        if bytes.len() as u64 > MAX_FILE {
            return Err(format!("{name} is larger than {MAX_FILE} bytes"));
        }
        write(&unit.join(to), &bytes)?;
        unpacked.push(to.clone());
    }
    let staged = game.join(STAGED);
    let placed = std::fs::create_dir_all(&staged).and_then(|()| {
        let mut name = unit_name(std::time::SystemTime::now());
        while staged.join(&name).exists() {
            name = next_unit(&name);
        }
        std::fs::rename(&unit, staged.join(name))
    });
    let _gone = std::fs::remove_dir_all(scratch);
    placed.map_err(|e| format!("cannot stage the update: {e}"))?;
    Ok(unpacked)
}

/// A unit's name for an update staged at `at`: its nanoseconds since 1970,
/// zero-padded, so names sort as the updates came.
fn unit_name(at: std::time::SystemTime) -> String {
    let nanos = at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{nanos:0UNIT_DIGITS$}")
}

/// The name after `name`, for two updates staged in the same nanosecond.
fn next_unit(name: &str) -> String {
    let next = name.parse::<u128>().map_or(0, |n| n.saturating_add(1));
    format!("{next:0UNIT_DIGITS$}")
}

/// Whether `name` names a unit: 1 to [`UNIT_DIGITS`] ASCII digits, as the
/// stub reads them.
fn is_unit(name: &str) -> bool {
    (1..=UNIT_DIGITS).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_digit())
}

/// The units in `game`'s staging folder.
fn units(game: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(game.join(STAGED)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|entry| entry.file_name().to_str().is_some_and(is_unit))
        .map(|entry| entry.path())
        .collect()
}

/// The DLLs directly in `game`'s `mods/` folder, as `mods/<file>`.
fn dlls_on_disk(game: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(game.join("mods")) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_dll(name))
        .map(|name| format!("mods/{name}"))
        .collect()
}

/// Entry `entry` of `zip`, named `name`, read up to the size limit.
fn unpack(
    zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    entry: usize,
    name: &str,
) -> Result<Vec<u8>, String> {
    let mut file = zip
        .by_index(entry)
        .map_err(|e| format!("cannot read {name}: {e}"))?;
    if file.size() > MAX_FILE {
        return Err(format!("{name} is larger than {MAX_FILE} bytes"));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot unpack {name}: {e}"))?;
    Ok(bytes)
}

/// Deletes the DLLs of the mods that `id` replaces, from `replaced` as the
/// loader lists them: each a `.dll` directly under `mods/` that no entry of
/// `taken` names, compared without case. Returns the files deleted and,
/// apart, why others were not.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::ModEntry;
/// use enshwick_updater::stage::delete_replaced;
///
/// let game = std::env::temp_dir().join(format!("enshwick-doc-replaced-{}", std::process::id()));
/// std::fs::create_dir_all(game.join("mods"))?;
/// std::fs::write(game.join("mods/old.dll"), b"old")?;
/// let old = ModEntry {
///     id: "old".into(),
///     file: "mods/old.dll".into(),
///     replaced_by: "new".into(),
///     ..ModEntry::default()
/// };
/// let (deleted, kept) = delete_replaced(&game, "new", &[old], &[]);
/// assert_eq!(deleted, ["mods/old.dll"]);
/// assert!(kept.is_empty() && !game.join("mods/old.dll").exists());
/// # std::fs::remove_dir_all(&game)?;
/// # Ok::<(), std::io::Error>(())
/// ```
#[must_use]
pub fn delete_replaced(
    game: &Path,
    id: &str,
    replaced: &[enshwick_loader_api::mods::ModEntry],
    taken: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut deleted = Vec::new();
    let mut kept = Vec::new();
    for old in replaced.iter().filter(|old| old.replaced_by == id) {
        let file = old.file.as_str();
        let direct = file.matches('/').count() == 1
            && Path::new(file)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"));
        if !allowed(file) || !direct || taken.iter().any(|t| t.eq_ignore_ascii_case(file)) {
            kept.push(format!("{file} of {} is not a file to delete", old.id));
            continue;
        }
        match std::fs::remove_file(game.join(file)) {
            Ok(()) => deleted.push(file.to_owned()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => kept.push(format!("{file} of {}: {e}", old.id)),
        }
    }
    (deleted, kept)
}

/// The file under the updater's data folder that names the mods installed
/// where no copy of them ran: the loader lists the mods they replace only
/// once they run, at the next start.
const FRESH: &str = "fresh-installs";

/// Notes in the data folder `data` that `id` was installed where no copy of
/// it ran.
pub(crate) fn note_fresh(data: &Path, id: &str) -> std::io::Result<()> {
    let path = data.join(FRESH);
    let mut ids = std::fs::read_to_string(&path).unwrap_or_default();
    if ids.lines().all(|line| line != id) {
        ids.push_str(id);
        ids.push('\n');
        write(&path, ids.as_bytes()).map_err(std::io::Error::other)?;
    }
    Ok(())
}

/// After a start: for each mod noted by [`note_fresh`] that runs now, one
/// of `installed`, deletes the DLLs of the mods it replaces as
/// [`delete_replaced`] does; the note is gone afterwards, also for a mod
/// that does not run. Returns each file deleted with the id that replaces
/// it, and apart why others were not.
pub(crate) fn delete_replaced_of_fresh(
    game: &Path,
    data: &Path,
    installed: &[enshwick_loader_api::mods::ModEntry],
    replaced: &[enshwick_loader_api::mods::ModEntry],
    taken: &[String],
) -> (Vec<(String, String)>, Vec<String>) {
    let path = data.join(FRESH);
    let Ok(ids) = std::fs::read_to_string(&path) else {
        return (Vec::new(), Vec::new());
    };
    let _gone = std::fs::remove_file(&path);
    let mut deleted = Vec::new();
    let mut kept = Vec::new();
    for id in ids
        .lines()
        .filter(|id| installed.iter().any(|m| m.id == *id))
    {
        let (files, why) = delete_replaced(game, id, replaced, taken);
        deleted.extend(files.into_iter().map(|file| (file, id.to_owned())));
        kept.extend(why);
    }
    (deleted, kept)
}

/// Whether an update of `file` already waits in `game`'s staging folder.
#[must_use]
pub fn waiting(game: &Path, file: &str) -> bool {
    !file.is_empty() && allowed(file) && units(game).iter().any(|unit| unit.join(file).is_file())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, bytes))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_update_never_writes_the_dll_of_a_mod_the_loader_does_not_list() {
        let game = std::env::temp_dir().join(format!("enshwick-stage-off-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/enshwick-safes.dll"), b"off").unwrap();
        std::fs::write(game.join("mods/my_mod.dll"), b"old").unwrap();
        let scratch = game.join("scratch");
        let fresh = Download {
            name: "enshwick-safes.dll",
            bytes: b"other",
        };
        let refused = stage(fresh, None, true, &[], &game, &scratch);
        assert!(refused.is_err(), "{refused:?}");
        assert_eq!(
            std::fs::read(game.join("mods/enshwick-safes.dll")).unwrap(),
            b"off"
        );
        let own = Download {
            name: "MY_MOD.dll",
            bytes: b"new",
        };
        let staged = stage(own, Some("mods/my_mod.dll"), true, &[], &game, &scratch);
        assert_eq!(staged, Ok(vec!["mods/my_mod.dll".to_owned()]));
        let _gone = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn an_unsigned_update_takes_no_dll_below_mods() {
        let names = ["mods/enshwick/x.dll".to_owned()];
        assert!(plan(&names, None, true).is_err());
        let deep = ["mods/pack/helper.dll".to_owned()];
        assert!(plan(&deep, None, true).is_err());
        assert!(plan(&deep, None, false).is_ok(), "a signed archive may");
    }

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
            assert!(plan(&list, None, false).is_err(), "{bad}");
        }
        assert!(plan(&names(&["LICENSE"]), None, false).is_err());
        let many: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("mods/{i}.dll")).collect();
        assert!(plan(&many, None, false).is_err());
    }

    #[test]
    fn archive_paths_are_kept_and_a_mods_dll_follows_its_installed_name() {
        let loader = names(&["LICENSE", "powrprof.dll", "mods/enshwick/enshwick.dll"]);
        assert_eq!(
            plan(&loader, Some("mods/enshwick/enshwick.dll"), false).unwrap(),
            [
                (1, "powrprof.dll".to_owned()),
                (2, "mods/enshwick/enshwick.dll".to_owned())
            ]
        );
        let safes = names(&["mods/", "mods/enshwick-safes.dll", "LICENSE"]);
        assert_eq!(
            plan(&safes, None, false).unwrap(),
            [(1, "mods/enshwick-safes.dll".to_owned())]
        );
        assert_eq!(
            plan(&safes, Some("mods/enshwick_safes.dll"), false).unwrap(),
            [(1, "mods/enshwick_safes.dll".to_owned())]
        );
        assert!(plan(&safes, Some("../evil.dll"), false).is_err());
        let two = names(&["mods/a.dll", "mods/b.dll"]);
        assert_eq!(
            plan(&two, Some("mods/c.dll"), false).unwrap(),
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
        let download = Download {
            name: "enshwick-loader.zip",
            bytes: &archive,
        };
        let staged = stage(download, None, false, &[], &game, &game.join("scratch")).unwrap();
        assert_eq!(staged, ["powrprof.dll", "mods/enshwick/enshwick.dll"]);
        let [root] = &units(&game)[..] else {
            panic!("one unit");
        };
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
    fn each_update_is_one_unit_named_in_order_and_nothing_half_written_counts() {
        let game = scratch("units");
        let one = zip(&[("mods/a.dll", b"a1")]);
        let two = zip(&[("mods/a.dll", b"a2"), ("mods/b.dll", b"b")]);
        for bytes in [&one, &two] {
            let download = Download {
                name: "a.zip",
                bytes,
            };
            stage(download, None, false, &[], &game, &game.join("scratch")).unwrap();
        }
        let mut found = units(&game);
        found.sort();
        assert_eq!(found.len(), 2);
        assert_eq!(std::fs::read(found[0].join("mods/a.dll")).unwrap(), b"a1");
        assert_eq!(std::fs::read(found[1].join("mods/a.dll")).unwrap(), b"a2");
        assert!(waiting(&game, "mods/b.dll"));
        // What a crash leaves while staging is no unit, so nothing waits.
        let fresh = scratch("units-crash");
        std::fs::create_dir_all(fresh.join(STAGED).join(".unit")).unwrap();
        write(&fresh.join("scratch/unit/mods/c.dll"), b"half").unwrap();
        write(&fresh.join(STAGED).join(".unit/mods/c.dll"), b"half").unwrap();
        assert!(!waiting(&fresh, "mods/c.dll"));
        assert!(units(&fresh).is_empty());
        let _gone = std::fs::remove_dir_all(&game);
        let _gone = std::fs::remove_dir_all(&fresh);
    }

    #[test]
    fn unit_names_sort_as_the_updates_came() {
        let at = |nanos| std::time::UNIX_EPOCH + std::time::Duration::from_nanos(nanos);
        let (early, late) = (unit_name(at(900)), unit_name(at(1_000)));
        assert!(early < late);
        assert_eq!(early.len(), UNIT_DIGITS);
        assert!(is_unit(&early) && is_unit(&next_unit(&early)));
        assert_eq!(next_unit("00000000000000000999"), "00000000000000001000");
        assert!(!is_unit(".unit") && !is_unit("mods") && !is_unit(""));
        assert!(unit_name(std::time::SystemTime::now()).len() <= UNIT_DIGITS);
    }

    fn replaced(id: &str, file: &str, by: &str) -> enshwick_loader_api::mods::ModEntry {
        enshwick_loader_api::mods::ModEntry {
            id: id.into(),
            file: file.into(),
            replaced_by: by.into(),
            ..enshwick_loader_api::mods::ModEntry::default()
        }
    }

    #[test]
    fn only_the_dlls_of_the_replaced_mods_are_deleted() {
        let game = scratch("replaced");
        let mods = game.join("mods");
        for file in [
            "enshwick_proton_optimizer.dll",
            "enshwick_optimizer.dll",
            "enshwick_dev_tools.dll",
            "keep.txt",
            "enshwick/enshwick.dll",
        ] {
            write(&mods.join(file), b"x").unwrap();
        }
        let listed = [
            replaced(
                "proton-optimizer",
                "mods/enshwick_proton_optimizer.dll",
                "optimizer",
            ),
            replaced("dev-tools", "mods/enshwick_dev_tools.dll", "admin-tools"),
            replaced("taken", "mods/enshwick_optimizer.dll", "optimizer"),
            replaced("not-a-dll", "mods/keep.txt", "optimizer"),
            replaced("loader", "mods/enshwick/enshwick.dll", "optimizer"),
            replaced("outside", "mods/../powrprof.dll", "optimizer"),
            replaced("gone", "mods/gone.dll", "optimizer"),
        ];
        let taken = ["mods/Enshwick_Optimizer.dll".to_owned()];
        let (deleted, kept) = delete_replaced(&game, "optimizer", &listed, &taken);
        assert_eq!(deleted, ["mods/enshwick_proton_optimizer.dll"]);
        assert_eq!(kept.len(), 4, "{kept:?}");
        assert!(!mods.join("enshwick_proton_optimizer.dll").exists());
        for file in [
            "enshwick_optimizer.dll",
            "enshwick_dev_tools.dll",
            "keep.txt",
            "enshwick/enshwick.dll",
        ] {
            assert!(mods.join(file).exists(), "{file}");
        }
        let _gone = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_fresh_install_deletes_what_it_replaces_at_the_next_start() {
        let game = scratch("fresh");
        let data = game.join("mods/enshwick/data/enshwick-updater");
        let mods = game.join("mods");
        for file in ["old.dll", "new.dll", "other.dll"] {
            write(&mods.join(file), b"x").unwrap();
        }
        note_fresh(&data, "new").unwrap();
        note_fresh(&data, "new").unwrap();
        note_fresh(&data, "absent").unwrap();
        assert_eq!(
            std::fs::read_to_string(data.join(FRESH)).unwrap(),
            "new\nabsent\n"
        );
        let running = |id: &str, file: &str| enshwick_loader_api::mods::ModEntry {
            id: id.into(),
            file: file.into(),
            ..enshwick_loader_api::mods::ModEntry::default()
        };
        let installed = [
            running("new", "mods/new.dll"),
            running("other", "mods/other.dll"),
        ];
        let listed = [
            replaced("old", "mods/old.dll", "new"),
            replaced("by-other", "mods/other.dll", "new"),
            replaced("loader", "mods/enshwick/enshwick.dll", "new"),
            replaced("gone", "mods/gone.dll", "absent"),
        ];
        let taken = ["mods/new.dll".to_owned(), "mods/other.dll".to_owned()];
        let (deleted, kept) = delete_replaced_of_fresh(&game, &data, &installed, &listed, &taken);
        assert_eq!(deleted, [("mods/old.dll".to_owned(), "new".to_owned())]);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert!(!mods.join("old.dll").exists());
        assert!(mods.join("new.dll").exists() && mods.join("other.dll").exists());
        assert!(!data.join(FRESH).exists());
        let again = delete_replaced_of_fresh(&game, &data, &installed, &listed, &taken);
        assert_eq!(again, (Vec::new(), Vec::new()));
        let _gone = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_hostile_archive_stages_nothing() {
        let game = scratch("hostile");
        let archive = zip(&[("mods/a.dll", b"a"), ("../../x.dll", b"x")]);
        let staged = |name, bytes| {
            stage(
                Download { name, bytes },
                None,
                false,
                &[],
                &game,
                &game.join("scratch"),
            )
        };
        assert!(staged("a.zip", &archive).is_err());
        assert!(!game.join(STAGED).exists());
        assert!(staged("a.zip", b"not a zip").is_err());
        assert!(staged("../a.dll", b"dll").is_err());
        let _gone = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn the_dll_sits_under_mods_at_the_root_or_in_one_folder() {
        let to = |list: &[&str]| plan(&names(list), Some("mods/my_mod.dll"), false).unwrap();
        let mine = |at: usize| vec![(at, "mods/my_mod.dll".to_owned())];
        assert_eq!(to(&["mods/my-mod.dll", "LICENSE"]), mine(0));
        assert_eq!(to(&["LICENSE", "my-mod.dll"]), mine(1));
        assert_eq!(to(&["my-mod-1.2.0/", "my-mod-1.2.0/my-mod.dll"]), mine(1));
        assert_eq!(to(&["my-mod/README.md", "my-mod/mods/my-mod.dll"]), mine(1));
        assert!(
            plan(&names(&["my-mod/a.dll", "other/b.dll"]), None, false).is_err(),
            "files in two folders are not where Enshwick keeps them"
        );
        assert_eq!(
            plan(&names(&["powrprof.dll"]), None, false).unwrap(),
            [(0, "powrprof.dll".to_owned())]
        );
    }

    #[test]
    fn an_unsigned_update_takes_the_mods_own_dll_alone() {
        let own = |list: &[&str], installed| plan(&names(list), installed, true);
        let installed = Some("mods/enshwick_my_mod.dll");
        assert_eq!(
            own(
                &["my_mod.dll", "mods/pack/items.toml", "powrprof.dll"],
                installed
            )
            .unwrap(),
            [(0, "mods/enshwick_my_mod.dll".to_owned())]
        );
        assert_eq!(
            own(&["mods/helper.dll", "mods/enshwick_MY_mod.dll"], installed).unwrap(),
            [(1, "mods/enshwick_my_mod.dll".to_owned())],
            "the DLL with the installed name"
        );
        assert!(own(&["mods/a.dll", "mods/b.dll"], installed).is_err());
        assert!(own(&["README.md"], installed).is_err());
        assert_eq!(
            own(&["x/my_mod.dll"], None).unwrap(),
            [(0, "mods/my_mod.dll".to_owned())]
        );
    }

    #[test]
    fn a_downloaded_dll_is_staged_as_the_mods_dll() {
        let game = scratch("dll");
        let download = Download {
            name: "MyMod.dll",
            bytes: b"dll",
        };
        let installed = Some("mods/enshwick_my_mod.dll");
        let staged = stage(download, installed, true, &[], &game, &game.join("scratch")).unwrap();
        assert_eq!(staged, ["mods/enshwick_my_mod.dll"]);
        assert_eq!(
            std::fs::read(units(&game)[0].join("mods/enshwick_my_mod.dll")).unwrap(),
            b"dll"
        );
        let taken = ["mods/MyMod.dll".to_owned()];
        assert!(
            stage(download, None, true, &taken, &game, &game.join("scratch")).is_err(),
            "never another mod's file"
        );
        let _gone = std::fs::remove_dir_all(&game);
    }
}
