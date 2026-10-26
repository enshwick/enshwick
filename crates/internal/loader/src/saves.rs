//! Saves on demand and the world's key for mods' per-world data.
//!
//! A save request sets the flags the server's own 30-second save sets; the
//! server writes the world over the next loop iterations and
//! `Event::WORLD_SAVED` follows. The world key comes from the path of the
//! world's save file, which the engine builds whenever it reads or writes
//! it: the loader links the path builder and keeps the world path built
//! last.
//!
//! A dedicated server runs one world, so its key is the last one built. A
//! client also builds the paths of all its worlds for its menus, and its
//! save system builds them on a thread of its own, not on the hosted game's
//! (client 410 and 426). There the key is the last one built since the
//! hosted game started to initialise, and none once that game ends.

use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick::abi::{HookFlags, Side, Status};
use enshwick_reflect::fnv1a32;

use crate::chains::Next;
use crate::host::Loader;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "save.flags",
    "save.path",
    "save.path_data",
    "save.path_lengths",
    "save.path_parts",
    "save.phase",
    "server.save_blocked",
    "server.save_request",
    "server.save_system",
];

/// The file name extension of a world save.
const WORLD_EXTENSION: &str = "ksd";

/// The starts of the names of a client's other save files, which share the
/// world saves' folder and extension: its characters and its global save.
const OTHER_SAVES: &[&str] = &["character", "global"];

type RequestFn = unsafe extern "system" fn(server: *mut u8);
type PathFn = unsafe extern "system" fn(result: *mut u8, path: *mut u8, slot: *mut u8) -> *mut u8;

/// The game functions and offsets, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    request: RequestFn,
    save_system: usize,
    blocked: usize,
    flags: usize,
    phase: usize,
    path_data: usize,
    path_lengths: usize,
    path_parts: usize,
    side: Side,
}

/// `flags` bit: the world may be saved now, set once it has loaded.
const SAVE_ALLOWED: u8 = 1 << 2;

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
// SAFETY: each slot joins the chain of the function its type names.
static PATH_NEXT: Next<PathFn> = unsafe { Next::new() };

/// The world key built last, and on a client whether a hosted game names
/// its world now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Keys {
    latest: Option<&'static str>,
    open: bool,
}

impl Keys {
    /// No key, and a client keeps none until a hosted game starts.
    const CLOSED: Self = Self {
        latest: None,
        open: false,
    };
    /// No key yet, and a client keeps the next ones.
    const OPEN: Self = Self {
        latest: None,
        open: true,
    };

    /// Keeps `key` when `side` names its world now.
    fn note(&mut self, side: Side, key: &'static str) {
        if side == Side::SERVER || self.open {
            self.latest = Some(key);
        }
    }

    /// The key of the world `side` runs.
    fn pick(self, side: Side) -> Option<&'static str> {
        (side == Side::SERVER || self.open)
            .then_some(self.latest)
            .flatten()
    }
}

static KEYS: Mutex<Keys> = Mutex::new(Keys::CLOSED);

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let constant = |name: &str| {
                loader
                    .build
                    .constant(name)
                    .and_then(|v| usize::try_from(v).ok())
            };
            let request =
                usize::try_from(loader.base + loader.build.symbol("server.save_request")?).ok()?;
            Some(Layout {
                // SAFETY: the symbol is the game function with this signature
                // on this build.
                request: unsafe { core::mem::transmute::<usize, RequestFn>(request) },
                save_system: constant("server.save_system")?,
                blocked: constant("server.save_blocked")?,
                flags: constant("save.flags")?,
                phase: constant("save.phase")?,
                path_data: constant("save.path_data")?,
                path_lengths: constant("save.path_lengths")?,
                path_parts: constant("save.path_parts")?,
                side: loader.side,
            })
        })
        .as_ref()
}

/// Links the save path builder, for the world key.
pub(crate) fn link(loader: &'static Loader) {
    if layout(loader).is_none() {
        crate::log!(WARN, "world keys off: this build has no save layout");
        return;
    }
    let target = loader.address("save.path");
    loader.link_feature("world keys", target, &PATH_NEXT, built, HookFlags::NONE);
}

/// The switch of the save path notes.
static SAVES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("save path notes");

unsafe extern "system" fn built(result: *mut u8, path: *mut u8, slot: *mut u8) -> *mut u8 {
    // SAFETY: forwarded unchanged.
    let out = unsafe { PATH_NEXT.get()(result, path, slot) };
    let layout = LAYOUT.get().copied().flatten();
    // SAFETY: the game wrote its result's first byte, 0 on success.
    let code = (!result.is_null()).then(|| unsafe { result.read() });
    if let Some(layout) = layout {
        let _caught = SAVES_FUSE.run("the built", || {
            // SAFETY: the path the game has just built.
            let text = unsafe { read_path(&layout, path) };
            // SAFETY: no preconditions.
            let thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
            crate::log!(
                DEBUG,
                "save path built ({code:?}) on thread {thread}: {text:?}"
            );
            if code == Some(0)
                && let Some(text) = text
            {
                remember(&text, layout.side);
            }
        });
    }
    out
}

/// The text of an engine path: its buffer and the lengths of its parts.
///
/// # Safety
///
/// `path` is null or an engine path.
unsafe fn read_path(layout: &Layout, path: *mut u8) -> Option<String> {
    if path.is_null() {
        return None;
    }
    let path = path.addr();
    // SAFETY: an engine path points at its buffer.
    let data: usize = unsafe { crate::memory::read(path, layout.path_data) };
    let len = (0..layout.path_parts)
        // SAFETY: the path holds the lengths of its parts one after another.
        .map(|part| unsafe { crate::memory::read::<u64>(path, layout.path_lengths + part * 8) })
        .try_fold(0u64, u64::checked_add)?;
    let len = usize::try_from(len)
        .ok()
        .filter(|&len| len > 0 && len < 0x800)?;
    if data == 0 {
        return None;
    }
    // SAFETY: the buffer holds `len` bytes of the path.
    let bytes =
        unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(data), len) };
    core::str::from_utf8(bytes).ok().map(str::to_owned)
}

/// Keeps the key of `path` when it names a world save.
fn remember(path: &str, side: Side) {
    let Some(key) = key_of(path) else {
        return;
    };
    let key = intern(&key, path);
    KEYS.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .note(side, key);
}

/// Every world key seen, kept once for the process: a menu that builds the
/// paths of several worlds sees each key again and again.
static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// `key`, kept for the process; logged the first time, from `path`.
fn intern(key: &str, path: &str) -> &'static str {
    let mut seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(&known) = seen.iter().find(|&&k| k == key) {
        return known;
    }
    crate::log!(INFO, "world key {key}, from {path}");
    let kept: &'static str = Box::leak(key.into());
    seen.push(kept);
    kept
}

/// A hosted game starts to initialise, before it reads its save: the world
/// paths built from now on are its world's, the menu's before are not.
pub(crate) fn hosting_begins() {
    *KEYS.lock().unwrap_or_else(PoisonError::into_inner) = Keys::OPEN;
}

/// Forgets the key, when a hosted game ends.
pub(crate) fn forget() {
    *KEYS.lock().unwrap_or_else(PoisonError::into_inner) = Keys::CLOSED;
}

/// The world key of a save file path: the file's stem and a hash of its
/// folder, so two worlds of one name in different folders differ.
fn key_of(path: &str) -> Option<String> {
    let path = Path::new(path);
    if path.extension()? != WORLD_EXTENSION {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if stem.is_empty()
        || OTHER_SAVES.iter().any(|other| stem.starts_with(other))
        || !stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let folder = path.parent().map_or(String::new(), |p| {
        p.to_string_lossy().to_lowercase().replace('\\', "/")
    });
    let folder = folder.trim_end_matches('/');
    Some(format!("{stem}-{:08x}", fnv1a32(folder.as_bytes())))
}

/// The key of the world the server runs, once the engine has built its path.
pub(crate) fn world_key(loader: &Loader) -> Option<&'static str> {
    let key = KEYS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pick(loader.side);
    if let Some(key) = key {
        crate::log!(DEBUG, "world key {key}");
    }
    key
}

/// Asks the server to save the world now; `Event::WORLD_SAVED` follows when
/// the save is on disk. A save already under way counts as asked.
pub(crate) fn request(loader: &Loader) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = crate::world::server()?;
    // SAFETY: the server's flag that refuses saves, on its game thread.
    let blocked: u8 = unsafe { crate::memory::read(server, layout.blocked) };
    // SAFETY: the server holds its save system.
    let system: usize = unsafe { crate::memory::read(server, layout.save_system) };
    if blocked != 0 || system == 0 {
        return Err(Status::REFUSED);
    }
    // SAFETY: the save system's phase.
    let phase: u8 = unsafe { crate::memory::read(system, layout.phase) };
    // SAFETY: and its flags.
    let flags: u8 = unsafe { crate::memory::read(system, layout.flags) };
    if phase != 0 {
        return Ok(());
    }
    if flags & SAVE_ALLOWED == 0 {
        return Err(Status::REFUSED);
    }
    // SAFETY: the server, on its game thread between steps, as the server's
    // own loop asks for its timed saves.
    unsafe { (layout.request)(core::ptr::with_exposed_provenance_mut(server)) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_key_is_kept_once() {
        let first = intern("a1b2", "x");
        for _ in 0..3 {
            let _other = intern("c3d4", "y");
            assert!(core::ptr::eq(intern("a1b2", "x"), first));
        }
        let seen = SEEN.lock().unwrap();
        assert_eq!(seen.iter().filter(|k| **k == "a1b2").count(), 1);
    }

    #[test]
    fn a_world_key_names_the_save_and_its_folder() {
        let key = key_of(r"D:\Server\savegame\world0.ksd").unwrap();
        assert!(key.starts_with("world0-"));
        assert_eq!(Some(key.clone()), key_of("d:/server/SAVEGAME/world0.ksd"));
        assert_ne!(Some(key), key_of(r"D:\Other\savegame\world0.ksd"));
        assert_eq!(key_of(r"D:\Server\savegame\world0.ksd.backup-1"), None);
        assert_eq!(key_of(r"D:\Server\savegame\characters"), None);
        assert_eq!(key_of(r"D:\Server\savegame\a b.ksd"), None);
    }

    #[test]
    fn a_server_names_the_world_whose_path_it_built_last() {
        let mut keys = Keys::CLOSED;
        keys.note(Side::SERVER, "world0-a");
        keys.note(Side::SERVER, "world0-b");
        assert_eq!(keys.pick(Side::SERVER), Some("world0-b"));
    }

    #[test]
    fn a_client_names_the_world_its_hosted_game_loads_whatever_thread_built_the_path() {
        let mut keys = Keys::CLOSED;
        keys.note(Side::CLIENT, "world9-menu");
        assert_eq!(keys.pick(Side::CLIENT), None);
        keys = Keys::OPEN;
        assert_eq!(keys.pick(Side::CLIENT), None);
        keys.note(Side::CLIENT, "world1-hosted");
        assert_eq!(keys.pick(Side::CLIENT), Some("world1-hosted"));
        keys = Keys::CLOSED;
        keys.note(Side::CLIENT, "world2-menu");
        assert_eq!(keys.pick(Side::CLIENT), None);
    }

    #[test]
    fn character_and_global_saves_name_no_world() {
        assert_eq!(key_of("character0.ksd"), None);
        assert_eq!(key_of(r"C:\Saves\default\character12.ksd"), None);
        assert_eq!(key_of("global.ksd"), None);
        assert!(key_of("world1.ksd").is_some_and(|k| k.starts_with("world1-")));
    }
}
