//! Saves on demand and the world's key for mods' per-world data.
//!
//! A save request sets the flags the server's own 30-second save sets; the
//! server writes the world over the next loop iterations and
//! `Event::WORLD_SAVED` follows. The world key comes from the path of the
//! world's save file, which the engine builds whenever it reads or writes
//! it: the loader links the path builder and keeps the world paths each
//! thread built last.
//!
//! A dedicated server runs one world, so its key is the last one built. A
//! client also builds the paths of its own worlds, for its menus, so there
//! the key is the one built on the thread that runs the hosted game, and
//! none once that game ends.

use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{HookFlags, Side, Status};
use enshwick_loader_reflect::fnv1a32;

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
}

/// `flags` bit: the world may be saved now, set once it has loaded.
const SAVE_ALLOWED: u8 = 1 << 2;

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
// SAFETY: each slot joins the chain of the function its type names.
static PATH_NEXT: Next<PathFn> = unsafe { Next::new() };
/// How many threads' keys are kept.
const KEPT: usize = 8;

/// The key of the world whose save path each thread built last, by thread
/// id, the latest last.
static KEYS: Mutex<Vec<(u32, &'static str)>> = Mutex::new(Vec::new());

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
            crate::log!(DEBUG, "save path built ({code:?}): {text:?}");
            if code == Some(0)
                && let Some(text) = text
            {
                // SAFETY: no preconditions.
                let thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
                remember(&text, thread);
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
    let data: usize = unsafe { crate::world::read(path, layout.path_data) };
    let len = (0..layout.path_parts)
        // SAFETY: the path holds the lengths of its parts one after another.
        .map(|part| unsafe { crate::world::read::<u64>(path, layout.path_lengths + part * 8) })
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

/// Keeps the key of `path`, built on `thread`, when it names a world save.
fn remember(path: &str, thread: u32) {
    let Some(key) = key_of(path) else {
        return;
    };
    let mut keys = KEYS.lock().unwrap_or_else(PoisonError::into_inner);
    let known = keys.iter().find(|&&(_, k)| k == key).map(|&(_, k)| k);
    let key = known.unwrap_or_else(|| {
        crate::log!(INFO, "world key {key}, from {path}");
        Box::leak(key.into_boxed_str())
    });
    keep(&mut keys, thread, key);
}

/// Makes `key` the latest of `thread` in `keys`, keeping at most [`KEPT`].
fn keep(keys: &mut Vec<(u32, &'static str)>, thread: u32, key: &'static str) {
    keys.retain(|&(t, _)| t != thread);
    if keys.len() == KEPT {
        keys.remove(0);
    }
    keys.push((thread, key));
}

/// The key a side reads from `keys`: a server's latest, and a client's
/// from the thread of the game it hosts, `hosted` once its first step ran
/// and else the thread that asks.
fn pick(
    keys: &[(u32, &'static str)],
    side: Side,
    hosted: Option<u32>,
    me: u32,
) -> Option<&'static str> {
    if side == Side::SERVER {
        return keys.last().map(|&(_, key)| key);
    }
    let thread = hosted.unwrap_or(me);
    keys.iter()
        .rev()
        .find(|&&(t, _)| t == thread)
        .map(|&(_, key)| key)
}

/// Forgets every key, when a hosted game ends.
pub(crate) fn forget() {
    KEYS.lock().unwrap_or_else(PoisonError::into_inner).clear();
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
    // SAFETY: no preconditions.
    let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    let keys = KEYS.lock().unwrap_or_else(PoisonError::into_inner);
    let key = pick(&keys, loader.side, crate::events::hosted_thread(), me);
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
    let blocked: u8 = unsafe { crate::world::read(server, layout.blocked) };
    // SAFETY: the server holds its save system.
    let system: usize = unsafe { crate::world::read(server, layout.save_system) };
    if blocked != 0 || system == 0 {
        return Err(Status::REFUSED);
    }
    // SAFETY: the save system's phase.
    let phase: u8 = unsafe { crate::world::read(system, layout.phase) };
    // SAFETY: and its flags.
    let flags: u8 = unsafe { crate::world::read(system, layout.flags) };
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
    fn a_client_takes_the_key_of_the_hosted_game_s_thread_and_a_server_the_latest() {
        let mut keys = Vec::new();
        keep(&mut keys, 1, "hosted");
        keep(&mut keys, 2, "menu");
        assert_eq!(pick(&keys, Side::SERVER, None, 9), Some("menu"));
        assert_eq!(pick(&keys, Side::CLIENT, Some(1), 2), Some("hosted"));
        assert_eq!(pick(&keys, Side::CLIENT, None, 1), Some("hosted"));
        assert_eq!(pick(&keys, Side::CLIENT, Some(3), 2), None);
        keep(&mut keys, 1, "next");
        assert_eq!(pick(&keys, Side::CLIENT, Some(1), 2), Some("next"));
        for thread in 10..20 {
            keep(&mut keys, thread, "other");
        }
        assert_eq!(keys.len(), KEPT);
        assert_eq!(pick(&keys, Side::CLIENT, Some(1), 2), None);
    }
}
