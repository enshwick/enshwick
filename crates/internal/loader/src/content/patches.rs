//! Running mods' patches of a served resource: its first build, a rebuild
//! over the copy the game holds, and a build for a mod that reads a key as
//! served.

use std::sync::atomic::Ordering;

use enshwick::abi::{ResourceKey, Status};

use super::{LOADED, PENDING, SERVED_LIST, Served, find, hex, key_name, mod_on};
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

/// Resource `key` as the game reads it now: a served key built from its
/// base in the archive with the patches of the mods on, as the game's
/// first read builds it, kept nowhere; any other key from the archive.
/// Refused before the registrations seal and for a key whose patches run
/// on this thread, which a patch reading it would build again without end.
pub(crate) fn served_now(key: ResourceKey) -> Result<Vec<u8>, Status> {
    if SERVED_LIST.get().is_none() || BUILDING.with_borrow(|building| building.contains(&key)) {
        return Err(Status::REFUSED);
    }
    let archive = |key: &ResourceKey| {
        crate::archive::game()
            .and_then(|archive| archive.resource(key))
            .ok_or(Status::MISSING)
    };
    let Some((_, entry)) = find(&key) else {
        return archive(&key);
    };
    let base = archive(&entry.base)?;
    let (data, len, _) = run_patches(entry, base.as_ptr().expose_provenance(), base.len(), None)
        .ok_or(Status::MISSING)?;
    let mut bytes = vec![0; len];
    // SAFETY: `run_patches` applied a patch, so `data` is the last patch's
    // `len` built bytes, valid for the process (`ResourcePatchFn`); only a
    // rebuild writes over built bytes, and it never learns of these, as
    // this build is kept nowhere.
    unsafe { crate::memory::read_bytes(data, 0, &mut bytes) };
    Ok(bytes)
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

thread_local! {
    /// The served keys whose patches run on this thread, innermost last.
    static BUILDING: core::cell::RefCell<Vec<ResourceKey>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

/// Marks a key's patches running on this thread until dropped, unwinding
/// included.
struct Building;

impl Building {
    fn enter(key: ResourceKey) -> Self {
        BUILDING.with_borrow_mut(|building| building.push(key));
        Self
    }
}

impl Drop for Building {
    fn drop(&mut self) {
        BUILDING.with_borrow_mut(Vec::pop);
    }
}

/// Runs the patches of the mods still on, or of the mods `only` names,
/// over the `len` bytes at `data`, each on the result of the one before; a
/// failed one turns its mod off and is skipped. The bytes built, their
/// length and who built them; `None` when no patch did.
pub(super) fn run_patches(
    entry: &Served,
    data: usize,
    len: usize,
    only: Option<&[ModIndex]>,
) -> Option<(usize, usize, Vec<ModIndex>)> {
    let _building = Building::enter(entry.key);
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
