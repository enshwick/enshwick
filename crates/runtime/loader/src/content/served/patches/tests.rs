use core::ffi::c_void;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use super::super::{Patch, State};
use super::buffer::{LIVE, PAGE};
use super::*;

const KEY: ResourceKey = ResourceKey {
    guid: enshwick_core::content::Guid([9; 16]),
    type_hash: 1,
    part: 0,
};

std::thread_local! {
    /// What [`note_building`] saw, on the thread of the test that ran it.
    static SEEN_BUILDING: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    static SEEN_SCRATCH: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}
/// The byte [`value`] serves.
static VALUE: AtomicU8 = AtomicU8::new(1);

/// Notes what the thread builds and hands its base out unchanged.
unsafe extern "system" fn note_building(
    base: *const u8,
    base_len: u32,
    _user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    SEEN_BUILDING.set(building(&KEY));
    SEEN_SCRATCH.set(scratch());
    // SAFETY: `run_patches` passes `base_len` readable bytes at `base`.
    let bytes = unsafe { core::slice::from_raw_parts(base, base_len as usize) }.to_vec();
    // SAFETY: `run_patches` passes `out` and `out_len` pointing at locals,
    // writable for the call.
    unsafe { hand_out(bytes, out, out_len) }
}

/// Hands its base back as its output: bytes no buffer of the build holds.
unsafe extern "system" fn own_bytes(
    base: *const u8,
    base_len: u32,
    _user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `run_patches` passes `out` pointing at a local, writable for
    // the call.
    unsafe { out.write(base.cast_mut()) };
    // SAFETY: as `out`, for `out_len`.
    unsafe { out_len.write(base_len) };
    Status::OK
}

unsafe extern "system" fn fail(
    _base: *const u8,
    _base_len: u32,
    _user: *mut c_void,
    _out: *mut *mut u8,
    _out_len: *mut u32,
) -> Status {
    Status::REFUSED
}

/// Copies the `base_len` bytes at `base` to `skip` bytes into a buffer the
/// loader lends, `extra` bytes longer, sets the byte after the copy to 1
/// when the buffer has one, and writes `len` bytes from `skip` as the
/// output.
///
/// # Safety
///
/// - As [`enshwick_core::abi::ResourcePatchFn`] says, in a build.
unsafe fn lent_output(
    (base, base_len): (*const u8, u32),
    (skip, extra, len): (usize, usize, u32),
    (out, out_len): (*mut *mut u8, *mut u32),
) -> Status {
    let base_len = base_len as usize;
    let Some(at) = lend(base_len + extra) else {
        return Status::REFUSED;
    };
    // SAFETY: `lend` gave `base_len + extra` writable bytes at `at`, owned by
    // the build and touched by nothing else during this call.
    let lent = unsafe { core::slice::from_raw_parts_mut(at, base_len + extra) };
    // SAFETY: the caller guarantees `base_len` readable bytes at `base`.
    let base = unsafe { core::slice::from_raw_parts(base, base_len) };
    lent[skip..skip + base_len].copy_from_slice(base);
    if let Some(after) = lent.get_mut(skip + base_len) {
        *after = 1;
    }
    let output = lent[skip..].as_mut_ptr();
    // SAFETY: the caller guarantees `out` writable for one pointer.
    unsafe { out.write(output) };
    // SAFETY: the caller guarantees `out_len` writable for one `u32`.
    unsafe { out_len.write(len) };
    Status::OK
}

/// Builds its base and one more byte in a buffer the loader lends.
unsafe extern "system" fn grow(
    base: *const u8,
    base_len: u32,
    _user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `run_patches` passes a base and outs as `ResourcePatchFn` says.
    unsafe { lent_output((base, base_len), (0, 1, base_len + 1), (out, out_len)) }
}

/// Builds its base after one byte it skips: the output starts inside the
/// buffer the loader lent.
unsafe extern "system" fn skip_first(
    base: *const u8,
    base_len: u32,
    _user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `run_patches` passes a base and outs as `ResourcePatchFn` says.
    unsafe { lent_output((base, base_len), (1, 1, base_len), (out, out_len)) }
}

/// Builds its base at the start of a buffer one byte longer: the output
/// fills only part of the buffer the loader lent.
unsafe extern "system" fn short_of_buffer(
    base: *const u8,
    base_len: u32,
    _user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `run_patches` passes a base and outs as `ResourcePatchFn` says.
    unsafe { lent_output((base, base_len), (0, 1, base_len), (out, out_len)) }
}

/// Serves one byte, [`VALUE`].
unsafe extern "system" fn value(
    _base: *const u8,
    _base_len: u32,
    _user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `run_patches` passes `out` and `out_len` writable for the call.
    unsafe { hand_out(vec![VALUE.load(Ordering::Relaxed)], out, out_len) }
}

/// Serves the served entry at `user` as a mod's served read finds it now.
unsafe extern "system" fn read_other(
    _base: *const u8,
    _base_len: u32,
    user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `user` is a served entry the test leaked for the process.
    let other = unsafe { &*user.cast::<Served>() };
    match read_served(other, vec![0], unarchived) {
        // SAFETY: `run_patches` passes `out` and `out_len` writable for the
        // call.
        Ok(bytes) => unsafe { hand_out(bytes, out, out_len) },
        Err(error) => crate::host::layer::Refusal::status(&error),
    }
}

/// An archive without the key: no game runs the tests.
fn unarchived(_key: &ResourceKey) -> Result<Vec<u8>, BuildResourceError> {
    Err(BuildResourceError::NotInArchive)
}

fn by_loader(apply: enshwick_core::abi::ResourcePatchFn) -> Patch {
    Patch::new(ModIndex::LOADER, apply, 0)
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
    let entry = Served::new(KEY, KEY, vec![Patch::new(owner, note_building, 0)], false);
    let base = [1u8; 4];
    let at = base.as_ptr().expose_provenance();
    let built = run_patches(&entry, at, base.len(), Some(&[owner]), Build::Game);
    assert!(built.is_some());
    assert!(SEEN_BUILDING.get(), "inside its build");
    assert!(!SEEN_SCRATCH.get(), "the game's build");
    assert!(BUILDING.with_borrow(Vec::is_empty), "after its build");
    assert!(!building(&KEY), "after its build");
    run_patches(&entry, at, base.len(), Some(&[owner]), Build::Scratch);
    assert!(SEEN_SCRATCH.get(), "a mod's served read");
    assert!(!scratch(), "after the read");
}

#[test]
fn a_failed_patch_in_a_served_read_is_skipped_and_the_rest_builds() {
    let (failing, building) = (ModIndex::new(3), ModIndex::new(4));
    let entry = Served::new(
        KEY,
        KEY,
        vec![
            Patch::new(failing, fail, 0),
            Patch::new(building, note_building, 0),
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
    assert_eq!(served_now(KEY), Err(BuildResourceError::Unsealed));
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
        Err(BuildResourceError::NotInArchive)
    );
}

#[test]
fn the_game_s_build_whose_every_patch_fails_serves_a_changed_key_as_archived_and_a_new_one_as_missing()
 {
    let base = [5u8; 3];
    let at = base.as_ptr().expose_provenance();
    let changed = Served::new(KEY, KEY, vec![by_loader(fail)], false);
    let mut state = State::default();
    // SAFETY: `base` holds three readable bytes, unchanged for the call.
    let (data, len) = unsafe { build_first(&changed, &mut state, at, base.len()) }.unwrap();
    assert_ne!(data, at, "a copy the loader keeps, not the game's bytes");
    // SAFETY: `build_first` kept `len` bytes at `data` for the process.
    let kept =
        unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(data), len) };
    assert_eq!(kept, base);
    assert!(state.failed, "no mod counts as serving it");
    let new = ResourceKey { part: 1, ..KEY };
    let added = Served::new(new, KEY, vec![by_loader(fail)], false);
    let mut state = State::default();
    // SAFETY: `base` holds three readable bytes, unchanged for the call.
    assert!(unsafe { build_first(&added, &mut state, at, base.len()) }.is_none());
    assert!(state.failed);
}

/// Whether the [`tail`] after the `len` bytes at `data` reads as zero.
///
/// # Safety
///
/// - `data` holds `len + tail(len)` readable bytes.
unsafe fn zero_tail_after(data: usize, len: usize) -> bool {
    // SAFETY: the caller guarantees `len + tail(len)` readable bytes at
    // `data`.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(data),
            len + tail(len),
        )
    };
    bytes[len..].iter().all(|&byte| byte == 0)
}

#[test]
fn a_large_buffer_keeps_a_sixteenth_of_its_length_as_zeros_and_a_small_one_a_page() {
    assert_eq!(tail(1 << 20), 1 << 16);
    assert_eq!(tail(1), PAGE);
}

#[test]
fn every_buffer_a_build_lends_or_takes_has_its_zeroed_tail_after_its_bytes() {
    let _build = enter_build(KEY);
    assert!(lend(5).is_some());
    let (mut out, mut out_len) = (core::ptr::null_mut::<u8>(), 0u32);
    // SAFETY: `out` and `out_len` are locals, writable for the call.
    let status = unsafe { hand_out(vec![7; 3], &raw mut out, &raw mut out_len) };
    assert_eq!((status, out_len), (Status::OK, 3));
    BUILDING.with_borrow(|frames| {
        let buffers = &frames.last().unwrap().buffers;
        for (buffer, len) in buffers.iter().zip([5, 3]) {
            assert!(buffer.holds(buffer.at(), len), "its own bytes");
            assert!(
                !buffer.holds(buffer.at(), len + 1),
                "no output reaches past them"
            );
            assert!(buffer.all.len() >= len + tail(len), "room for the tail");
            // SAFETY: the buffer is alive in the open build, and its
            // allocation holds `len + tail(len)` bytes, as checked above.
            assert!(unsafe { zero_tail_after(buffer.at(), len) });
        }
    });
}

#[test]
fn the_game_s_kept_build_and_its_failed_build_copy_read_zero_for_their_tail_past_their_end() {
    let base = [5u8; 3];
    let at = base.as_ptr().expose_provenance();
    for apply in [grow, note_building, fail, skip_first, short_of_buffer] {
        let entry = Served::new(KEY, KEY, vec![by_loader(apply)], false);
        let mut state = State::default();
        // SAFETY: `base` holds three readable bytes, unchanged for the call.
        let (data, len) = unsafe { build_first(&entry, &mut state, at, base.len()) }.unwrap();
        // SAFETY: `build_first` kept `len` bytes at `data` and at least
        // `tail(len)` zeroed bytes after them for the process.
        assert!(unsafe { zero_tail_after(data, len) });
    }
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
    rebuild_readers(list, &list[0], &mut vec![list[0].key]);
    for entry in &list[1..] {
        let (data, _) = entry.state.relock().built.unwrap();
        let mut held = [0u8];
        // SAFETY: `data` is the entry's first build, one byte kept for the
        // process.
        unsafe { crate::engine::memory::read_bytes(data, 0, &mut held) };
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
    unsafe { crate::engine::memory::read_bytes(data, 0, &mut held) };
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
    assert_eq!(rebuild_entry(list, &list[0]), Ok(()));
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
    rebuild_readers(list, &list[0], &mut vec![list[0].key]);
}

#[test]
fn a_rebuild_a_patch_asks_for_of_its_own_key_is_refused_instead_of_waited_on() {
    let list = read_and_reader();
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // The game's first build of the reader, whose patch asks to rebuild
        // the key it builds.
        let _game = list[1].state.relock();
        let _building = Building::enter(list[1].key, Build::Game);
        let _sent = done.send(rebuild_entry(list, &list[1]));
    });
    assert_eq!(
        finished
            .recv_timeout(std::time::Duration::from_secs(5))
            .ok(),
        Some(Err(BuildResourceError::Building)),
        "the patch's thread holds the key's build"
    );
}

/// Two served keys of one test whose patches ask whether icons show, with
/// what they serve: each test leaks its own, so tests running at once share
/// nothing.
struct Icons {
    /// The byte [`ask_icons`] serves as its icon.
    icon: AtomicU8,
    /// How often [`ask_icons_reading_first`] ran.
    reader_runs: AtomicUsize,
    list: [Served; 2],
}

/// `list` leaked for the process as [`Icons`] serving icon 1, with the
/// `user` of every patch pointing at it.
fn icons(list: [Served; 2]) -> &'static Icons {
    let icons = Box::into_raw(Box::new(Icons {
        icon: AtomicU8::new(1),
        reader_runs: AtomicUsize::new(0),
        list,
    }));
    let user = icons.expose_provenance();
    // SAFETY: `icons` is a fresh leak no reference points into yet; the
    // writes go through it, so the address the patches read stays valid.
    let list = unsafe { &mut (*icons).list };
    for entry in list {
        for patch in &mut entry.patches {
            patch.user = user;
        }
    }
    // SAFETY: `icons` is the leak made here, valid for the process, and the
    // loop's mutable borrow of it ended.
    unsafe { &*icons }
}

/// Asks, as a build that names an icon does, whether icons show among the
/// [`Icons`] at `user`, then serves its icon.
unsafe extern "system" fn ask_icons(
    _base: *const u8,
    _base_len: u32,
    user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `user` is the `Icons` the test leaked for the process.
    let icons = unsafe { &*user.cast::<Icons>() };
    note_icons_in(&icons.list);
    // SAFETY: `run_patches` passes `out` and `out_len` writable for the call.
    unsafe { hand_out(vec![icons.icon.load(Ordering::Relaxed)], out, out_len) }
}

/// As [`ask_icons`], but serves the first key of the [`Icons`] at `user` as
/// a served read finds it, counting its runs.
unsafe extern "system" fn ask_icons_reading_first(
    _base: *const u8,
    _base_len: u32,
    user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `user` is the `Icons` the test leaked for the process.
    let icons = unsafe { &*user.cast::<Icons>() };
    note_icons_in(&icons.list);
    icons.reader_runs.fetch_add(1, Ordering::Relaxed);
    match read_served(&icons.list[0], vec![0], unarchived) {
        // SAFETY: `run_patches` passes `out` and `out_len` writable for the
        // call.
        Ok(bytes) => unsafe { hand_out(bytes, out, out_len) },
        Err(error) => crate::host::layer::Refusal::status(&error),
    }
}

#[test]
fn a_key_whose_build_asked_for_icons_before_they_show_is_built_again_once_they_show() {
    let icons = icons([
        Served::new(KEY, KEY, vec![by_loader(ask_icons)], false),
        Served::new(
            ResourceKey { part: 1, ..KEY },
            KEY,
            vec![by_loader(grow)],
            false,
        ),
    ]);
    let list = &icons.list;
    build_each(list);
    assert!(list[0].reads_icons.load(Ordering::SeqCst));
    assert!(
        list[0].state.relock().base.is_some(),
        "its base is kept for the rebuild"
    );
    assert!(!list[1].reads_icons.load(Ordering::SeqCst));
    assert!(list[1].state.relock().base.is_none(), "nothing kept for it");
    icons.icon.store(2, Ordering::Relaxed);
    rebuild_icon_readers(list);
    assert_eq!(held(&list[0]), 2, "the icon written over the game's copy");
    assert_eq!(list[0].state.relock().built.map(|(_, len)| len), Some(1));
    assert!(
        list[0].state.relock().base.is_none(),
        "the base kept only for this rebuild is let go"
    );
}

#[test]
fn keys_that_asked_for_icons_and_read_each_other_are_each_built_again_once() {
    let icons = icons([
        Served::new(KEY, KEY, vec![by_loader(ask_icons)], false),
        Served::new(
            ResourceKey { part: 1, ..KEY },
            ResourceKey { part: 1, ..KEY },
            vec![by_loader(ask_icons_reading_first)],
            false,
        ),
    ]);
    let list = &icons.list;
    build_each(list);
    assert!(list.iter().all(|s| s.reads_icons.load(Ordering::SeqCst)));
    icons.icon.store(2, Ordering::Relaxed);
    let runs = icons.reader_runs.load(Ordering::Relaxed);
    rebuild_icon_readers(list);
    assert_eq!(
        icons.reader_runs.load(Ordering::Relaxed),
        runs + 1,
        "the reader built again once, as a reader, not again for its own ask"
    );
    assert_eq!(held(&list[1]), 2, "the reader takes the new icon");
    assert!(list[0].state.relock().base.is_none(), "read by none");
    assert!(
        list[1].state.relock().base.is_some(),
        "it reads the first key, so a rebuild of that still needs its base"
    );
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

/// Runs `build` through [`patched`] under `fuse` on the base `[1, 2]` in a
/// game build: the status and the bytes handed out.
fn patch_with(
    fuse: &Fuse,
    build: impl FnOnce(Vec<u8>) -> Option<Vec<u8>>,
) -> (Status, Option<Vec<u8>>) {
    let _build = enter_build(KEY);
    let base = [1u8, 2];
    let (mut out, mut out_len) = (core::ptr::null_mut::<u8>(), 0u32);
    // SAFETY: `base` holds two readable bytes, and `out` and `out_len` are
    // locals, writable for the call.
    let status = unsafe {
        patched(
            fuse,
            "a test patch",
            (base.as_ptr(), 2),
            (&raw mut out, &raw mut out_len),
            build,
        )
    };
    let made = (!out.is_null()).then(|| {
        // SAFETY: a patch that built handed out `out_len` bytes at `out`, in
        // a buffer of the build `_build` holds open.
        unsafe { core::slice::from_raw_parts(out, out_len as usize) }.to_vec()
    });
    (status, made)
}

#[test]
fn a_loader_patch_hands_out_nothing_outside_a_build() {
    let (mut out, mut out_len) = (core::ptr::null_mut::<u8>(), 0u32);
    // SAFETY: `out` and `out_len` are locals, writable for the call.
    let status = unsafe { hand_out(vec![1], &raw mut out, &raw mut out_len) };
    assert_eq!(status, Status::REFUSED);
    assert!(out.is_null());
    assert_eq!(LIVE.get(), 0, "nothing is kept for the process");
}

#[test]
fn an_output_outside_the_buffers_its_build_lent_fails_the_patch() {
    let (own, building) = (ModIndex::new(3), ModIndex::new(4));
    let entry = Served::new(
        KEY,
        KEY,
        vec![Patch::new(own, own_bytes, 0), Patch::new(building, grow, 0)],
        false,
    );
    let base = [1u8; 2];
    let at = base.as_ptr().expose_provenance();
    let built = run_patches(&entry, at, 2, Some(&[own, building]), Build::Scratch).unwrap();
    assert_eq!((built.len, built.by.clone()), (3, vec![building]));
    assert_eq!(built.into_bytes(), [1, 1, 1]);
    assert_eq!(LIVE.get(), 0);
    let alone = Served::new(KEY, KEY, vec![by_loader(own_bytes)], false);
    assert!(run_patches(&alone, at, 2, None, Build::Scratch).is_none());
}

std::thread_local! {
    /// How often [`faulted`] ran on the test's thread.
    static FAULTED_RUNS: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };
}

/// Answers as a patch whose builder panicked.
unsafe extern "system" fn faulted(
    _base: *const u8,
    _base_len: u32,
    _user: *mut c_void,
    _out: *mut *mut u8,
    _out_len: *mut u32,
) -> Status {
    FAULTED_RUNS.set(FAULTED_RUNS.get() + 1);
    Status::FAULTED
}

#[test]
fn a_patch_whose_builder_panicked_runs_again_only_in_a_rebuild() {
    let entry = Served::new(KEY, KEY, vec![by_loader(faulted), by_loader(grow)], false);
    let base = [1u8; 2];
    let at = base.as_ptr().expose_provenance();
    for kind in [Build::Scratch, Build::Game] {
        let built = run_patches(&entry, at, 2, None, kind).unwrap();
        assert_eq!(built.by, vec![ModIndex::LOADER], "{kind:?}");
    }
    assert_eq!(FAULTED_RUNS.get(), 1, "a panicked builder runs once");
    assert!(!entry.patches[0].on.load(Ordering::Acquire));
    assert!(entry.patches[1].on.load(Ordering::Acquire));
    let rebuilt = run_patches(&entry, at, 2, Some(&[ModIndex::LOADER]), Build::Game).unwrap();
    assert_eq!(rebuilt.by, vec![ModIndex::LOADER]);
    assert_eq!(
        FAULTED_RUNS.get(),
        2,
        "a rebuild runs every patch the game's build applied"
    );
}

#[test]
fn a_loader_patch_that_panics_turns_its_feature_off_and_refuses_from_then_on() {
    static STACKS: Fuse = Fuse::new("test stacks");
    let grown = patch_with(&STACKS, |mut bytes| {
        bytes.push(3);
        Some(bytes)
    });
    assert_eq!(grown, (Status::OK, Some(vec![1, 2, 3])));
    assert_eq!(patch_with(&STACKS, |_| None), (Status::REFUSED, None));
    assert!(!STACKS.is_off(), "a patch that builds nothing is no fault");
    let broken = patch_with(&STACKS, |_| panic!("broken stack"));
    assert_eq!(broken, (Status::FAULTED, None));
    assert!(STACKS.is_off());
    let mut ran = false;
    let after = patch_with(&STACKS, |bytes| {
        ran = true;
        Some(bytes)
    });
    assert_eq!(after, (Status::FAULTED, None));
    assert!(!ran, "an off feature's patch never runs");
    let listed = crate::engine::diag::parts_off();
    assert!(
        listed
            .iter()
            .any(|p| p.part == "test stacks" && p.reason.contains("broken stack")),
        "{listed:?}"
    );
}

#[test]
fn a_build_without_rekeys_leaves_out_the_rekey_patches_and_runs_the_rest() {
    let rekey = Patch {
        rekey: Some(ResourceKey { part: 1, ..KEY }),
        ..by_loader(grow)
    };
    let entry = Served::new(KEY, KEY, vec![rekey, by_loader(grow)], false);
    let base = [1u8; 4];
    let at = base.as_ptr().expose_provenance();
    let len = |kind| run_patches(&entry, at, base.len(), None, kind).map(|built| built.len);
    assert_eq!(len(Build::Scratch), Some(6), "a served read runs both");
    assert_eq!(len(Build::Unkeyed), Some(5), "the re-key left out");
}
