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

use enshwick::abi::{HookFlags, Status};
use enshwick::code::Pattern;
use rustc_hash::FxHashMap;

use crate::chains::Next;
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// The game's own number of recipes per workshop.
const WORKSHOP_RECIPES: usize = 512;

/// The check before a recipe joins its workshop's list, the same on server
/// and client: `movzx eax, word [rdx+0x1000]; mov ecx, 0x200; cmp ax, cx;
/// je stop; mov [rdx+rax*8], rdi; inc r13`.
const LIMIT_SITE: &str =
    "0F B7 82 00 10 00 00 B9 00 02 00 00 66 3B C1 0F 84 0E 01 00 00 48 89 3C C2 49 FF C5";
/// The block that stops the game on a full list, from the check.
const STOP: usize = 0x123;
/// Its length.
const STOP_LEN: usize = 27;
/// How the block starts: the message's arguments.
const STOP_START: &str = "4C 8D 05 ?? ?? ?? ?? 33 C9 48 8D 15";
/// The next recipe of the loop, from the check.
const NEXT_RECIPE: usize = 0x23;
/// The per-workshop list lookup's first bytes, the same on server and client.
const LOOKUP: &str = "48 89 5C 24 08 48 89 74 24 10 48 89 7C 24 18 83 BA 4C 01 00 00 00 4C 8B D2 48 8B BA 08 01 00 00";

/// A patch of game code, registered during init.
#[derive(Debug)]
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

/// Whether two byte ranges, as start and length, share a byte.
fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.0.saturating_add(b.1) && b.0 < a.0.saturating_add(a.1)
}

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

/// The game's code section as bytes.
pub(crate) fn text_bytes(base: u64) -> Option<&'static [u8]> {
    let (start, len) = text(base)?;
    // SAFETY: `text` found the `.text` section of the game executable's own
    // headers: `len` bytes from `start`, mapped readable for the process.
    Some(unsafe { core::slice::from_raw_parts(start as *const u8, len) })
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
    let inside = |(address, len): (usize, usize)| {
        len != 0
            && address >= text.0
            && address.saturating_add(len) <= text.0.saturating_add(text.1)
    };
    if patches
        .iter()
        .any(|p| p.replacement.len() != p.expected.len() || !inside(p.span()))
    {
        return Status::BAD_ARGS;
    }
    let mut pending = PENDING.relock();
    let clash = patches.iter().enumerate().any(|(i, patch)| {
        pending
            .iter()
            .chain(&patches[..i])
            .any(|p| overlaps(p.span(), patch.span()))
    });
    if clash {
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
// lookup that `lookup_address` finds by the `LOOKUP` pattern, and the slot is
// used only as the `next` of the `lookup_detour` link on that function.
static LOOKUP_NEXT: Next<LookupFn> = unsafe { Next::new() };
/// Full recipe lists of the workshops past the game's limit, as addresses.
static WORKSHOPS: OnceLock<FxHashMap<u32, Box<[usize]>>> = OnceLock::new();

/// The stop block's replacement: `inc r13` (the loop counter the skipped
/// append would have raised), then `jmp` to the next recipe.
fn stop_replacement() -> [u8; STOP_LEN] {
    let mut bytes = [0xcc; STOP_LEN];
    bytes[..4].copy_from_slice(&[0x49, 0xff, 0xc5, 0xe9]);
    let from = i32::try_from(STOP + 8).unwrap_or(i32::MAX);
    let to = i32::try_from(NEXT_RECIPE).unwrap_or(0);
    bytes[4..8].copy_from_slice(&(to - from).to_le_bytes());
    bytes
}

/// The one offset in `code` where `pattern` matches, or why not.
pub(crate) fn find(code: &[u8], pattern: &str) -> Result<usize, String> {
    let parsed = Pattern::parse(pattern).map_err(|e| e.to_string())?;
    parsed
        .find_unique(code)
        .map_err(|e| format!("{e} for {pattern}"))
}

/// The stop block's patch for the check at `site` in `code`, which starts at
/// `start`.
fn limit_patch(code: &[u8], start: usize) -> Result<CodePatch, String> {
    let site = find(code, LIMIT_SITE)?;
    let stop = site + STOP;
    let expected = code
        .get(stop..stop + STOP_LEN)
        .ok_or("the stop block is past the code")?;
    let starts = Pattern::parse(STOP_START).map_err(|e| e.to_string())?;
    if !starts.matches(expected) {
        return Err(format!("the stop block reads {}", hex(expected)));
    }
    Ok(CodePatch {
        owner: ModIndex::LOADER,
        address: start + stop,
        expected: expected.into(),
        replacement: stop_replacement().into(),
        needs: Box::default(),
    })
}

/// Prepares the lift of the workshop limit: the stop block's patch and the
/// lookup hook, both armed with the rest. Called before arming when mods
/// serve the recipe registry.
pub(crate) fn lift_workshop_limit(loader: &'static Loader) {
    let found = (|| {
        let wide = |v: u64| usize::try_from(v).map_err(|e| e.to_string());
        let offset = |ty: &str, field: &str| {
            loader
                .reflection
                .field_offset(ty, field)
                .map_err(|e| format!("{ty}.{field}: {e:?}"))
                .and_then(wide)
        };
        let layout = Layout {
            recipes: offset("keen::RecipeRegistryResource", "recipes")?,
            recipe: loader
                .reflection
                .type_info("keen::RecipeInfo")
                .map(|info| info.size() as usize)
                .map_err(|e| format!("keen::RecipeInfo: {e:?}"))?,
            workshop: offset("keen::RecipeInfo", "workshopId")?,
        };
        let (start, _) = text(loader.base).ok_or("the code section is not found")?;
        let code = text_bytes(loader.base).ok_or("the code section is not found")?;
        let patch = limit_patch(code, start)?;
        let lookup = lookup_address(loader).ok_or("the recipe list lookup is not linked")?;
        Ok::<_, String>((layout, patch, lookup))
    })();
    let (layout, patch, lookup) = match found {
        Ok(found) => found,
        Err(reason) => {
            crate::log!(WARN, "workshop recipe limit kept: {reason}");
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
        let (start, _) = text(loader.base)?;
        let code = text_bytes(loader.base)?;
        let lookup = match find(code, LOOKUP) {
            Ok(at) => (start + at) as u64,
            Err(why) => {
                crate::log!(WARN, "the recipe list lookup is not found: {why}");
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
    crate::bases::arming(patches, verdicts);
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
mod tests {
    use super::*;

    fn patch(owner: ModIndex, address: usize, expected: &[u8], replacement: &[u8]) -> CodePatch {
        CodePatch {
            owner,
            address,
            expected: expected.into(),
            replacement: replacement.into(),
            needs: Box::default(),
        }
    }

    #[test]
    fn ranges_overlap_only_when_they_share_a_byte() {
        assert!(overlaps((10, 5), (14, 1)));
        assert!(overlaps((14, 1), (10, 5)));
        assert!(!overlaps((10, 5), (15, 3)));
        assert!(!overlaps((15, 3), (10, 5)));
        assert!(!overlaps((10, 0), (10, 5)));
    }

    #[test]
    fn a_patch_that_reads_as_expected_is_written_and_a_changed_one_skipped() {
        let code = [0x83_u8, 0xf8, 0x08, 0x72, 0x05, 0xc3];
        let base = code.as_ptr().addr();
        let current = |address: usize, len: usize| {
            let from = address.checked_sub(base)?;
            code.get(from..from + len).map(<[u8]>::to_vec)
        };
        let patches = [
            patch(ModIndex::new(0), base + 2, &[0x08], &[0x10]),
            patch(ModIndex::new(1), base + 3, &[0x74], &[0xeb]),
            patch(ModIndex::new(2), base + 5, &[0xc3, 0x90], &[0x90, 0x90]),
        ];
        let verdicts = check(&patches, &[], |_| false, current);
        assert_eq!(verdicts[0], Verdict::Write);
        assert!(matches!(&verdicts[1], Verdict::Skip(why) if why.contains("72")));
        assert!(matches!(&verdicts[2], Verdict::Skip(why) if why.contains("cannot be read")));
    }

    #[test]
    fn an_owner_s_patches_go_in_together_or_not_at_all() {
        let code = [0x83_u8, 0xf8, 0x08, 0x72, 0x05, 0xc3];
        let base = code.as_ptr().addr();
        let current = |address: usize, len: usize| {
            let from = address.checked_sub(base)?;
            code.get(from..from + len).map(<[u8]>::to_vec)
        };
        let patches = [
            patch(ModIndex::new(0), base + 2, &[0x08], &[0x10]),
            patch(ModIndex::new(0), base + 3, &[0x74], &[0xeb]),
            patch(ModIndex::new(1), base + 5, &[0xc3], &[0x90]),
        ];
        let verdicts = check(&patches, &[], |_| false, current);
        assert!(matches!(&verdicts[0], Verdict::Skip(why) if why.contains("another patch")));
        assert!(matches!(&verdicts[1], Verdict::Skip(why) if why.contains("72")));
        assert_eq!(verdicts[2], Verdict::Write);
    }

    #[test]
    fn a_patch_over_hooked_bytes_is_skipped_and_one_of_a_mod_that_is_off_left_out() {
        let code = [0x90_u8; 16];
        let base = code.as_ptr().addr();
        let current = |address: usize, len: usize| {
            let from = address.checked_sub(base)?;
            code.get(from..from + len).map(<[u8]>::to_vec)
        };
        let patches = [
            patch(ModIndex::new(0), base + 4, &[0x90, 0x90], &[0xcc, 0xcc]),
            patch(ModIndex::new(1), base + 12, &[0x90], &[0xcc]),
        ];
        let verdicts = check(
            &patches,
            &[(base, 5)],
            |owner| owner == ModIndex::new(1),
            current,
        );
        assert!(matches!(&verdicts[0], Verdict::Skip(why) if why.contains("hook")));
        assert_eq!(verdicts[1], Verdict::Off);
    }

    #[test]
    fn registering_refuses_overlaps_bad_lengths_and_bytes_outside_the_code() {
        let text = (0x1000, 0x100);
        // Each test owns its addresses: the registry is shared.
        assert_eq!(
            register(patch(ModIndex::new(0), 0x1010, &[1, 2], &[3, 4]), text),
            Status::OK
        );
        assert_eq!(
            register(patch(ModIndex::new(1), 0x1011, &[2], &[5]), text),
            Status::REFUSED
        );
        assert_eq!(
            register(patch(ModIndex::new(1), 0x1012, &[9], &[5]), text),
            Status::OK
        );
        assert_eq!(
            register(patch(ModIndex::new(1), 0x1020, &[1], &[]), text),
            Status::BAD_ARGS
        );
        assert_eq!(
            register(patch(ModIndex::new(1), 0x1020, &[], &[]), text),
            Status::BAD_ARGS
        );
        assert_eq!(
            register(patch(ModIndex::new(1), 0x10ff, &[1, 2], &[3, 4]), text),
            Status::BAD_ARGS
        );
        assert_eq!(
            register(patch(ModIndex::new(1), 0x0fff, &[1, 2], &[3, 4]), text),
            Status::BAD_ARGS
        );
    }

    #[test]
    fn a_patch_whose_hook_is_not_armed_stays_out_with_its_owner_s_others() {
        let code = [0x90_u8; 16];
        let base = code.as_ptr().addr();
        let current = |address: usize, len: usize| {
            let from = address.checked_sub(base)?;
            code.get(from..from + len).map(<[u8]>::to_vec)
        };
        let hook = u64::try_from(base + 8).unwrap();
        let needing = |owner, at| CodePatch {
            needs: Box::new([hook]),
            ..patch(owner, base + at, &[0x90], &[0xcc])
        };
        let patches = [
            needing(ModIndex::new(0), 0),
            patch(ModIndex::new(0), base + 1, &[0x90], &[0xcc]),
        ];
        let verdicts = check(&patches, &[], |_| false, current);
        assert!(matches!(&verdicts[0], Verdict::Skip(why) if why.contains("needs")));
        assert!(matches!(&verdicts[1], Verdict::Skip(why) if why.contains("another patch")));
        let verdicts = check(&patches, &[(base + 8, 5)], |_| false, current);
        assert_eq!(verdicts, [Verdict::Write, Verdict::Write]);
    }

    #[test]
    fn a_set_of_patches_registers_whole_or_not_at_all() {
        let text = (0x2000, 0x100);
        assert_eq!(
            register_all(
                vec![
                    patch(ModIndex::new(0), 0x2010, &[1], &[2]),
                    patch(ModIndex::new(0), 0x2020, &[1, 2], &[3]),
                ],
                text
            ),
            Status::BAD_ARGS
        );
        assert_eq!(
            register_all(
                vec![
                    patch(ModIndex::new(0), 0x2010, &[1], &[2]),
                    patch(ModIndex::new(0), 0x2010, &[1], &[3])
                ],
                text
            ),
            Status::REFUSED
        );
        assert_eq!(
            register(patch(ModIndex::new(1), 0x2010, &[1], &[2]), text),
            Status::OK
        );
        assert_eq!(
            register(patch(ModIndex::new(1), 0x2020, &[1], &[2]), text),
            Status::OK
        );
    }

    #[test]
    fn the_stop_block_becomes_inc_r13_and_a_jump_to_the_next_recipe() {
        let mut code = vec![0x90_u8; 0x200];
        let site = 0x40;
        let check = Pattern::parse(LIMIT_SITE).unwrap();
        assert_eq!(check.len(), 28);
        for (i, byte) in LIMIT_SITE.split(' ').enumerate() {
            code[site + i] = u8::from_str_radix(byte, 16).unwrap();
        }
        let stop = site + STOP;
        code[stop..stop + 12]
            .copy_from_slice(&[0x4c, 0x8d, 0x05, 1, 2, 3, 4, 0x33, 0xc9, 0x48, 0x8d, 0x15]);
        let made = limit_patch(&code, 0x1_4000_0000).unwrap();
        assert_eq!(made.address, 0x1_4000_0000 + stop);
        assert_eq!(&made.expected[..], &code[stop..stop + STOP_LEN]);
        // The `je` of the check lands on the stop block.
        let je = i32::from_le_bytes(code[site + 0x11..site + 0x15].try_into().unwrap());
        assert_eq!(site + 0x15 + usize::try_from(je).unwrap(), stop);
        // inc r13, then jmp rel32 from the end of the jump to the next recipe.
        assert_eq!(&made.replacement[..4], &[0x49, 0xff, 0xc5, 0xe9]);
        let rel = i32::from_le_bytes(made.replacement[4..8].try_into().unwrap());
        let end = i64::try_from(stop + 8).unwrap();
        assert_eq!(
            end + i64::from(rel),
            i64::try_from(site + NEXT_RECIPE).unwrap()
        );
        assert!(made.replacement[8..].iter().all(|&b| b == 0xcc));

        code[stop] = 0x90;
        assert!(limit_patch(&code, 0).is_err());
    }

    #[test]
    fn the_text_section_is_found_in_the_headers() {
        let mut headers = vec![0_u8; 0x400];
        headers[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
        headers[0x80..0x84].copy_from_slice(b"PE\0\0");
        headers[0x86..0x88].copy_from_slice(&2_u16.to_le_bytes());
        headers[0x94..0x96].copy_from_slice(&0xf0_u16.to_le_bytes());
        let first = 0x80 + 24 + 0xf0;
        headers[first..first + 8].copy_from_slice(b".rdata\0\0");
        let second = first + 40;
        headers[second..second + 8].copy_from_slice(b".text\0\0\0");
        headers[second + 8..second + 12].copy_from_slice(&0x5000_u32.to_le_bytes());
        headers[second + 12..second + 16].copy_from_slice(&0x1000_u32.to_le_bytes());
        assert_eq!(text_section(&headers), Some((0x1000, 0x5000)));
        headers[second] = b'x';
        assert_eq!(text_section(&headers), None);
    }

    #[test]
    fn only_workshops_past_the_limit_get_full_lists() {
        let layout = Layout {
            recipes: 8,
            recipe: 0x10,
            workshop: 4,
        };
        let mut registry = vec![0_u8; 0x10 + 5 * 0x10];
        registry[8..12].copy_from_slice(&8_i32.to_le_bytes());
        registry[12..16].copy_from_slice(&5_u32.to_le_bytes());
        for (i, workshop) in [7_u32, 9, 7, 7, 9].into_iter().enumerate() {
            let at = 0x10 + i * 0x10 + 4;
            registry[at..at + 4].copy_from_slice(&workshop.to_le_bytes());
        }
        let lists = full_lists(&registry, layout, 2).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[&7], [0x10, 0x30, 0x40]);
        assert!(full_lists(&registry[..0x30], layout, 2).is_none());
    }
}
