//! Saves on demand and the world's key for mods' per-world data.
//!
//! A save request sets the flags the server's own 30-second save sets; the
//! server writes the world over the next loop iterations and
//! `Event::WORLD_SAVED` follows. The world key comes from the path of the
//! world's save file, which the engine builds whenever it reads or writes
//! it: the loader links the path builder and keeps the last world path.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{HookFlags, Status};
use enshwick_loader_reflect::fnv1a32;

use crate::chains::{LOADER, Link};
use crate::host::Loader;

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
static PATH_NEXT: AtomicUsize = AtomicUsize::new(0);
/// The key of the world whose save path the engine built last.
static KEY: Mutex<Option<&'static str>> = Mutex::new(None);

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
    let (Some(_), Some(rva)) = (layout(loader), loader.build.symbol("save.path")) else {
        crate::log!(WARN, "world keys off: this build has no save layout");
        return;
    };
    let link = Link {
        owner: LOADER,
        detour: (built as *const ()).addr(),
        next: &PATH_NEXT,
        priority: i32::MIN,
        flags: HookFlags::NONE,
    };
    let status = loader.chains.lock().map_or(Status::REFUSED, |mut chains| {
        chains.add(loader.base + rva, link)
    });
    if !status.is_ok() {
        crate::log!(WARN, "world keys off: refused ({})", status.0);
    }
}

unsafe extern "system" fn built(result: *mut u8, path: *mut u8, slot: *mut u8) -> *mut u8 {
    let next = PATH_NEXT.load(Ordering::Acquire);
    // SAFETY: the loader points the slot at the rest of the chain, which has
    // the hooked function's signature, before the link can run.
    let next = unsafe { core::mem::transmute::<usize, PathFn>(next) };
    // SAFETY: forwarded unchanged.
    let out = unsafe { next(result, path, slot) };
    let layout = LAYOUT.get().copied().flatten();
    // SAFETY: the game wrote its result's first byte, 0 on success.
    let code = (!result.is_null()).then(|| unsafe { result.read() });
    if let Some(layout) = layout {
        let _caught = std::panic::catch_unwind(|| {
            // SAFETY: the path the game has just built.
            let text = unsafe { read_path(&layout, path) };
            crate::log!(DEBUG, "save path built ({code:?}): {text:?}");
            if code == Some(0)
                && let Some(text) = text
            {
                remember(&text);
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

/// Keeps the key of `path` when it names a world save.
fn remember(path: &str) {
    let Some(key) = key_of(path) else {
        return;
    };
    let mut held = KEY.lock().unwrap_or_else(PoisonError::into_inner);
    if held.is_none_or(|old| old != key) {
        crate::log!(INFO, "world key {key}, from {path}");
        *held = Some(Box::leak(key.into_boxed_str()));
    }
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
pub(crate) fn world_key() -> Option<&'static str> {
    *KEY.lock().unwrap_or_else(PoisonError::into_inner)
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
}
