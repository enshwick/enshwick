//! Running mods' patches of a served resource: its first build, a rebuild
//! over the copy the game holds, and a build for a mod that reads a key as
//! served.

use std::sync::atomic::Ordering;

use enshwick::abi::{ResourceKey, Status};

use super::{
    LOADED, OPEN_NEXT, PENDING, RESOLVE_NEXT, RESOURCES_FUSE, SERVED_LIST, Served, hex, key_name,
    mod_on, route,
};
use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// For mod `owner`, which serves `key`: during init, keeps its base when it
/// is first built; afterwards, builds it again from that base and writes the
/// bytes that differ over the resource the game holds.
pub(crate) fn rebuild(owner: ModIndex, key: ResourceKey) -> Status {
    let serves = |s: &Served| s.key == key && s.patches.iter().any(|p| p.owner == owner);
    let Some(list) = SERVED_LIST.get() else {
        let pending = PENDING.relock();
        return match pending.iter().find(|s| serves(s)) {
            Some(entry) => {
                entry.keeps_base.store(true, Ordering::Relaxed);
                Status::OK
            }
            None => Status::BAD_ARGS,
        };
    };
    let Some(entry) = list.iter().find(|s| serves(s)) else {
        return Status::BAD_ARGS;
    };
    let _one_at_a_time = entry.rebuilding.relock();
    // The game resolves the resource meanwhile: the patches run outside the
    // state's lock, on a copy of the base.
    let (data, len, base, built_by) = {
        let state = entry.state.relock();
        let Some((data, len)) = state.built else {
            // The first build takes the values in force.
            return Status::OK;
        };
        let Some(base) = state.base.clone() else {
            return Status::MISSING;
        };
        (data, len, base, state.built_by.clone())
    };
    let Some((built, built_len, _)) = run_patches(
        entry,
        base.as_ptr().expose_provenance(),
        base.len(),
        Some(&built_by),
        Build::Game,
    ) else {
        return Status::MISSING;
    };
    if built_len != len {
        crate::log!(
            WARN,
            "served resource {:08x}:{} not rebuilt: {built_len} bytes, the game holds {len}",
            entry.key.type_hash,
            hex(&entry.key.guid)
        );
        return Status::REFUSED;
    }
    let changed = write_differences(data, built, len);
    crate::log!(
        INFO,
        "served resource {:08x}:{} rebuilt, {changed} bytes changed",
        entry.key.type_hash,
        hex(&entry.key.guid)
    );
    Status::OK
}

/// Resource `key` as the game's first read would build it now: a served
/// key, renamed keys included, built from its base in the archive with the
/// patches of the mods on, kept nowhere; any other key, and every key while
/// served resources are not linked or off, as the game opens it from the
/// archive. A patch that fails is skipped, its mod left on. Refused before
/// the registrations seal; [`Status::BUILDING`] for a key whose patches run
/// on this thread, which a patch reading it would build again without end.
pub(crate) fn served_now(key: ResourceKey) -> Result<Vec<u8>, Status> {
    if SERVED_LIST.get().is_none() {
        return Err(Status::REFUSED);
    }
    let archive = |key: &ResourceKey| {
        crate::archive::game()
            .and_then(|archive| archive.resource(key))
            .ok_or(Status::MISSING)
    };
    if !linked() {
        return archive(&key);
    }
    let (found, new, aliased) = route(&key);
    let Some((_, entry)) = found else {
        return archive(&new.or(aliased).unwrap_or(key));
    };
    if BUILDING.with_borrow(|building| building.iter().any(|(at, _)| *at == entry.key)) {
        return Err(Status::BUILDING);
    }
    let base = archive(&entry.base)?;
    let (data, len, _) = run_patches(
        entry,
        base.as_ptr().expose_provenance(),
        base.len(),
        None,
        Build::Scratch,
    )
    .ok_or(Status::MISSING)?;
    let mut bytes = vec![0; len];
    // SAFETY: `run_patches` applied a patch, so `data` is the last patch's
    // `len` built bytes, valid for the process (`ResourcePatchFn`); only a
    // rebuild writes over built bytes, and it never learns of these, as
    // this build is kept nowhere.
    unsafe { crate::memory::read_bytes(data, 0, &mut bytes) };
    Ok(bytes)
}

/// Whether the game reads served resources: their links made and not off.
fn linked() -> bool {
    !RESOURCES_FUSE.is_off() && OPEN_NEXT.wired().is_some() && RESOLVE_NEXT.wired().is_some()
}

/// Whether patches run on this thread for a mod's served read, not for
/// the game: the loader's own patches then log nothing, as the game's
/// build logs once.
pub(crate) fn scratch() -> bool {
    BUILDING.with_borrow(|building| building.iter().any(|(_, build)| *build == Build::Scratch))
}

/// Writes the bytes of the `len` bytes at `new` that differ over those at
/// `held`, which the game reads meanwhile; how many differed.
pub(super) fn write_differences(held: usize, new: usize, len: usize) -> usize {
    // SAFETY: `new` is a patch's build of `len` bytes, valid for the
    // process.
    let new =
        unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(new), len) };
    let held = core::ptr::with_exposed_provenance_mut::<u8>(held);
    let mut changed = 0;
    for (at, &byte) in new.iter().enumerate() {
        let slot = held.wrapping_add(at);
        // SAFETY: `held` is the resource the game reads, `len` bytes a patch
        // built and left writable for the process (`ResourcePatchFn`), and
        // `at` is below `len`. A volatile read, which `enshwick::raw` has
        // not: the bytes are the patch's own, read while the game reads them.
        // ast-grep-ignore: game-memory-through-raw
        let old = unsafe { slot.read_volatile() };
        if old != byte {
            // SAFETY: `held` is the patch's build of `len` bytes, writable for
            // the process (`ResourcePatchFn`), and `at` is below `len`; one
            // byte at a time, so a read meanwhile sees each byte old or new.
            unsafe { slot.write_volatile(byte) };
            changed += 1;
        }
    }
    changed
}

/// Who a build of a served resource is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Build {
    /// The game's first build or a rebuild, whose bytes the game holds: a
    /// failed patch turns its mod off.
    Game,
    /// A mod's served read, kept nowhere: a failed patch is skipped and its
    /// mod stays on, since the game never read that build.
    Scratch,
}

thread_local! {
    /// The served keys whose patches run on this thread and who for,
    /// innermost last.
    static BUILDING: core::cell::RefCell<Vec<(ResourceKey, Build)>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

/// Marks a key's patches running on this thread until dropped, unwinding
/// included.
struct Building;

impl Building {
    fn enter(key: ResourceKey, build: Build) -> Self {
        BUILDING.with_borrow_mut(|building| building.push((key, build)));
        Self
    }
}

impl Drop for Building {
    fn drop(&mut self) {
        BUILDING.with_borrow_mut(Vec::pop);
    }
}

/// Runs the patches of the mods still on, or of the mods `only` names,
/// over the `len` bytes at `data`, each on the result of the one before,
/// for `kind`; a failed one is skipped, and in the game's build turns its
/// mod off. The bytes built, their length and who built them; `None` when
/// no patch did.
pub(super) fn run_patches(
    entry: &Served,
    data: usize,
    len: usize,
    only: Option<&[ModIndex]>,
    kind: Build,
) -> Option<(usize, usize, Vec<ModIndex>)> {
    let _building = Building::enter(entry.key, kind);
    let (mut current, mut current_len) = (data, len);
    let mut applied = Vec::new();
    let runs = |owner: ModIndex| only.map_or_else(|| mod_on(owner), |only| only.contains(&owner));
    for patch in entry.patches.iter().filter(|p| runs(p.owner)) {
        let (mut built, mut built_len) = (core::ptr::null::<u8>(), 0usize);
        // SAFETY: `patch.apply` is a live mod's or the loader's
        // `ResourcePatchFn`, given the `current_len` bytes so far at
        // `current`, valid for the call, the user pointer it registered and
        // out pointers to locals on this stack.
        let mut apply = || unsafe {
            (patch.apply)(
                core::ptr::with_exposed_provenance(current),
                current_len,
                core::ptr::with_exposed_provenance_mut(patch.user),
                &raw mut built,
                &raw mut built_len,
            )
        };
        let status = if patch.owner == ModIndex::LOADER {
            apply()
        } else {
            crate::profile::call(patch.owner, crate::profile::Feature::Resources, apply)
        };
        if status != Status::OK || built.is_null() {
            if kind == Build::Scratch {
                continue;
            }
            if patch.owner == ModIndex::LOADER {
                crate::log!(
                    WARN,
                    "the loader's own patch of {} failed to build",
                    key_name(&entry.key)
                );
            } else if let Some(loader) = LOADED.get() {
                loader.turn_off(
                    patch.owner,
                    &format!(
                        "served {} failed to build ({})",
                        key_name(&entry.key),
                        status.0
                    ),
                );
            }
            continue;
        }
        (current, current_len) = (built.expose_provenance(), built_len);
        applied.push(patch.owner);
    }
    (!applied.is_empty()).then_some((current, current_len, applied))
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::super::{Patch, State};
    use super::*;

    const KEY: ResourceKey = ResourceKey {
        guid: [9; 16],
        type_hash: 1,
        part: 0,
    };

    static SEEN_BUILDING: AtomicBool = AtomicBool::new(false);
    static SEEN_SCRATCH: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn note_building(
        base: *const u8,
        base_len: usize,
        _user: *mut c_void,
        out: *mut *const u8,
        out_len: *mut usize,
    ) -> Status {
        SEEN_BUILDING.store(
            BUILDING.with_borrow(|building| building.iter().any(|(key, _)| *key == KEY)),
            Ordering::Relaxed,
        );
        SEEN_SCRATCH.store(scratch(), Ordering::Relaxed);
        // SAFETY: `run_patches` passes `out` pointing at a local, writable
        // for the call; `base` is the test's own array, which outlives it.
        unsafe { out.write(base) };
        // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
        // writable for the call.
        unsafe { out_len.write(base_len) };
        Status::OK
    }

    unsafe extern "system" fn fail(
        _base: *const u8,
        _base_len: usize,
        _user: *mut c_void,
        _out: *mut *const u8,
        _out_len: *mut usize,
    ) -> Status {
        Status::REFUSED
    }

    fn served(patches: Vec<Patch>) -> Served {
        Served {
            key: KEY,
            base: KEY,
            patches,
            keeps_base: AtomicBool::new(false),
            state: Mutex::new(State::default()),
            rebuilding: Mutex::new(()),
        }
    }

    #[test]
    fn a_key_counts_as_building_only_while_its_patches_run() {
        let owner = ModIndex::new(3);
        let entry = served(vec![Patch {
            owner,
            apply: note_building,
            user: 0,
        }]);
        let base = [1u8; 4];
        let at = base.as_ptr().expose_provenance();
        let built = run_patches(&entry, at, base.len(), Some(&[owner]), Build::Game);
        assert!(built.is_some());
        assert!(SEEN_BUILDING.load(Ordering::Relaxed), "inside its build");
        assert!(!SEEN_SCRATCH.load(Ordering::Relaxed), "the game's build");
        assert!(BUILDING.with_borrow(Vec::is_empty), "after its build");
        run_patches(&entry, at, base.len(), Some(&[owner]), Build::Scratch);
        assert!(SEEN_SCRATCH.load(Ordering::Relaxed), "a mod's served read");
        assert!(!scratch(), "after the read");
    }

    #[test]
    fn a_failed_patch_in_a_served_read_is_skipped_and_the_rest_builds() {
        let (failing, building) = (ModIndex::new(3), ModIndex::new(4));
        let entry = served(vec![
            Patch {
                owner: failing,
                apply: fail,
                user: 0,
            },
            Patch {
                owner: building,
                apply: note_building,
                user: 0,
            },
        ]);
        let base = [1u8; 4];
        let at = base.as_ptr().expose_provenance();
        let (_, len, by) = run_patches(
            &entry,
            at,
            base.len(),
            Some(&[failing, building]),
            Build::Scratch,
        )
        .unwrap();
        assert_eq!((len, by), (4, vec![building]));
    }

    #[test]
    fn a_served_read_is_refused_before_the_registrations_seal() {
        assert_eq!(served_now(KEY), Err(Status::REFUSED));
    }
}
