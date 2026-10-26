//! Identifying the running game, its build table and its window.

use std::path::{Path, PathBuf};
use std::time::Duration;

use enshwick::abi::Side;
use enshwick_reflect::{Image, Layout, Reflection};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};

use crate::{builds, log};

use super::pe;

/// How often the client looks for its window.
const WINDOW_POLL: Duration = Duration::from_millis(100);

/// The bytes of the executable's headers the loader reads, one page.
const HEADER_PAGE: usize = 0x400;

/// The folders beside the game's executable the loader keeps.
pub(super) const GAME_DIRS: [&str; 4] = ["mods", "mod-configs", "mod-data", "logs"];

/// The stub contract the loader speaks.
const STUB_ABI: u32 = 2;

/// The game as the loader identified it.
pub(super) struct Game {
    /// Client or server.
    pub(super) side: Side,
    /// Where the executable's image is mapped.
    pub(super) base: u64,
    /// The build's fingerprint from its debug directory.
    pub(super) fingerprint: enshwick_reflect::Fingerprint,
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
            // SAFETY: `found` is the address of the caller's flag.
            unsafe { flag.write(true) };
            return 0;
        }
        1
    }

    let started = std::time::Instant::now();
    while started.elapsed() < limit {
        let mut found = false;
        // SAFETY: the callback only writes the flag, which outlives the call.
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

/// Creates `mods/` and opens the log; `None` when the folder cannot be made.
pub(super) fn begin(stub_abi: u32, missing_forwards: u32) -> Option<PathBuf> {
    let game_dir = game_dir()?;
    let mods_dir = game_dir.join("mods");
    for dir in GAME_DIRS {
        std::fs::create_dir_all(game_dir.join(dir)).ok()?;
    }
    let settings_file = game_dir
        .join("mod-configs")
        .join(format!("{}.toml", crate::settings::LOADER_ID));
    let (settings, problem) = crate::settings::Settings::read(&settings_file);
    crate::status::mark_start();
    crate::diag::configure(&settings);
    crate::reload::configure(settings.dev_reload);
    crate::knowledge::guard::configure(settings.knowledge_guard);
    crate::admission::configure(&settings);
    crate::ui::configure_actions_key(&settings.actions_key);
    builds::configure(settings.build_patterns);
    log::set_levels(settings);
    log::open(&game_dir.join("logs").join(crate::settings::LOADER_ID));
    crate::log!(INFO, "Enshwick {} starting", env!("CARGO_PKG_VERSION"));
    if let Some(problem) = problem {
        crate::log!(WARN, "loader settings left at their defaults: {problem}");
    }
    if stub_abi != STUB_ABI {
        crate::log!(
            WARN,
            "stub contract {stub_abi}, the loader speaks {STUB_ABI}"
        );
    }
    if missing_forwards == u32::MAX {
        crate::log!(WARN, "the system powrprof.dll could not be loaded");
    } else if missing_forwards > 0 {
        crate::log!(
            INFO,
            "{missing_forwards} powrprof.dll exports are missing on this system"
        );
    }
    crate::report::write(&game_dir, stub_abi, None);
    Some(mods_dir)
}

/// Identifies the running game and its build table; `None`, logged, when the
/// loader cannot serve it. On a build it has no table for, a mod's hold under
/// `data_dir` stops the game.
pub(super) fn game(data_dir: &Path) -> Option<Game> {
    let Some((side, base, image_len)) = identify() else {
        crate::log!(
            ERROR,
            "this is not Enshrouded or its dedicated server; nothing loads"
        );
        return None;
    };
    // SAFETY: the game's image stays mapped for the life of the process.
    let bytes: &'static [u8] = unsafe { core::slice::from_raw_parts(base as *const u8, image_len) };
    let Ok(image) = Image::parse(bytes, Layout::Mapped, Some(base)) else {
        crate::log!(ERROR, "the game's image could not be read; nothing loads");
        return None;
    };
    let image: &'static Image<'static> = Box::leak(Box::new(image));
    let Some(fingerprint) = image.fingerprint() else {
        crate::log!(ERROR, "the game build has no fingerprint; nothing loads");
        return None;
    };
    let reflection = Reflection::scan(image);
    crate::log!(
        INFO,
        "{} types and {} systems reflected",
        reflection.type_count(),
        reflection.system_count()
    );
    let tables = builds::load();
    let code = image.section(".text").and_then(|text| {
        let range = image.range(text);
        image
            .read(range.start, range.end - range.start)
            .map(|bytes| (bytes, range.start - base))
    });
    let Some(mut build) = builds::select(&tables, side, fingerprint, code) else {
        crate::log!(
            ERROR,
            "unknown game build {}/{}; the game runs vanilla until Enshwick has its build table",
            hex(&fingerprint.guid()),
            fingerprint.age()
        );
        crate::holds::enforce(data_dir, &[], true);
        return None;
    };
    for (name, rva) in build.checks() {
        let entry = reflection
            .system(name)
            .map_or(0, enshwick_reflect::System::entry);
        if entry != base + rva {
            crate::log!(
                ERROR,
                "{name} is not where the build table says; nothing loads"
            );
            crate::holds::enforce(data_dir, &[], true);
            return None;
        }
    }
    if let Some(game_dir) = data_dir.parent() {
        crate::attribute_slots::supply(&mut build, &reflection, game_dir, side);
    }
    let build: &'static builds::KnownBuild = Box::leak(Box::new(build));
    match build.kind {
        builds::Kind::Exact => crate::log!(INFO, "game: {}", build.name),
        builds::Kind::Fallback => crate::log!(
            WARN,
            "game build {}/{} has no build table yet: {} entries found by their code patterns, {} missing; features and mods that need a missing one stay off until a loader build carries its table",
            hex(&fingerprint.guid()),
            fingerprint.age(),
            build.entries(),
            build.missing().len()
        ),
    }
    if build.kind == builds::Kind::Fallback {
        crate::log!(
            DEBUG,
            "missing on this build: {}",
            build.missing().join(", ")
        );
    }
    Some(Game {
        side,
        base,
        fingerprint,
        build,
        reflection,
    })
}

/// `bytes` as lowercase hex digits, for the log.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _written = write!(out, "{b:02x}");
            out
        })
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
    let base = module.addr();
    // SAFETY: the executable's headers are mapped at its base.
    let header = unsafe { core::slice::from_raw_parts(base as *const u8, HEADER_PAGE) };
    let optional = usize::try_from(pe::u32_at(header, pe::PE_POINTER)?).ok()? + pe::OPTIONAL;
    let len = usize::try_from(pe::u32_at(header, optional + pe::SIZE_OF_IMAGE)?).ok()?;
    Some((side, u64::try_from(base).ok()?, len))
}
