use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use super::super::{Patch, State};
use super::*;

const KEY: ResourceKey = ResourceKey {
    guid: [9; 16],
    type_hash: 1,
    part: 0,
};

static SEEN_BUILDING: AtomicBool = AtomicBool::new(false);
static SEEN_SCRATCH: AtomicBool = AtomicBool::new(false);
/// The byte [`value`] serves.
static VALUE: AtomicU8 = AtomicU8::new(1);

unsafe extern "system" fn note_building(
    base: *const u8,
    base_len: usize,
    _user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    SEEN_BUILDING.store(building(&KEY), Ordering::Relaxed);
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

/// Builds its base and one more byte in a buffer the loader lends.
unsafe extern "system" fn grow(
    base: *const u8,
    base_len: usize,
    _user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    let Some(at) = lend(base_len + 1) else {
        return Status::REFUSED;
    };
    // SAFETY: `lend` gave `base_len + 1` writable bytes at `at`, apart from
    // the `base_len` bytes `run_patches` passes readable at `base`.
    unsafe { core::ptr::copy_nonoverlapping(base, at, base_len) };
    // SAFETY: `base_len` is below the `base_len + 1` bytes `lend` gave.
    let last = unsafe { at.add(base_len) };
    // SAFETY: `last` is the last of the bytes `lend` gave, writable.
    unsafe { last.write(1) };
    // SAFETY: `run_patches` passes `out` pointing at a local, writable for
    // the call.
    unsafe { out.write(at.cast_const()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
    // writable for the call.
    unsafe { out_len.write(base_len + 1) };
    Status::OK
}

/// Builds its base after one byte it skips: the output starts inside the
/// buffer the loader lent.
unsafe extern "system" fn skip_first(
    base: *const u8,
    base_len: usize,
    _user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    let Some(at) = lend(base_len + 1) else {
        return Status::REFUSED;
    };
    // SAFETY: `base_len` is below the `base_len + 1` bytes `lend` gave.
    let rest = unsafe { at.add(1) };
    // SAFETY: `rest` has `base_len` writable bytes, apart from the
    // `base_len` bytes `run_patches` passes readable at `base`.
    unsafe { core::ptr::copy_nonoverlapping(base, rest, base_len) };
    // SAFETY: `run_patches` passes `out` pointing at a local, writable for
    // the call.
    unsafe { out.write(rest.cast_const()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
    // writable for the call.
    unsafe { out_len.write(base_len) };
    Status::OK
}

/// Builds its base at the start of a buffer one byte longer: the output
/// fills only part of the buffer the loader lent.
unsafe extern "system" fn short_of_buffer(
    base: *const u8,
    base_len: usize,
    _user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    let Some(at) = lend(base_len + 1) else {
        return Status::REFUSED;
    };
    // SAFETY: `lend` gave `base_len + 1` writable bytes at `at`, apart from
    // the `base_len` bytes `run_patches` passes readable at `base`.
    unsafe { core::ptr::copy_nonoverlapping(base, at, base_len) };
    // SAFETY: `run_patches` passes `out` pointing at a local, writable for
    // the call.
    unsafe { out.write(at.cast_const()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
    // writable for the call.
    unsafe { out_len.write(base_len) };
    Status::OK
}

/// Serves one byte, [`VALUE`].
unsafe extern "system" fn value(
    _base: *const u8,
    _base_len: usize,
    _user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `run_patches` passes `out` and `out_len` writable for the call.
    unsafe { hand_out(vec![VALUE.load(Ordering::Relaxed)], out, out_len) }
}

/// Serves the served entry at `user` as a mod's served read finds it now.
unsafe extern "system" fn read_other(
    _base: *const u8,
    _base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is a served entry the test leaked for the process.
    let other = unsafe { &*user.cast::<Served>() };
    match read_served(other, vec![0], unarchived) {
        // SAFETY: `run_patches` passes `out` and `out_len` writable for the
        // call.
        Ok(bytes) => unsafe { hand_out(bytes, out, out_len) },
        Err(status) => status,
    }
}

/// An archive without the key: no game runs the tests.
fn unarchived(_key: &ResourceKey) -> Result<Vec<u8>, Status> {
    Err(Status::MISSING)
}

fn by_loader(apply: enshwick_core::abi::ResourcePatchFn) -> Patch {
    Patch {
        owner: ModIndex::LOADER,
        apply,
        user: 0,
    }
}

/// The game's first build of each of `entries` from a base of one zero byte.
fn build_each(entries: &[Served]) {
    let base = [0u8];
    for entry in entries {
        let mut state = State::default();
        // SAFETY: `base` holds one readable byte, unchanged for the call.
        let built = unsafe { build_first(entry, &mut state, base.as_ptr().expose_provenance(), 1) };
        assert!(built.is_some());
        *entry.state.relock() = state;
    }
}

#[test]
fn a_key_counts_as_building_only_while_its_patches_run() {
    let owner = ModIndex::new(3);
    let entry = Served::new(
        KEY,
        KEY,
        vec![Patch {
            owner,
            apply: note_building,
            user: 0,
        }],
        false,
    );
    let base = [1u8; 4];
    let at = base.as_ptr().expose_provenance();
    let built = run_patches(&entry, at, base.len(), Some(&[owner]), Build::Game);
    assert!(built.is_some());
    assert!(SEEN_BUILDING.load(Ordering::Relaxed), "inside its build");
    assert!(!SEEN_SCRATCH.load(Ordering::Relaxed), "the game's build");
    assert!(BUILDING.with_borrow(Vec::is_empty), "after its build");
    assert!(!building(&KEY), "after its build");
    run_patches(&entry, at, base.len(), Some(&[owner]), Build::Scratch);
    assert!(SEEN_SCRATCH.load(Ordering::Relaxed), "a mod's served read");
    assert!(!scratch(), "after the read");
}

#[test]
fn a_failed_patch_in_a_served_read_is_skipped_and_the_rest_builds() {
    let (failing, building) = (ModIndex::new(3), ModIndex::new(4));
    let entry = Served::new(
        KEY,
        KEY,
        vec![
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
        ],
        false,
    );
    let base = [1u8; 4];
    let at = base.as_ptr().expose_provenance();
    let built = run_patches(
        &entry,
        at,
        base.len(),
        Some(&[failing, building]),
        Build::Scratch,
    )
    .unwrap();
    assert_eq!((built.len, built.by), (4, vec![building]));
}

#[test]
fn a_served_read_is_refused_before_the_registrations_seal() {
    assert_eq!(served_now(KEY), Err(Status::REFUSED));
}

#[test]
fn a_served_read_whose_every_patch_fails_reads_a_changed_key_as_archived_and_a_new_one_as_missing()
{
    let changed = Served::new(KEY, KEY, vec![by_loader(fail)], false);
    assert_eq!(
        read_served(&changed, vec![5; 3], unarchived),
        Ok(vec![5; 3])
    );
    let new = ResourceKey { part: 1, ..KEY };
    let added = Served::new(new, KEY, vec![by_loader(fail)], false);
    assert_eq!(
        read_served(&added, vec![5; 3], unarchived),
        Err(Status::MISSING)
    );
}

#[test]
fn served_reads_free_every_buffer_their_patches_took() {
    let entry = Served::new(KEY, KEY, vec![by_loader(grow), by_loader(grow)], false);
    for _ in 0..100 {
        assert_eq!(
            read_served(&entry, vec![7; 4], unarchived),
            Ok(vec![7, 7, 7, 7, 1, 1])
        );
    }
    assert_eq!(LIVE.get(), 0, "no patch output outlives its read");
}

#[test]
fn an_output_inside_a_lent_buffer_reads_as_its_own_bytes_and_frees_the_buffer() {
    for apply in [skip_first, short_of_buffer] {
        let entry = Served::new(KEY, KEY, vec![by_loader(apply)], false);
        assert_eq!(read_served(&entry, vec![3, 4], unarchived), Ok(vec![3, 4]));
    }
    assert_eq!(LIVE.get(), 0);
}

#[test]
fn the_game_s_build_keeps_only_its_last_output_and_a_rebuild_frees_its_own() {
    let entry = Served::new(KEY, KEY, vec![by_loader(grow), by_loader(grow)], true);
    build_each(core::slice::from_ref(&entry));
    assert_eq!(LIVE.get(), 1, "the game holds the last output");
    assert_eq!(entry.state.relock().built.map(|(_, len)| len), Some(3));
    for _ in 0..10 {
        assert_eq!(rebuild_one(&entry), Ok(Some(0)));
    }
    assert_eq!(LIVE.get(), 1, "a rebuild frees what it built");
}

#[test]
fn a_rebuild_reaches_the_keys_whose_builds_read_the_rebuilt_one_and_theirs() {
    let (read, reader, outer) = (
        KEY,
        ResourceKey { part: 1, ..KEY },
        ResourceKey { part: 2, ..KEY },
    );
    let list: &mut [Served; 3] = Box::leak(Box::new([
        Served::new(read, read, vec![by_loader(value)], true),
        Served::new(reader, reader, vec![by_loader(read_other)], false),
        Served::new(outer, outer, vec![by_loader(read_other)], false),
    ]));
    list[1].patches[0].user = (&raw const list[0]).expose_provenance();
    list[2].patches[0].user = (&raw const list[1]).expose_provenance();
    let list: &[Served; 3] = list;
    VALUE.store(1, Ordering::Relaxed);
    build_each(list);
    assert!(
        list[1].state.relock().base.is_some(),
        "a build that read a served key keeps its base"
    );
    assert_eq!(
        *list[0].read_by.relock(),
        [reader, outer],
        "a read inside a read counts for every game build on the thread"
    );
    assert_eq!(*list[1].read_by.relock(), [outer]);
    // The outer one now reaches the rebuilt key only through its reader.
    list[0].read_by.relock().truncate(1);
    // The reader and the read read each other, as two items re-keyed to each
    // other do.
    list[1].read_by.relock().push(read);
    VALUE.store(2, Ordering::Relaxed);
    assert_eq!(rebuild_one(&list[0]), Ok(Some(1)));
    rebuild_readers(list, &list[0]);
    for entry in &list[1..] {
        let (data, _) = entry.state.relock().built.unwrap();
        let mut held = [0u8];
        // SAFETY: `data` is the entry's first build, one byte kept for the
        // process.
        unsafe { crate::memory::read_bytes(data, 0, &mut held) };
        assert_eq!(held, [2], "{:?} takes the rebuilt value", entry.key);
    }
}

/// A key that serves [`VALUE`] and one whose game build read it, the first
/// read by a mod's served read only, never built by the game.
fn read_and_reader() -> &'static [Served; 2] {
    let reader = ResourceKey { part: 1, ..KEY };
    let list: &mut [Served; 2] = Box::leak(Box::new([
        Served::new(KEY, KEY, vec![by_loader(value)], true),
        Served::new(reader, reader, vec![by_loader(read_other)], false),
    ]));
    list[1].patches[0].user = (&raw const list[0]).expose_provenance();
    VALUE.store(1, Ordering::Relaxed);
    build_each(&list[1..]);
    list
}

/// The one byte the game holds for `entry`'s first build.
fn held(entry: &Served) -> u8 {
    let (data, _) = entry.state.relock().built.unwrap();
    let mut held = [0u8];
    // SAFETY: `data` is the entry's first build, one byte kept for the
    // process.
    unsafe { crate::memory::read_bytes(data, 0, &mut held) };
    held[0]
}

#[test]
fn a_rebuild_of_a_key_the_game_never_built_still_rebuilds_the_keys_that_read_it() {
    let list = read_and_reader();
    assert!(
        list[0].state.relock().built.is_none(),
        "only a served read built it"
    );
    VALUE.store(2, Ordering::Relaxed);
    assert_eq!(rebuild_entry(list, &list[0]), Status::OK);
    assert_eq!(held(&list[1]), 2, "the reader takes the rebuilt value");
}

#[test]
fn a_rebuild_leaves_a_reader_whose_build_runs_on_this_thread() {
    let list = read_and_reader();
    // The game's first build of the reader holds its state while its patch
    // rebuilds the key it read.
    let _game = list[1].state.relock();
    let _building = Building::enter(list[1].key, Build::Game);
    VALUE.store(2, Ordering::Relaxed);
    rebuild_readers(list, &list[0]);
}

#[test]
fn a_served_read_whose_every_patch_fails_reads_a_vanilla_key_from_another_base_as_archived() {
    let other = ResourceKey { part: 1, ..KEY };
    let entry = Served::new(KEY, other, vec![by_loader(fail)], false);
    assert_eq!(
        read_served(&entry, vec![5], |key| {
            assert_eq!(*key, KEY);
            Ok(vec![8])
        }),
        Ok(vec![8])
    );
}
