//! Code patches: bytes of game code replaced at arming, in the same batch as
//! the hooks, and the loader's own patch that lifts the game's limit of 512
//! recipes per workshop.
//!
//! A patch names the bytes it expects and the bytes that replace them, and
//! the hooks it is unsafe without. The loader refuses a patch that overlaps
//! another when it is registered, and at arming skips one whose bytes no
//! longer read as expected, that overlaps the bytes a hook replaces or whose
//! hooks are not armed with it, turning its owner off.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::__private::rules::{patch_fits, spans_overlap as overlaps};
use enshwick::abi::{HookFlags, Status};
use rustc_hash::FxHashMap;

use crate::chains::Next;
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;

use enshwick::content::recipes::WORKSHOP_RECIPES;

/// The build table ids the workshop limit's lift names: the check before a
/// recipe joins its workshop's list, `movzx eax, word [rdx+0x1000]; mov
/// ecx, 0x200; cmp ax, cx; je stop; ...`, the block its `je` reaches, which
/// stops the game on a full list and which the lift replaces whole, the
/// loop's next recipe after the check, and the per-workshop list lookup.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::recipe::LIMIT_CHECK.id(),
    pinned::recipe::LIMIT_NEXT.id(),
    pinned::recipe::LIMIT_STOP.id(),
    pinned::recipe::LIST_LOOKUP.id(),
];
/// The bytes the stop block's replacement needs: `inc r13` and a `jmp`.
const JUMP_LEN: usize = 8;

/// A patch of game code, registered during init.
#[derive(Clone, Debug)]
pub(crate) struct CodePatch {
    /// The owning mod's index, or [`ModIndex::LOADER`].
    pub(crate) owner: ModIndex,
    /// The first byte.
    pub(crate) address: usize,
    /// What the bytes must read at arming.
    pub(crate) expected: Box<[u8]>,
    /// What they become.
    pub(crate) replacement: Box<[u8]>,
    /// The hook targets that must be armed in the same batch: a patch that
    /// only works together with the loader's links stays out without them.
    pub(crate) needs: Box<[u64]>,
}

impl CodePatch {
    fn span(&self) -> (usize, usize) {
        (self.address, self.expected.len())
    }
}

/// What arming does with one patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Written with the hooks.
    Write,
    /// Left out: its owner is off.
    Off,
    /// Left out, and why; its owner is turned off.
    Skip(String),
}

static PENDING: Mutex<Vec<CodePatch>> = Mutex::new(Vec::new());
static SEALED: AtomicBool = AtomicBool::new(false);
static TEXT: OnceLock<Option<(usize, usize)>> = OnceLock::new();

/// The `.text` section in PE `headers`: its offset from the image base and
/// its length.
pub(crate) fn text_section(headers: &[u8]) -> Option<(usize, usize)> {
    let read_u16 = |at: usize| -> Option<usize> {
        Some(usize::from(u16::from_le_bytes(
            headers.get(at..at + 2)?.try_into().ok()?,
        )))
    };
    let read_u32 = |at: usize| -> Option<usize> {
        usize::try_from(u32::from_le_bytes(
            headers.get(at..at + 4)?.try_into().ok()?,
        ))
        .ok()
    };
    let pe = read_u32(0x3c)?;
    if headers.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let sections = read_u16(pe + 6)?;
    let first = pe + 24 + read_u16(pe + 20)?;
    (0..sections).find_map(|i| {
        let section = first + i * 40;
        (headers.get(section..section + 8)? == b".text\0\0\0")
            .then(|| Some((read_u32(section + 12)?, read_u32(section + 8)?)))
            .flatten()
    })
}

/// The game's code section as loaded: its address and length.
pub(crate) fn text(base: u64) -> Option<(usize, usize)> {
    *TEXT.get_or_init(|| {
        let base = usize::try_from(base).ok().filter(|&b| b != 0)?;
        // SAFETY: every caller passes the loader's `base`, the game
        // executable's non-zero image base (checked earlier), where Windows
        // maps its PE headers as one readable page for the process.
        let headers = unsafe { core::slice::from_raw_parts(base as *const u8, 0x1000) };
        let (offset, len) = text_section(headers)?;
        Some((base.checked_add(offset)?, len))
    })
}

/// Where the game's code section starts, from the image base.
fn text_start(base: u64) -> Option<usize> {
    text(base)?.0.checked_sub(usize::try_from(base).ok()?)
}

/// The game's code section as bytes.
pub(crate) fn text_bytes(base: u64) -> Option<&'static [u8]> {
    let (start, len) = text(base)?;
    // SAFETY: `text` found the `.text` section of the game executable's own
    // headers: `len` bytes from `start`, mapped readable for the process.
    Some(unsafe { core::slice::from_raw_parts(start as *const u8, len) })
}

/// The bytes the stub writes over the server's entry point while it holds
/// it, `jmp [rip+0]` and the target, and writes back once released.
pub(crate) const ENTRY_HOLD: usize = 14;

/// The game's code while the loader starts: the copy each mod's load, init
/// and ready is compared against.
#[derive(Debug)]
pub(crate) struct Watching {
    watch: crate::code_watch::Watch,
    /// Where `.text` is mapped.
    start: usize,
    len: usize,
    /// Where `.text` starts, from the image base.
    rva: usize,
}

impl Watching {
    /// Starts from the image on disk when it reads, so changes made before
    /// the loader count too, or else from the code as it is now; `None`
    /// when the code section cannot be found. The entry point's first bytes,
    /// which the stub holds and gives back, are left out.
    pub(crate) fn start(base: u64) -> Option<Self> {
        let (start, len) = text(base)?;
        let rva = text_start(base)?;
        let ignored: Vec<(usize, usize)> = entry_rva(base)
            .and_then(|entry| entry.checked_sub(rva))
            .filter(|&at| at < len)
            .map(|at| (at, ENTRY_HOLD))
            .into_iter()
            .collect();
        let mut watching = Self {
            watch: crate::code_watch::Watch::new(&[], &[]),
            start,
            len,
            rva,
        };
        let now = watching.now();
        watching.watch = disk_text(base, rva, len)
            .and_then(|disk| crate::code_watch::Watch::from_disk(disk, now, &ignored))
            .unwrap_or_else(|| {
                crate::log!(
                    INFO,
                    "game code is compared from now on: the executable on disk does not read as the code in memory"
                );
                crate::code_watch::Watch::new(now, &ignored)
            });
        watching.log_from(0);
        Some(watching)
    }

    /// The game's code as it reads now, for one comparison.
    fn now<'a>(&self) -> &'a [u8] {
        // SAFETY: `start` and `len` are the game executable's `.text`
        // section as `text` found it, mapped readable for the life of the
        // process. Mod code, the stub's entry hold and arming may write these
        // bytes between two calls, so each check builds the slice anew and
        // reads it at once; a byte has no invalid value.
        unsafe { core::slice::from_raw_parts(self.start as *const u8, self.len) }
    }

    /// Compares the code now and puts what changed down to `by` at `stage`.
    pub(crate) fn check(&mut self, by: &str, stage: crate::code_watch::Stage) {
        let before = self.watch.changes().len();
        let now = self.now();
        if self.watch.check(now, by, stage) > 0 {
            self.log_from(before);
        }
    }

    /// Takes the code now as it should be: the loader's hooks and the
    /// patches mods asked for were just written.
    pub(crate) fn absorb(&mut self) {
        let now = self.now();
        self.watch.absorb(now);
    }

    /// Ends the watch and keeps what it found for the conflict report.
    pub(crate) fn finish(self) {
        crate::conflicts::set_code_changes(self.rva, self.watch.into_changes());
    }

    fn log_from(&self, first: usize) {
        for change in &self.watch.changes()[first..] {
            crate::log!(
                WARN,
                "{} changed game code at game+{:#x}, {} bytes, outside the API {}",
                change.who(),
                self.rva + change.offset,
                change.len,
                change.stage.describe()
            );
        }
    }
}

/// The game's entry point, from its mapped PE header, relative to `base`.
pub(crate) fn entry_rva(base: u64) -> Option<usize> {
    let base = usize::try_from(base).ok().filter(|&b| b != 0)?;
    // SAFETY: `base` is the game executable's non-zero image base, where
    // Windows maps its PE headers as one readable page for the process.
    let headers = unsafe { core::slice::from_raw_parts(base as *const u8, 0x1000) };
    let word = |at: usize| -> Option<usize> {
        usize::try_from(u32::from_le_bytes(
            headers.get(at..at + 4)?.try_into().ok()?,
        ))
        .ok()
    };
    let pe = word(0x3c)?;
    // `AddressOfEntryPoint`, 16 bytes into the optional header.
    word(pe + 24 + 16)
}

/// The game's `.text` as the executable on disk holds it, relocated to
/// `base` and `len` bytes long as mapped; `None` when the file does not read
/// or its section starts elsewhere than `rva`.
fn disk_text(base: u64, rva: usize, len: usize) -> Option<Vec<u8>> {
    use enshwick_gamedata::reflect::{Image, Layout};
    let bytes = std::fs::read(std::env::current_exe().ok()?).ok()?;
    let image = Image::parse(&bytes, Layout::File, None).ok()?;
    let section = image.section(".text")?;
    if usize::try_from(section.rva()).ok()? != rva {
        return None;
    }
    let range = image.range(section);
    let raw = range
        .end
        .saturating_sub(range.start)
        .min(u64::try_from(len).ok()?);
    let mut text = image.read(range.start, raw)?.to_vec();
    text.resize(len, 0);
    crate::code_watch::relocate(
        &mut text,
        section.rva(),
        image.relocations(),
        base.wrapping_sub(image.base()),
    );
    Some(text)
}

/// Registers `patch` within `text`; refused once sealed, outside the code,
/// with lengths that differ or are zero, and over another patch.
pub(crate) fn register(patch: CodePatch, text: (usize, usize)) -> Status {
    register_all(vec![patch], text)
}

/// Registers every patch of `patches` within `text`, or none of them, on
/// the terms of [`register`]; patches of one set must not overlap each
/// other either.
pub(crate) fn register_all(patches: Vec<CodePatch>, text: (usize, usize)) -> Status {
    if SEALED.load(Ordering::Acquire) {
        return Status::TOO_LATE;
    }
    if patches
        .iter()
        .any(|p| p.replacement.len() != p.expected.len() || !patch_fits(p.span(), text))
    {
        return Status::BAD_ARGS;
    }
    let mut pending = PENDING.relock();
    let clash = patches.iter().enumerate().find_map(|(i, patch)| {
        pending
            .iter()
            .chain(&patches[..i])
            .find(|p| overlaps(p.span(), patch.span()))
            .map(|held| (held.owner, held.address, patch))
    });
    if let Some((holder, address, patch)) = clash {
        if holder != patch.owner {
            let feature = format!("code patch at {address:#x}");
            crate::conflicts::note(
                feature.clone(),
                holder,
                crate::conflicts::Mode::Exclusive,
                Status::OK,
            );
            crate::conflicts::note(
                feature,
                patch.owner,
                crate::conflicts::Mode::Exclusive,
                Status::REFUSED,
            );
        }
        return Status::REFUSED;
    }
    pending.extend(patches);
    Status::OK
}

/// Ends registration and hands over every patch.
pub(crate) fn seal() -> Vec<CodePatch> {
    SEALED.store(true, Ordering::Release);
    std::mem::take(&mut *PENDING.relock())
}

/// What arming does with each patch, given the bytes the hooks being armed
/// replace, their targets first, and what `current` reads at an address and
/// length now. An owner's patches go in together or not at all: one skipped
/// skips the rest, since a change made of several patches may be unsafe in
/// part.
pub(crate) fn check(
    patches: &[CodePatch],
    hooked: &[(usize, usize)],
    off: impl Fn(ModIndex) -> bool,
    current: impl Fn(usize, usize) -> Option<Vec<u8>>,
) -> Vec<Verdict> {
    let mut verdicts: Vec<Verdict> = patches
        .iter()
        .map(|patch| {
            if off(patch.owner) {
                return Verdict::Off;
            }
            if let Some(&(target, _)) = hooked.iter().find(|&&h| overlaps(h, patch.span())) {
                return Verdict::Skip(format!("it overlaps the hook at {target:#x}"));
            }
            if let Some(target) = patch.needs.iter().find(|&&t| {
                !hooked
                    .iter()
                    .any(|&(at, _)| u64::try_from(at).is_ok_and(|at| at == t))
            }) {
                return Verdict::Skip(format!("the hook at {target:#x} it needs is not armed"));
            }
            match current(patch.address, patch.expected.len()) {
                Some(bytes) if *bytes == *patch.expected => Verdict::Write,
                Some(bytes) => Verdict::Skip(format!(
                    "the bytes read {}, not the expected {}",
                    hex(&bytes),
                    hex(&patch.expected)
                )),
                None => Verdict::Skip("the bytes cannot be read".into()),
            }
        })
        .collect();
    let skipped: Vec<ModIndex> = patches
        .iter()
        .zip(&verdicts)
        .filter(|(_, verdict)| matches!(verdict, Verdict::Skip(_)))
        .map(|(patch, _)| patch.owner)
        .collect();
    for (patch, verdict) in patches.iter().zip(&mut verdicts) {
        if *verdict == Verdict::Write && skipped.contains(&patch.owner) {
            *verdict = Verdict::Skip("another patch of its owner was skipped".into());
        }
    }
    verdicts
}

/// The bytes at `address` now, inside the game's code section only, once
/// it is known.
pub(crate) fn read(address: usize, len: usize) -> Option<Vec<u8>> {
    let (start, size) = (*TEXT.get()?)?;
    let from = address.checked_sub(start)?;
    if from.checked_add(len)? > size {
        return None;
    }
    // SAFETY: the earlier checks keep `len` bytes from `address` inside the
    // game's `.text` section that `text` found, mapped readable for the
    // process.
    let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, len) };
    Some(bytes.to_vec())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where the recipe registry keeps its recipes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    /// `RecipeRegistryResource.recipes`.
    pub(crate) recipes: usize,
    /// `RecipeInfo` size.
    pub(crate) recipe: usize,
    /// `RecipeInfo.workshopId`.
    pub(crate) workshop: usize,
}

/// The loader's lift of the workshop limit, once found.
#[derive(Debug)]
struct Lift {
    /// The patched stop block.
    patch: usize,
    /// The hooked list lookup.
    lookup: u64,
    layout: Layout,
}

static LIFT: OnceLock<Lift> = OnceLock::new();
static LIFTED: AtomicBool = AtomicBool::new(false);
// SAFETY: `LookupFn` is the exact signature of the game's per-workshop list
// lookup the table's `recipe.list_lookup` pins, and the slot is
// used only as the `next` of the `lookup_detour` link on that function.
static LOOKUP_NEXT: Next<LookupFn> = unsafe { Next::new() };
/// Full recipe lists of the workshops past the game's limit, as addresses.
static WORKSHOPS: OnceLock<FxHashMap<u32, Box<[usize]>>> = OnceLock::new();

/// The replacement of the `len` bytes of the stop block at `stop`: `inc r13`
/// (the loop counter the skipped append would have raised), then `jmp` to
/// the next recipe at `next`, the rest `int3`; `None` when the block is too
/// short or the jump too far.
fn stop_replacement(stop: usize, next: usize, len: usize) -> Option<Box<[u8]>> {
    if len < JUMP_LEN {
        return None;
    }
    let from = i64::try_from(stop.checked_add(JUMP_LEN)?).ok()?;
    let rel = i32::try_from(i64::try_from(next).ok()? - from).ok()?;
    let mut bytes = vec![0xcc; len];
    bytes[..4].copy_from_slice(&[0x49, 0xff, 0xc5, 0xe9]);
    bytes[4..JUMP_LEN].copy_from_slice(&rel.to_le_bytes());
    Some(bytes.into())
}

/// The lift of the workshop limit, as the log and the Mods screen name it.
const LIFT_FEATURE: &str = "workshop recipe limit";

/// The per-workshop list lookup, as the log and the Mods screen name it.
const LOOKUP_FEATURE: &str = "recipe list lookup";

/// Why the lift is off, by the table entry it names.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
enum LiftLimitError {
    /// The entry is not in this build's table.
    #[error("{}", crate::builds::ABSENT)]
    NotInBuild { entry: String },
    /// The game's code at the entry is not the table's.
    #[error("does not hold the table's code")]
    CodeDiffers { entry: String },
    /// The entry's address does not fit a pointer.
    #[error("does not fit a pointer")]
    Address {
        entry: String,
        #[source]
        reason: core::num::TryFromIntError,
    },
    /// The table carries no body for the entry.
    #[error("carries no body")]
    NoBody { entry: String },
    /// The stop block is too short for the jump the lift writes.
    #[error("of {len} bytes cannot jump to {next:#x}")]
    TooShort {
        entry: String,
        len: usize,
        next: usize,
    },
    /// The lookup the lift needs is not linked.
    #[error("is not linked")]
    NotLinked { entry: String },
}

impl LiftLimitError {
    /// The table entry, as the log and the Mods screen name it.
    fn entry(&self) -> &str {
        match self {
            Self::NotInBuild { entry }
            | Self::CodeDiffers { entry }
            | Self::Address { entry, .. }
            | Self::NoBody { entry }
            | Self::TooShort { entry, .. }
            | Self::NotLinked { entry } => entry,
        }
    }
}

/// The table's symbol `id` in the running game when its code holds, or
/// why not.
fn checked(loader: &Loader, id: pinned::Symbol) -> Result<usize, LiftLimitError> {
    let entry = || id.id().to_owned();
    let address = loader
        .address(id)
        .ok_or_else(|| LiftLimitError::NotInBuild { entry: entry() })?;
    if !loader.code_holds(id) {
        return Err(LiftLimitError::CodeDiffers { entry: entry() });
    }
    usize::try_from(address).map_err(|reason| LiftLimitError::Address {
        entry: entry(),
        reason,
    })
}

/// The stop block's patch from the build table: the check's code holds, and
/// the block it jumps to holds its whole code, which the patch replaces.
fn limit_patch(loader: &Loader) -> Result<CodePatch, LiftLimitError> {
    let _check = checked(loader, pinned::recipe::LIMIT_CHECK)?;
    let stop = checked(loader, pinned::recipe::LIMIT_STOP)?;
    let next = checked(loader, pinned::recipe::LIMIT_NEXT)?;
    let entry = || pinned::recipe::LIMIT_STOP.id().to_owned();
    let len = loader
        .build
        .body(pinned::recipe::LIMIT_STOP.id())
        .map(enshwick_gamedata::Body::len)
        .ok_or_else(|| LiftLimitError::NoBody { entry: entry() })?;
    let replacement =
        stop_replacement(stop, next, len).ok_or_else(|| LiftLimitError::TooShort {
            entry: entry(),
            len,
            next,
        })?;
    let mut expected = vec![0u8; len];
    // SAFETY: `stop` is the table's `recipe.limit_stop`, whose whole `len`
    // bytes of code `checked` compared against the table's body, inside the
    // game's image, which stays mapped and readable.
    unsafe { crate::memory::read_bytes(stop, 0, &mut expected) };
    Ok(CodePatch {
        owner: ModIndex::LOADER,
        address: stop,
        expected: expected.into(),
        replacement,
        needs: Box::default(),
    })
}

/// Prepares the lift of the workshop limit: the stop block's patch and the
/// lookup hook, both armed with the rest. Called before arming when mods
/// serve the recipe registry.
pub(crate) fn lift_workshop_limit(loader: &'static Loader) {
    let found = (|| {
        use crate::catalogue::ids;
        let missing = |id: &str| LiftLimitError::NotInBuild {
            entry: id.to_owned(),
        };
        let offset = |id: &str| crate::catalogue::offset(loader, id).ok_or_else(|| missing(id));
        let layout = Layout {
            recipes: offset(ids::RECIPE_REGISTRY_RECIPES)?,
            recipe: crate::catalogue::size(loader, ids::RECIPE)
                .ok_or_else(|| missing(ids::RECIPE))?,
            workshop: offset(ids::RECIPE_WORKSHOP)?,
        };
        let patch = limit_patch(loader)?;
        let lookup = lookup_address(loader).ok_or_else(|| LiftLimitError::NotLinked {
            entry: pinned::recipe::LIST_LOOKUP.id().to_owned(),
        })?;
        Ok::<_, LiftLimitError>((layout, patch, lookup))
    })();
    let (layout, patch, lookup) = match found {
        Ok(found) => found,
        Err(off) => {
            let _off = loader.feature_off(LIFT_FEATURE, off.entry(), &off.to_string());
            return;
        }
    };
    let address = patch.address;
    let Some(range) = text(loader.base) else {
        return;
    };
    let patch = CodePatch {
        needs: Box::new([lookup]),
        ..patch
    };
    let status = register(patch, range);
    if !status.is_ok() {
        crate::log!(
            WARN,
            "workshop recipe limit kept: its patch at {address:#x} was refused ({})",
            status.0
        );
        return;
    }
    let _ = LIFT.set(Lift {
        patch: address,
        lookup,
        layout,
    });
}

/// The per-workshop list lookup, linked on first use: the lift of the
/// workshop limit and the disabled recipes both answer there.
static LOOKUP_LINK: OnceLock<Option<u64>> = OnceLock::new();

/// The address of the linked per-workshop list lookup, linking it on the
/// first call; `None` when its code is not found or the link is refused.
fn lookup_address(loader: &'static Loader) -> Option<u64> {
    *LOOKUP_LINK.get_or_init(|| {
        let lookup = match checked(loader, pinned::recipe::LIST_LOOKUP) {
            Ok(at) => at as u64,
            Err(off) => {
                let _off = crate::host::report_off(LOOKUP_FEATURE, lookup_off(loader.build, &off));
                return None;
            }
        };
        let status = loader.link(lookup, &LOOKUP_NEXT, lookup_detour, HookFlags::NONE);
        if status.is_ok() {
            Some(lookup)
        } else {
            crate::log!(
                WARN,
                "workshop recipe list lookup at {lookup:#x} refused ({})",
                status.0
            );
            None
        }
    })
}

/// The warning line and status entry of the lookup turned off on `build`
/// because its entry failed for `off`.
fn lookup_off(build: &crate::builds::KnownBuild, off: &LiftLimitError) -> (String, String) {
    crate::host::off_report(build, LOOKUP_FEATURE, off.entry(), &off.to_string())
}

/// Links the per-workshop list lookup for disabled recipes; whether it is.
pub(crate) fn link_recipe_lookup(loader: &'static Loader) -> bool {
    lookup_address(loader).is_some()
}

/// Settles whether the limit is lifted before arming writes anything:
/// lifted when the stop block's patch is written and the lookup hooked.
pub(crate) fn arming(patches: &[CodePatch], verdicts: &[Verdict], hooked: &[u64]) {
    let lifted = LIFT.get().is_some_and(|lift| {
        hooked.contains(&lift.lookup)
            && patches
                .iter()
                .zip(verdicts)
                .any(|(p, v)| p.address == lift.patch && *v == Verdict::Write)
    });
    LIFTED.store(lifted, Ordering::Release);
    crate::bases::arming(patches, verdicts, hooked);
}

/// Whether the workshop limit is lifted.
pub(crate) fn lifted() -> bool {
    LIFTED.load(Ordering::Acquire)
}

/// The most recipes one workshop takes, as the API reports it.
pub(crate) fn workshop_recipes() -> u32 {
    if lifted() {
        u32::MAX
    } else {
        u32::try_from(WORKSHOP_RECIPES).unwrap_or(u32::MAX)
    }
}

/// Offsets in `registry` of every recipe of each workshop with more than
/// `limit` recipes, in registry order.
pub(crate) fn full_lists(
    registry: &[u8],
    layout: Layout,
    limit: usize,
) -> Option<FxHashMap<u32, Vec<usize>>> {
    let read_u32 = |at: usize| {
        Some(u32::from_le_bytes(
            registry.get(at..at + 4)?.try_into().ok()?,
        ))
    };
    let field = layout.recipes;
    let relative = isize::try_from(read_u32(field)?.cast_signed()).ok()?;
    let count = usize::try_from(read_u32(field + 4)?).ok()?;
    let start = field.checked_add_signed(relative)?;
    let mut lists: FxHashMap<u32, Vec<usize>> = FxHashMap::default();
    for i in 0..count {
        let at = start.checked_add(i.checked_mul(layout.recipe)?)?;
        lists
            .entry(read_u32(at + layout.workshop)?)
            .or_default()
            .push(at);
    }
    lists.retain(|_, list| list.len() > limit);
    Some(lists)
}

/// Notes the recipe registry the game is served, `len` bytes at `data`, and
/// keeps full lists of its crowded workshops for the lookup.
pub(crate) fn recipes_served(data: usize, len: usize) {
    let Some(lift) = LIFT.get().filter(|_| lifted()) else {
        return;
    };
    // SAFETY: the one caller, the content build of the recipe registry,
    // passes the registry it served the game: `len` readable bytes at `data`
    // that live for the process and are not written again.
    let registry = unsafe { core::slice::from_raw_parts(data as *const u8, len) };
    let Some(lists) = full_lists(registry, lift.layout, WORKSHOP_RECIPES) else {
        crate::log!(WARN, "the served recipe registry does not parse");
        return;
    };
    for (workshop, list) in &lists {
        crate::log!(
            INFO,
            "workshop {workshop}: {} recipes, past the game's {WORKSHOP_RECIPES}",
            list.len()
        );
    }
    let lists = lists
        .into_iter()
        .map(|(workshop, list)| (workshop, list.into_iter().map(|at| data + at).collect()))
        .collect();
    if WORKSHOPS.set(lists).is_err() {
        crate::log!(
            WARN,
            "the recipe registry was served again; the first lists stay"
        );
    }
}

type LookupFn = unsafe extern "system" fn(
    out: *mut [u64; 2],
    registry: *mut u8,
    workshop: *const u32,
) -> *mut [u64; 2];

/// The per-workshop list lookup: the game's answer, or the full list of a
/// workshop past the game's limit.
unsafe extern "system" fn lookup_detour(
    out: *mut [u64; 2],
    registry: *mut u8,
    workshop: *const u32,
) -> *mut [u64; 2] {
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the list lookup's signature.
    let result = unsafe { LOOKUP_NEXT.get()(out, registry, workshop) };
    if out.is_null() || workshop.is_null() {
        return result;
    }
    // SAFETY: the game passes the lookup a pointer to the `u32` workshop id,
    // non-null by the earlier check and readable for the call.
    let id: u32 = unsafe { enshwick::raw::read_at(workshop.cast(), 0) };
    if let Some(list) = WORKSHOPS.get().and_then(|lists| lists.get(&id)) {
        // SAFETY: `out` is the lookup's non-null out span (checked earlier),
        // writable for a `[u64; 2]` for the call; the list it now names lives
        // in `WORKSHOPS` for the process.
        unsafe {
            enshwick::raw::write_at(
                out.cast(),
                0,
                [list.as_ptr().expose_provenance() as u64, list.len() as u64],
            );
        };
    }
    let _caught = LOOKUP_FUSE.run("the recipe list lookup", || {
        // SAFETY: `out` is the lookup's non-null out span (checked earlier)
        // for the call, which the game or the lift just filled with the
        // address and length of a list of live registry recipes.
        unsafe { crate::recipes::filter(id, out) };
    });
    result
}

/// The switch of the recipe list lookup's filter.
static LOOKUP_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("disabled recipes");

#[cfg(test)]
mod tests;
