//! Starting the loader on a game executable without running the game.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};

use enshwick_gamedata::reflect::{Image, Layout, Reflection};

use crate::chains::Chains;
use crate::host::{self, Loader};
use crate::resolve::{self, Candidate};
use crate::{builds, log};

use super::game::GAME_DIRS;
use super::library::{Library, Run, libraries};
use super::{initialise, read_frame, records};
use crate::pe;

/// Why a dry run could not start the loader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DryRunError {
    /// The executable is not the game's client or server.
    #[error("{0} is not the game")]
    NotTheGame(String),
    /// A file or folder could not be read or made.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder.
        path: String,
        /// What went wrong.
        source: std::io::Error,
    },
    /// The executable's headers lack a field the mapping needs.
    #[error("the executable has no {0}")]
    MissingHeader(&'static str),
    /// The image cannot be mapped at its preferred base.
    #[error("the image cannot be mapped at {0:#x}")]
    Unmapped(u64),
    /// The executable does not read as a PE image.
    #[error(transparent)]
    Image(#[from] enshwick_gamedata::reflect::ImageError),
    /// The executable has no fingerprint to pick a build table by.
    #[error("the executable has no fingerprint")]
    NoFingerprint,
    /// No build table serves the build.
    #[error("no build table serves this build")]
    NoTable,
    /// The loader already started in this process.
    #[error("the loader already ran in this process")]
    AlreadyRan,
}

/// What the loader would do on a game build: its table and how each mod
/// fared when it started.
#[derive(Debug, Clone)]
pub struct DryRun {
    /// The table's name for the log.
    pub table: String,
    /// Whether the table was made for the build.
    pub exact: bool,
    /// The table's entries the build lacks.
    pub missing: Vec<String>,
    /// Every mod in `mods/`, in load order, then the skipped ones.
    pub mods: Vec<DryMod>,
    /// Every line the loader and the mods logged.
    pub lines: Vec<(enshwick::abi::Level, String, String)>,
}

/// How one mod fared in a [`DryRun`].
#[derive(Debug, Clone)]
pub struct DryMod {
    /// The DLL's file name.
    pub file: String,
    /// The mod id; empty when its manifest did not read.
    pub id: String,
    /// The mod's version.
    pub version: String,
    /// Whether it started and stayed on.
    pub running: bool,
    /// Why not, when it did not.
    pub reason: String,
}

/// Starts the loader on the game executable `exe` without running the game.
///
/// The executable's image is mapped at its preferred base, the build table
/// taken as the loader takes it (only by patterns with `patterns`), and every
/// mod in `game_dir/mods/` resolved and initialised as at a start.
///
/// Nothing is hooked or patched, and no mod's `ready` runs. Once per process:
/// the loader keeps its state for the process.
///
/// # Errors
///
/// When the executable is not the game, cannot be mapped, or no table
/// serves it.
pub fn dry_run(exe: &Path, game_dir: &Path, patterns: bool) -> Result<DryRun, DryRunError> {
    let side = exe
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(pe::side_of)
        .ok_or_else(|| DryRunError::NotTheGame(exe.display().to_string()))?;
    let io = |path: &Path| {
        let path = path.display().to_string();
        move |source| DryRunError::Io { path, source }
    };
    log::capture();
    let bytes = std::fs::read(exe).map_err(io(exe))?;
    let (base, mapped) = map_image(&bytes)?;
    let image = Image::parse(mapped, Layout::Mapped, Some(base))?;
    let image: &'static Image<'static> = Box::leak(Box::new(image));
    let fingerprint = image.fingerprint().ok_or(DryRunError::NoFingerprint)?;
    let reflection = Reflection::scan(image);
    let mods_dir = game_dir.join("mods");
    let tables = builds::load();
    let code = image.section(".text").and_then(|text| {
        let range = image.range(text);
        image
            .read(range.start, range.end - range.start)
            .map(|bytes| (bytes, range.start - base))
    });
    let mut build = if patterns {
        code.and_then(|code| builds::by_patterns(&tables, side, code))
    } else {
        builds::select(&tables, side, fingerprint, code)
    }
    .ok_or(DryRunError::NoTable)?;
    if let Some(exe_dir) = exe.parent() {
        crate::attribute_slots::supply(&mut build, &reflection, exe_dir, side);
    }
    let build: &'static builds::KnownBuild = Box::leak(Box::new(build));
    for dir in GAME_DIRS {
        let dir = game_dir.join(dir);
        std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    }
    let data_dir = game_dir.join("mod-data");
    let (libraries, candidates, others) = libraries(&mods_dir, Run::Dry);
    let plan = resolve::resolve(&candidates, side);
    let loader = Loader {
        side,
        base,
        fingerprint,
        build,
        reflection,
        configs_dir: game_dir.join("mod-configs"),
        data_dir,
        mods_dir,
        mods: records(&libraries, &candidates, &plan),
        others,
        chains: Mutex::new(Chains::default()),
        mids: Mutex::default(),
        caller_frame: read_frame,
        subscriptions: Mutex::default(),
        delivery: OnceLock::new(),
    };
    let loader = host::install(loader).ok_or(DryRunError::AlreadyRan)?;
    let order: Vec<&Library> = plan.order.iter().map(|&i| &libraries[i]).collect();
    initialise(loader, &order);
    let lines = log::captured();
    let mods = outcomes(loader, &order, &libraries, &candidates, &plan, &lines);
    Ok(DryRun {
        table: build.name.clone(),
        exact: build.kind == builds::Kind::Exact,
        missing: build.missing().to_vec(),
        mods,
        lines,
    })
}

/// How each mod fared, in load order, then the skipped ones, with the
/// reason a mod that is off gave in `lines`.
fn outcomes(
    loader: &Loader,
    order: &[&Library],
    libraries: &[Library],
    candidates: &[Candidate],
    plan: &resolve::Plan,
    lines: &[(enshwick::abi::Level, String, String)],
) -> Vec<DryMod> {
    let reason_of = |id: &str| {
        lines
            .iter()
            .filter(|(_, source, _)| source == id)
            .find_map(|(_, _, text)| text.strip_prefix("off: "))
            .unwrap_or_default()
            .to_owned()
    };
    let mut mods: Vec<DryMod> = loader
        .mods
        .iter()
        .zip(order)
        .map(|(record, library)| {
            let running = !record.off.load(Ordering::Acquire);
            DryMod {
                file: library.file.clone(),
                id: record.id.clone(),
                version: record.version.clone(),
                running,
                reason: if running {
                    String::new()
                } else {
                    reason_of(&record.id)
                },
            }
        })
        .collect();
    mods.extend(plan.skipped.iter().map(|(index, reason)| DryMod {
        file: libraries[*index].file.clone(),
        id: candidates[*index].id.clone(),
        version: candidates[*index].version.to_string(),
        running: false,
        reason: format!("skipped: {reason}"),
    }));
    mods
}

/// Maps the executable `bytes` at its preferred base as Windows would, its
/// sections where their headers put them, without relocations or imports;
/// the base and the image, which live for the process.
fn map_image(bytes: &[u8]) -> Result<(u64, &'static [u8]), DryRunError> {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc,
    };

    let headers = pe::Headers::read(bytes).map_err(DryRunError::MissingHeader)?;
    let at = usize::try_from(headers.base).map_err(|_| DryRunError::Unmapped(headers.base))?;
    // SAFETY: asks for fresh pages at the image's preferred base; the result
    // is checked before use.
    let memory = unsafe {
        VirtualAlloc(
            core::ptr::with_exposed_provenance(at),
            headers.size,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_READWRITE,
        )
    };
    if memory.is_null() || memory.addr() != at {
        return Err(DryRunError::Unmapped(headers.base));
    }
    // SAFETY: `VirtualAlloc` committed `headers.size` bytes at `memory`, which
    // nothing else references and which stay ours for the process.
    let image = unsafe { core::slice::from_raw_parts_mut(memory.cast::<u8>(), headers.size) };
    headers.copy(bytes, image);
    Ok((headers.base, image))
}
