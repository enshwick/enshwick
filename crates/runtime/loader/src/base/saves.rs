//! Saves on demand and the world's key, which places its mod folders.
//!
//! A save request sets the flags the server's own 30-second save sets; the
//! server writes the world over the next loop iterations and
//! `Event::WORLD_SAVED` follows. The world key is the path of the world's
//! save file without `.ksd`, which the engine builds whenever it reads or
//! writes it: the loader links the path builder and keeps the world path
//! built last. The world's folders sit beside the save
//! ([`crate::base::world_folders`]).
//!
//! A dedicated server runs one world, so its key is the last one built. A
//! client also builds the paths of all its worlds for its menus, and its
//! save system builds them on a thread of its own, not on the hosted game's
//! (client 410 and 426). There the key is the last one built since the
//! hosted game started to initialise, and none once that game ends. A
//! client names a world it saves to Steam by its file name alone; the
//! loader places it in the running account's Steam cloud folder.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_core::abi::{Event, HookFlags, Side, Status};

use crate::engine::chains::Next;
use crate::engine::loader::{Loader, Target};
use crate::engine::module::Export;
use crate::engine::pinned;
use crate::engine::pinned::Constant;
use enshwick_base::Relock;

mod holds;

pub(crate) use holds::{hold, shut_down, shutting_down};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::save::FLAGS.id(),
    pinned::save::PATH.id(),
    pinned::save::PATH_DATA.id(),
    pinned::save::PATH_LENGTHS.id(),
    pinned::save::PATH_PARTS.id(),
    pinned::save::PHASE.id(),
    pinned::server::SAVE_BLOCKED.id(),
    pinned::server::SAVE_REQUEST.id(),
    pinned::server::SAVE_SYSTEM.id(),
];

/// The file name extension of a world save.
const WORLD_EXTENSION: &str = "ksd";

/// The starts of the names of a client's other save files, which share the
/// world saves' folder and extension: its characters and its global save.
const OTHER_SAVES: &[&str] = &["character", "global"];

/// The folder beside Steam's `local/` user data folder that holds the
/// account's cloud files.
const STEAM_CLOUD_FOLDER: &str = "remote";
/// The last folder of the path Steam's `GetUserDataFolder` names.
const STEAM_LOCAL_FOLDER: &str = "local";

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
// SAFETY: `PathFn` is the signature of the game's `save.path`, and `link`
// uses this slot only as the `next` of `built`, its link on that function.
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

    /// Drops the key when `side` names its world now, by a path that names
    /// no folder.
    fn unplace(&mut self, side: Side) {
        if side == Side::SERVER || self.open {
            self.latest = None;
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
            let constant = |name: Constant| {
                loader
                    .build
                    .constant(name.id())
                    .and_then(|v| usize::try_from(v).ok())
            };
            let request = usize::try_from(
                loader.base + loader.build.symbol(pinned::server::SAVE_REQUEST.id())?,
            )
            .ok()?;
            Some(Layout {
                // SAFETY: `request` is the build table's `server.save_request`
                // in the loaded executable, the game function `RequestFn`
                // types.
                request: unsafe { core::mem::transmute::<usize, RequestFn>(request) },
                save_system: constant(pinned::server::SAVE_SYSTEM)?,
                blocked: constant(pinned::server::SAVE_BLOCKED)?,
                flags: constant(pinned::save::FLAGS)?,
                phase: constant(pinned::save::PHASE)?,
                path_data: constant(pinned::save::PATH_DATA)?,
                path_lengths: constant(pinned::save::PATH_LENGTHS)?,
                path_parts: constant(pinned::save::PATH_PARTS)?,
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
    let target = Target::Pinned(pinned::save::PATH);
    loader.link_feature("world keys", target, &PATH_NEXT, built, HookFlags::NONE);
}

/// The switch of the save path notes.
static SAVES_FUSE: crate::engine::fuse::Fuse = crate::engine::fuse::Fuse::new("save path notes");

/// The loader's link on the game's save path builder: notes the world key
/// of each world save path the game builds.
///
/// # Safety
///
/// - Called only by the game through the chain on `save.path`, with its own
///   result, path and slot arguments.
unsafe extern "system" fn built(result: *mut u8, path: *mut u8, slot: *mut u8) -> *mut u8 {
    // SAFETY: `PATH_NEXT` holds the rest of `save.path`'s chain while this
    // detour runs, and the arguments are the game's own, passed on unchanged.
    let out = unsafe { PATH_NEXT.get()(result, path, slot) };
    let layout = LAYOUT.get().copied().flatten();
    // SAFETY: `result` is non-null, checked first, and is the result the game
    // passed to `save.path`, whose first byte it wrote, 0 on success.
    let code = (!result.is_null()).then(|| unsafe { enshwick_core::raw::read_at::<u8>(result, 0) });
    if let Some(layout) = layout {
        let _caught = SAVES_FUSE.run("the built", || {
            // SAFETY: `path` is the game's own argument to `save.path`, null
            // or the engine path it just built, owned by the caller of this
            // detour on this thread.
            let text = unsafe { read_path(&layout, path) };
            // SAFETY: `GetCurrentThreadId` takes no arguments and cannot fail.
            let thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
            crate::log!(
                DEBUG,
                "save path built ({code:?}) on thread {thread}: {text:?}"
            );
            if code == Some(0)
                && let Some(text) = text
            {
                remember(&text, layout.side, steam_saves);
            }
        });
    }
    out
}

/// The text of an engine path: its buffer and the lengths of its parts.
///
/// # Safety
///
/// - `path` is null or points at an engine path of the running build: its
///   buffer pointer at `save.path_data`, `save.path_parts` lengths from
///   `save.path_lengths`, and a buffer that many bytes long.
/// - No other thread writes the path or its buffer during the call.
unsafe fn read_path(layout: &Layout, path: *mut u8) -> Option<String> {
    if path.is_null() {
        return None;
    }
    let path = path.expose_provenance();
    // SAFETY: `path` is non-null, checked by the guard before it, and is an
    // engine path by this function's `# Safety`, holding its buffer pointer at
    // the table's `save.path_data`.
    let data: usize = unsafe { crate::engine::memory::read(path, layout.path_data) };
    let len = (0..layout.path_parts)
        // SAFETY: `path` is a non-null engine path by this function's
        // `# Safety`, holding `save.path_parts` lengths of eight bytes from
        // the table's `save.path_lengths`.
        .map(|part| unsafe { crate::engine::memory::read::<u64>(path, layout.path_lengths + part * 8) })
        .try_fold(0u64, u64::checked_add)?;
    let len = usize::try_from(len)
        .ok()
        .filter(|&len| len > 0 && len < 0x800)?;
    if data == 0 {
        return None;
    }
    let mut bytes = vec![0u8; len];
    // SAFETY: `data` is non-null, checked by the guard before it, and the
    // engine path's buffer holds the sum of its part lengths, `len`, by this
    // function's `# Safety`.
    unsafe {
        enshwick_core::raw::read_bytes_at(core::ptr::with_exposed_provenance(data), 0, &mut bytes);
    }
    String::from_utf8(bytes).ok()
}

/// Keeps the key of `path` when it names a world save; a key that names
/// the world `side` runs from now on has its folders checked and stops the
/// game when a hold on that world is unmet, before its save is read. A
/// client world in Steam's cloud whose folder `steam` does not find has no
/// key, so no mod writes beside another account's or another world's save.
fn remember(path: &str, side: Side, steam: impl FnOnce() -> Option<&'static Path>) {
    static UNPLACED: crate::base::wire::Drops = crate::base::wire::Drops::new();
    if key_of(path).is_none() {
        return;
    }
    let Some(path) = full_path(path, side, steam) else {
        KEYS.relock().unplace(side);
        if let Some(total) = UNPLACED.add(1) {
            crate::log!(
                WARN,
                "save {path} is in Steam's cloud, whose folder Steam does not name ({total} times), \
                 so mods keep no settings or data for its world"
            );
        }
        return;
    };
    let Some(key) = key_of(&path) else {
        return;
    };
    let key = intern(&key, &path);
    let changed = {
        let mut keys = KEYS.relock();
        let before = keys.pick(side);
        keys.note(side, key);
        let now = keys.pick(side);
        now.is_some() && now != before
    };
    if changed && let Some(loader) = crate::engine::loader::loader() {
        arrive(key);
        crate::base::holds::enforce_world(loader, key);
    }
}

/// The world `key` appears: its folders are set aside when they belong to
/// another world, before anything reads them.
fn arrive(key: &str) {
    use crate::base::world_folders::{Identity, check};
    let stamp = crate::engine::early::stamp(crate::engine::diag::now());
    match check(key, &stamp) {
        Ok(Identity::Renamed { suffix }) => crate::log!(
            WARN,
            "world {key}: the mod settings and data beside its save belonged to another world, \
             so they were renamed with {suffix} and this world starts from the mods' defaults"
        ),
        Ok(Identity::Unreadable) => crate::log!(
            WARN,
            "world {key}: its save has no header the loader reads, so its mod folders are used \
             without checking they belong to it"
        ),
        Ok(_) => {}
        Err(e) => crate::log!(
            WARN,
            "world {key}: its mod folders were not checked against its save: {e}"
        ),
    }
}

/// The world saved: a new world's GUID is recorded once it has a save.
pub(crate) fn saved(loader: &Loader) {
    static UNNOTED: crate::base::wire::Drops = crate::base::wire::Drops::new();
    if let Some(key) = world_key(loader)
        && let Err(e) = crate::base::world_folders::note(key)
        && let Some(total) = UNNOTED.add(1)
    {
        crate::log!(
            WARN,
            "world {key}: its GUID was not recorded ({total} times): {e}"
        );
    }
}

/// Every world key seen, kept once for the process: a menu that builds the
/// paths of several worlds sees each key again and again.
static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// `key`, kept for the process; logged the first time, from `path`.
fn intern(key: &str, path: &str) -> &'static str {
    let mut seen = SEEN.relock();
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
    *KEYS.relock() = Keys::OPEN;
}

/// Forgets the key and every mod's hold on saves, when a hosted game ends.
pub(crate) fn forget() {
    *KEYS.relock() = Keys::CLOSED;
    UNDER_WAY.store(false, Ordering::Relaxed);
    holds::forget();
}

/// The world key of a save file path: the path without `.ksd`, so the
/// world's folders are the key with a suffix.
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
    Some(path.with_extension("").to_str()?.to_owned())
}

/// The full path of the save the game names `path` on `side`; `None` when a
/// client names a save in Steam's cloud and `steam`, the folder of the
/// account's Steam saves, is unknown, or names a relative path that is not
/// a bare file name.
///
/// A client names a world it saves to Steam by its Steam file name alone,
/// `world4.ksd`, and a local world by its full path (client 26430625).
fn full_path(
    path: &str,
    side: Side,
    steam: impl FnOnce() -> Option<&'static Path>,
) -> Option<Cow<'_, str>> {
    let named = Path::new(path);
    if side != Side::CLIENT || named.is_absolute() {
        return Some(Cow::Borrowed(path));
    }
    if named.parent() != Some(Path::new("")) {
        return None;
    }
    steam()?
        .join(path)
        .into_os_string()
        .into_string()
        .ok()
        .map(Cow::Owned)
}

/// The folder where Steam keeps the running account's cloud files of the
/// game, `userdata/<account>/<app>/remote/`; found once Steam has started.
fn steam_saves() -> Option<&'static Path> {
    static FOLDER: OnceLock<PathBuf> = OnceLock::new();
    if let Some(folder) = FOLDER.get() {
        return Some(folder);
    }
    let folder = cloud_folder(&steam_user_data()?)?;
    Some(FOLDER.get_or_init(|| folder))
}

/// The cloud folder beside the account's `local/` folder that
/// `GetUserDataFolder` names; `None` for any other answer. Steam keeps a
/// game's cloud files in `remote/` beside `local/` (client 26430625).
fn cloud_folder(user_data: &Path) -> Option<PathBuf> {
    (user_data.file_name()? == STEAM_LOCAL_FOLDER)
        .then(|| user_data.parent())
        .flatten()
        .map(|account| account.join(STEAM_CLOUD_FOLDER))
}

/// The folder Steam's `GetUserDataFolder` names for the running account,
/// `userdata/<account>/<app>/local/`; `None` before Steam starts.
fn steam_user_data() -> Option<PathBuf> {
    type SteamUser = unsafe extern "C" fn() -> *mut core::ffi::c_void;
    type UserDataFolder =
        unsafe extern "C" fn(this: *mut core::ffi::c_void, buffer: *mut u8, len: i32) -> u8;
    let module = crate::engine::module::Module::loaded("steam_api64.dll")?;
    let user = module.export("SteamAPI_SteamUser_v023")?;
    let folder = module.export("SteamAPI_ISteamUser_GetUserDataFolder")?;
    // SAFETY: the Steamworks flat API declares `SteamAPI_SteamUser_v023` as
    // `SteamUser`.
    let user = unsafe { core::mem::transmute::<Export, SteamUser>(user) };
    // SAFETY: the Steamworks flat API declares
    // `SteamAPI_ISteamUser_GetUserDataFolder` as `UserDataFolder`, its C++
    // `bool` read as a byte.
    let folder = unsafe { core::mem::transmute::<Export, UserDataFolder>(folder) };
    // SAFETY: `user` is the flat API's `SteamAPI_SteamUser_v023`, which takes
    // no arguments and returns null while Steam's API is not initialised,
    // before `SteamAPI_Init` and after `SteamAPI_Shutdown` (`steam_api.h`).
    let this = unsafe { user() };
    if this.is_null() {
        return None;
    }
    let mut buffer = [0u8; 1024];
    // SAFETY: `this` is the non-null user interface the call just before
    // returned on this thread, used at once and not kept; Steam writes at
    // most the 1024 bytes of the local `buffer`, NUL-terminated.
    let found = unsafe { folder(this, buffer.as_mut_ptr(), 1024) };
    if found == 0 {
        return None;
    }
    let text = core::ffi::CStr::from_bytes_until_nul(&buffer)
        .ok()?
        .to_str()
        .ok()?;
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// The key of the world the server runs, once the engine has built its path.
pub(crate) fn world_key(loader: &Loader) -> Option<&'static str> {
    let key = KEYS.relock().pick(loader.side);
    if let Some(key) = key {
        crate::log!(DEBUG, "world key {key}");
    }
    key
}

/// The save's stem in the key `key`, such as `world0`: what mods see.
pub(crate) fn stem(key: &'static str) -> &'static str {
    key.rsplit(['\\', '/']).next().unwrap_or(key)
}

/// Asks the server to save the world now; `Event::WORLD_SAVED` follows when
/// the save is on disk. A save already under way counts as asked.
pub(crate) fn request(loader: &Loader) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = crate::engine::world::server()?;
    // SAFETY: `world::server` returned the live server on its game thread
    // between steps, where nothing else writes it; its byte that refuses
    // saves sits at the table's `server.save_blocked`.
    let blocked: u8 = unsafe { crate::engine::memory::read(server, layout.blocked) };
    // SAFETY: `world::server` returned the live server on its game thread
    // between steps; the pointer to its save system sits at the table's
    // `server.save_system`.
    let system: usize = unsafe { crate::engine::memory::read(server, layout.save_system) };
    if blocked != 0 || system == 0 {
        return Err(Status::REFUSED);
    }
    // SAFETY: `system` is non-zero, checked by the guard before it, and is the
    // live server's save system, read on its game thread; its phase byte sits
    // at the table's `save.phase`.
    let phase: u8 = unsafe { crate::engine::memory::read(system, layout.phase) };
    // SAFETY: `system` is non-zero, checked by the guard before it, and is the
    // live server's save system, read on its game thread; its flags byte sits
    // at the table's `save.flags`.
    let flags: u8 = unsafe { crate::engine::memory::read(system, layout.flags) };
    if phase != 0 {
        return Ok(());
    }
    if flags & SAVE_ALLOWED == 0 {
        return Err(Status::REFUSED);
    }
    // SAFETY: `layout.request` is the table's `server.save_request`, called
    // with the live server on its game thread between steps
    // (`world::server`), as the server's own loop asks for its timed saves.
    unsafe { (layout.request)(core::ptr::with_exposed_provenance_mut(server)) };
    Ok(())
}

/// Whether a save was under way at the last step.
static UNDER_WAY: AtomicBool = AtomicBool::new(false);

/// Whether a save got under way since the last step, the save phase now
/// `phase`, keeping in `under_way` whether one is.
fn began(under_way: &AtomicBool, phase: u8) -> bool {
    let was = under_way.swap(phase != 0, Ordering::Relaxed);
    phase != 0 && !was
}

/// After each step on the server's game thread: raises
/// `Event::WORLD_SAVING` when a save got under way, sets the game's save
/// block while a running mod holds saves, giving the byte back once none
/// does, and warns while saves stay held long.
pub(crate) fn after_step(loader: &Loader) {
    let Some(layout) = layout(loader) else {
        return;
    };
    let Ok(server) = crate::engine::world::server() else {
        return;
    };
    // SAFETY: `world::server` returned the live server on its game thread
    // between steps; the pointer to its save system sits at the table's
    // `server.save_system`.
    let system: usize = unsafe { crate::engine::memory::read(server, layout.save_system) };
    let phase: u8 = if system == 0 {
        0
    } else {
        // SAFETY: `system` is non-zero, checked just before, and is the live
        // server's save system, read on its game thread; its phase byte sits
        // at the table's `save.phase`.
        unsafe { crate::engine::memory::read(system, layout.phase) }
    };
    let started = began(&UNDER_WAY, phase);
    // SAFETY: `server` is `world::server`'s, the live server on its game
    // thread between steps.
    unsafe { holds::after_step(loader, layout, server) };
    if started {
        loader.emit(Event::WORLD_SAVING, None, core::ptr::null());
    }
}

/// The save of the world the server runs, for mods.
pub(crate) fn info(
    loader: &Loader,
) -> Result<Option<crate::base::world_folders::SaveInfo>, Status> {
    static UNREAD: crate::base::wire::Drops = crate::base::wire::Drops::new();
    let key = world_key(loader).ok_or(Status::MISSING)?;
    crate::base::world_folders::save_info(key).map_err(|e| {
        if let Some(total) = UNREAD.add(1) {
            crate::log!(
                WARN,
                "world {key}: its save was not read ({total} times): {e}"
            );
        }
        Status::REFUSED
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_save_begins_once_and_the_next_begins_again() {
        let under_way = AtomicBool::new(false);
        let steps = [0, 2, 3, 0, 0, 2, 0, 1];
        let began: Vec<bool> = steps.iter().map(|&p| began(&under_way, p)).collect();
        assert_eq!(began, [false, true, false, false, false, true, false, true]);
    }

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
    fn a_world_key_is_the_save_path_without_its_extension() {
        let key = key_of(r"D:\Server\savegame\world0.ksd").unwrap();
        assert_eq!(key, r"D:\Server\savegame\world0");
        assert_eq!(stem(Box::leak(key.into_boxed_str())), "world0");
        assert_eq!(stem("world1"), "world1");
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
    fn a_client_world_saved_to_steam_lies_in_the_account_s_steam_saves_folder() {
        let steam = Path::new(r"C:\Steam\userdata\487707326\1203620\remote");
        let path = full_path("world4.ksd", Side::CLIENT, || Some(steam)).unwrap();
        assert_eq!(
            key_of(&path).as_deref(),
            Some(r"C:\Steam\userdata\487707326\1203620\remote\world4")
        );
        assert_eq!(full_path("world4.ksd", Side::CLIENT, || None), None);
        assert_eq!(
            full_path(r"\saves\world4.ksd", Side::CLIENT, || Some(steam)),
            None,
            "a relative path with folders is no Steam file name"
        );
    }

    #[test]
    fn steam_s_cloud_folder_lies_beside_the_local_folder_it_names_and_nowhere_else() {
        assert_eq!(
            cloud_folder(Path::new(r"C:\Steam\userdata\487707326\1203620\local")),
            Some(PathBuf::from(r"C:\Steam\userdata\487707326\1203620\remote"))
        );
        assert_eq!(
            cloud_folder(Path::new(r"C:\Steam\userdata\487707326\1203620")),
            None
        );
        assert_eq!(cloud_folder(Path::new("")), None);
    }

    #[test]
    fn a_hosted_steam_world_without_steam_s_folder_has_no_key_and_never_takes_another() {
        let local = r"C:\Users\a\Saved Games\keen games\Enshrouded\default\world1.ksd";
        let never = || -> Option<&'static Path> { unreachable!("no Steam lookup") };
        hosting_begins();
        remember(local, Side::CLIENT, never);
        assert_eq!(
            KEYS.relock().pick(Side::CLIENT),
            Some(r"C:\Users\a\Saved Games\keen games\Enshrouded\default\world1")
        );
        remember("world4.ksd", Side::CLIENT, || None);
        assert_eq!(
            KEYS.relock().pick(Side::CLIENT),
            None,
            "neither the menu's local world nor world4 beside the game"
        );
        let steam = Path::new(r"C:\Steam\userdata\487707326\1203620\remote");
        remember("world4.ksd", Side::CLIENT, || Some(steam));
        assert_eq!(
            KEYS.relock().pick(Side::CLIENT),
            Some(r"C:\Steam\userdata\487707326\1203620\remote\world4")
        );
        forget();
    }

    #[test]
    fn a_save_named_by_its_full_path_or_on_a_server_lies_where_it_is_named() {
        let local = "C:/Users/a/Saved Games/keen games/Enshrouded/default/world4.ksd";
        let never = || -> Option<&'static Path> { unreachable!("no Steam lookup") };
        assert_eq!(
            full_path(local, Side::CLIENT, never).as_deref(),
            Some(local)
        );
        assert_eq!(
            full_path("savegame/world0.ksd", Side::SERVER, never).as_deref(),
            Some("savegame/world0.ksd")
        );
    }

    #[test]
    fn character_and_global_saves_name_no_world() {
        assert_eq!(key_of("character0.ksd"), None);
        assert_eq!(key_of(r"C:\Saves\default\character12.ksd"), None);
        assert_eq!(key_of("global.ksd"), None);
        assert_eq!(key_of("world1.ksd").as_deref(), Some("world1"));
    }
}
