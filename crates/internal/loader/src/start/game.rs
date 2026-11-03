//! Identifying the running game, its build table and its window.

use std::path::{Path, PathBuf};
use std::time::Duration;

use enshwick::abi::Side;
use enshwick_gamedata::reflect::{Fingerprint, Image, Layout, Reflection};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};

use crate::{builds, errors, log};

use crate::pe;

/// How often the client looks for its window.
const WINDOW_POLL: Duration = Duration::from_millis(100);

/// The bytes of the executable's headers the loader reads, one page.
const HEADER_PAGE: usize = 0x400; // bound, not a game offset: the loader's read

/// The folders beside the game's executable the loader keeps.
pub(super) const GAME_DIRS: [&str; 3] = ["mods", "mod-configs", "logs"];

/// The stub contract the loader speaks.
const STUB_ABI: u32 = 2;

/// The game as the loader identified it.
pub(super) struct Game {
    /// Client or server.
    pub(super) side: Side,
    /// Where the executable's image is mapped.
    pub(super) base: u64,
    /// The build's fingerprint from its debug directory.
    pub(super) fingerprint: Fingerprint,
    /// The build table that serves it.
    pub(super) build: &'static builds::KnownBuild,
    /// Its reflected types and systems.
    pub(super) reflection: Reflection<'static>,
}

/// Waits until this process shows a window or `limit` passes; how long it
/// waited.
pub(super) fn wait_for_window(limit: Duration) -> Duration {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    /// # Safety
    ///
    /// - `EnumWindows` calls it with a window handle valid for the call.
    /// - `found` is the exposed address of a writable `bool` alive for the
    ///   whole enumeration.
    unsafe extern "system" fn ours(window: HWND, found: LPARAM) -> i32 {
        let mut process = 0u32;
        // SAFETY: a window Windows enumerates; the out word is ours.
        unsafe { GetWindowThreadProcessId(window, &raw mut process) };
        // SAFETY: `window` is a handle `EnumWindows` passes for the call.
        let visible = unsafe { IsWindowVisible(window) } != 0;
        // SAFETY: no preconditions.
        let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
        if visible && process == me {
            let flag = core::ptr::with_exposed_provenance_mut::<bool>(found.cast_unsigned());
            // SAFETY: `found` is the exposed address of the `found` flag that
            // `wait_for_window` passed `EnumWindows`, alive for the whole
            // enumeration.
            unsafe { flag.write(true) };
            return 0;
        }
        1
    }

    let started = std::time::Instant::now();
    while started.elapsed() < limit {
        let mut found = false;
        // SAFETY: `ours` matches `WNDENUMPROC` and writes only the flag whose
        // address goes in as `LPARAM`, a local that outlives the call.
        unsafe {
            EnumWindows(
                Some(ours),
                (&raw mut found).expose_provenance().cast_signed(),
            )
        };
        if found {
            break;
        }
        std::thread::sleep(WINDOW_POLL);
    }
    started.elapsed()
}

/// What the start has once the log is open.
pub(super) struct Begun {
    /// The `mods` folder.
    pub(super) mods_dir: PathBuf,
    /// What was wrong in the loader settings file, a line each.
    pub(super) settings_problems: Vec<String>,
    /// Whether developer tools are on, by the file read once at start.
    pub(super) developer_tools: bool,
}

/// Creates `mods/`, reads the loader settings and opens the log; `None`
/// when the folders cannot be made.
pub(super) fn begin(stub_abi: u32, missing_forwards: u32) -> Option<Begun> {
    crate::early::log_panics();
    let Some(game_dir) = game_dir() else {
        crate::early::note("the game's folder was not found; nothing loads");
        return None;
    };
    let mods_dir = game_dir.join("mods");
    for dir in GAME_DIRS {
        let dir = game_dir.join(dir);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            crate::early::note(&format!(
                "{} could not be made ({e}); nothing loads. Move the game to a folder \
                 Windows lets it write to, or run it with the rights to write there",
                dir.display()
            ));
            return None;
        }
    }
    let (settings, settings_problems) =
        crate::settings::Settings::read(&game_dir.join(crate::settings::FILE));
    crate::status::mark_start();
    crate::diag::configure(&settings);
    let developer_tools = crate::developer_tools::marked(&game_dir);
    builds::configure(settings.build_patterns);
    log::set_levels(settings);
    log::open(&game_dir.join(log::LOGS));
    crate::log!(
        INFO,
        "Enshwick {} starting on {}, {} logical processors",
        env!("CARGO_PKG_VERSION"),
        system(),
        std::thread::available_parallelism().map_or(0, std::num::NonZero::get)
    );
    if developer_tools {
        crate::log!(
            WARN,
            "developer tools are on: {} exists; never on a game players join",
            enshwick::mods::DEVELOPER_TOOLS
        );
    }
    let applies = if settings_problems.unread {
        "the defaults apply"
    } else {
        "the rest of the file applies"
    };
    for problem in &settings_problems.lines {
        crate::log!(
            ERROR,
            "loader settings: {problem}; {applies} ({})",
            errors::BAD_SETTINGS
        );
    }
    if stub_abi != STUB_ABI {
        crate::log!(
            WARN,
            "stub contract {stub_abi}, the loader speaks {STUB_ABI}"
        );
    }
    if missing_forwards == u32::MAX {
        crate::log!(
            WARN,
            "the system powrprof.dll could not be loaded ({})",
            errors::NO_SYSTEM_POWRPROF
        );
    } else if missing_forwards > 0 {
        crate::log!(
            INFO,
            "{missing_forwards} powrprof.dll exports are missing on this system"
        );
    }
    Some(Begun {
        mods_dir,
        settings_problems: settings_problems.lines,
        developer_tools,
    })
}

/// Windows, or Wine with its version when the process runs under Wine.
fn system() -> String {
    use windows_sys::Win32::System::LibraryLoader::GetProcAddress;
    type WineVersion = unsafe extern "C" fn() -> *const core::ffi::c_char;
    // SAFETY: a NUL-terminated name; ntdll is loaded in every process.
    let module = unsafe { GetModuleHandleW(windows_sys::core::w!("ntdll.dll")) };
    if module.is_null() {
        return "Windows".to_owned();
    }
    // SAFETY: `module` is loaded and the name is NUL-terminated.
    let Some(found) = (unsafe { GetProcAddress(module, c"wine_get_version".as_ptr().cast()) })
    else {
        return "Windows".to_owned();
    };
    // SAFETY: Wine's ntdll (dlls/ntdll) defines it as
    // `const char * CDECL wine_get_version(void)`; CDECL is the C calling
    // convention on x64, which `WineVersion` names.
    let version =
        unsafe { core::mem::transmute::<unsafe extern "system" fn() -> isize, WineVersion>(found) };
    // SAFETY: the function takes no arguments, touches no thread state and
    // only returns the address of a static string, on any thread.
    let text = unsafe { version() };
    if text.is_null() {
        return "Wine".to_owned();
    }
    // SAFETY: a static NUL-terminated string from Wine, valid for the process.
    let text = unsafe { core::ffi::CStr::from_ptr(text) };
    format!("Wine {}", text.to_string_lossy())
}

/// Identifies the running game and its build table; `None`, logged, when the
/// loader cannot serve it. On a build it has no table for, a mod's hold under
/// `mods_dir` stops the game.
pub(super) fn game(mods_dir: &Path) -> Option<Game> {
    let Some((side, base, image_len)) = identify() else {
        crate::log!(
            ERROR,
            "this is not Enshrouded or its dedicated server; nothing loads ({})",
            errors::NOT_THE_GAME
        );
        return None;
    };
    // SAFETY: `identify` found `image_len` bytes of the game's image at
    // `base`, whose provenance it exposed, mapped for the life of the process.
    let bytes: &'static [u8] = unsafe { core::slice::from_raw_parts(base as *const u8, image_len) };
    let Ok(image) = Image::parse(bytes, Layout::Mapped, Some(base)) else {
        crate::log!(
            ERROR,
            "the game's image could not be read; nothing loads ({})",
            errors::UNREADABLE_GAME
        );
        return None;
    };
    let image: &'static Image<'static> = Box::leak(Box::new(image));
    let Some(fingerprint) = image.fingerprint() else {
        crate::log!(
            ERROR,
            "the game build has no fingerprint; nothing loads ({})",
            errors::UNREADABLE_GAME
        );
        return None;
    };
    let reflection = Reflection::scan(image);
    if !reflection_read(&reflection, fingerprint, mods_dir) {
        return None;
    }
    let tables = builds::load();
    let code = image.section(".text").and_then(|text| {
        let range = image.range(text);
        image
            .read(range.start, range.end - range.start)
            .map(|bytes| (bytes, range.start - base))
    });
    let cell = |rva| image.u32(base + rva);
    let Some(mut build) = builds::select(&tables, side, fingerprint, code, cell) else {
        crate::log!(
            ERROR,
            "unknown game build {}; the game runs vanilla until Enshwick has its build table ({})",
            builds::fingerprint_name(fingerprint),
            errors::UNKNOWN_BUILD
        );
        crate::holds::enforce(mods_dir, &[], true, None);
        return None;
    };
    for (name, rva) in build.checks() {
        let entry = reflection
            .system(name)
            .map_or(0, enshwick_gamedata::reflect::System::entry);
        if entry != base + rva {
            crate::log!(
                ERROR,
                "{name} is not where the build table says; nothing loads ({})",
                errors::TABLE_MISMATCH
            );
            crate::holds::enforce(mods_dir, &[], true, None);
            return None;
        }
    }
    if let Some(game_dir) = mods_dir.parent() {
        crate::attribute_slots::supply(&mut build, &reflection, game_dir, side);
    }
    route(&mut build, image, &reflection, side);
    let build: &'static builds::KnownBuild = Box::leak(Box::new(build));
    match build.kind {
        builds::Kind::Exact => crate::log!(INFO, "game: {}", build.name),
        builds::Kind::Fallback => crate::log!(
            WARN,
            "game build {} has no build table yet: {} entries found by engine routes or their code patterns, {} missing; features and mods that need a missing one stay off until a loader build carries its table ({})",
            builds::fingerprint_name(fingerprint),
            build.entries(),
            build.missing().len(),
            errors::FEATURES_OFF
        ),
    }
    if build.kind == builds::Kind::Fallback {
        crate::log!(
            DEBUG,
            "missing on this build: {}",
            build.missing().join(", ")
        );
    }
    warn_lacking(build);
    Some(Game {
        side,
        base,
        fingerprint,
        build,
        reflection,
    })
}

/// Takes the entries the engine routes find on this build into `build`.
fn route(
    build: &mut builds::KnownBuild,
    image: &Image<'_>,
    reflection: &Reflection<'_>,
    side: Side,
) {
    let start = std::time::Instant::now();
    let routes = match enshwick_gamedata::Routes::embedded() {
        Ok(routes) => routes,
        Err(e) => {
            crate::log!(ERROR, "embedded engine routes: {e}");
            return;
        }
    };
    let read = start.elapsed();
    let routing = std::time::Instant::now();
    let found = build.route(&routes, image, reflection, side);
    crate::log!(
        INFO,
        "engine routes read in {} ms, found {found} entries in {} ms",
        read.as_millis(),
        routing.elapsed().as_millis()
    );
}

/// Warns about each loader feature `build` serves only partly or not at all.
fn warn_lacking(build: &builds::KnownBuild) {
    for part in build.parts(crate::TABLE_IDS) {
        if let Some(line) = crate::diag::part_line(&part) {
            crate::log!(
                WARN,
                "loader feature {line}; the game runs it vanilla ({})",
                errors::FEATURES_OFF
            );
        }
    }
}

/// The path of the game's executable.
fn exe_path() -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    #[expect(clippy::cast_possible_truncation, reason = "the buffer is 1024 units")]
    // SAFETY: the buffer and its length match; a null module is the executable.
    let len = unsafe {
        GetModuleFileNameW(
            core::ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    if len == 0 || len >= buffer.len() {
        return None;
    }
    String::from_utf16(&buffer[..len]).ok()
}

/// The folder of the game's executable.
fn game_dir() -> Option<PathBuf> {
    PathBuf::from(exe_path()?).parent().map(Path::to_path_buf)
}

/// The side, image base and image size of the running game.
pub(super) fn identify() -> Option<(Side, u64, usize)> {
    let exe = exe_path()?;
    let side = pe::side_of(exe.rsplit('\\').next()?)?;
    // SAFETY: a null name is the executable's module.
    let module = unsafe { GetModuleHandleW(core::ptr::null()) };
    if module.is_null() {
        return None;
    }
    let base = module.expose_provenance();
    // SAFETY: `module` is the executable's base, non-null as checked just
    // before, where Windows maps its read-only header page of `HEADER_PAGE`
    // bytes for the life of the process.
    let header = unsafe { core::slice::from_raw_parts(module.cast::<u8>(), HEADER_PAGE) };
    let optional = usize::try_from(pe::u32_at(header, pe::PE_POINTER)?).ok()? + pe::OPTIONAL;
    let len = usize::try_from(pe::u32_at(header, optional + pe::SIZE_OF_IMAGE)?).ok()?;
    Some((side, u64::try_from(base).ok()?, len))
}

/// Logs what the scan reflected, and whether it read the game: a build whose
/// scan finds no types or no systems is logged as unknown, and a mod's hold
/// under `mods_dir` stops the game.
fn reflection_read(reflection: &Reflection<'_>, fingerprint: Fingerprint, mods_dir: &Path) -> bool {
    crate::log!(
        INFO,
        "{} types and {} systems reflected",
        reflection.type_count(),
        reflection.system_count()
    );
    if builds::reflection_read(reflection.type_count(), reflection.system_count()) {
        return true;
    }
    crate::log!(
        ERROR,
        "game build {} reflects no types or no systems: its reflection format is new; the game runs vanilla until Enshwick reads it ({})",
        builds::fingerprint_name(fingerprint),
        errors::UNKNOWN_BUILD
    );
    crate::holds::enforce(mods_dir, &[], true, None);
    false
}
