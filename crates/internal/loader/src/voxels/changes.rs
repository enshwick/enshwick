//! What mods hear of the world's voxel edits. Every write of the server's
//! voxel editor, a player's tool or building, the game's own and the
//! loader's, passes the editor's commit with the whole new image of a box
//! aligned to its bricks (servers 399 and 26430253, client 26430625, on the
//! game thread). The loader's link reads the box before the game writes it,
//! keeps the voxels that differ, and raises them after the step as
//! `Event::VOXELS_CHANGED`, naming the mod whose call made the write. It
//! hears only the editor of the running server, noted after each step, so
//! a client's own copy of the world raises nothing.

use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
use enshwick::abi::{Event, Header, VoxelEditData};
use enshwick::abi::{ModHandle, VoxelChange};

#[cfg(windows)]
use crate::chains::Next;
use crate::fuse::Fuse;
#[cfg(windows)]
use crate::host::Loader;
use crate::locks::Relock;
#[cfg(windows)]
use crate::pinned;
use crate::wire::Drops;

/// Changed voxels waiting for the end of the step at most, about 5 MB;
/// writes past it are dropped and counted.
const MAX_WAITING: usize = 1 << 18;

/// One write of the editor and the voxels it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    by: ModHandle,
    min: [u32; 3],
    size: [u32; 3],
    changes: Vec<VoxelChange>,
}

/// The game's commit: the editor, the box's new words, its size and its
/// lowest corner, in voxels at the finest level (server 399 and 26430253).
#[cfg(windows)]
type CommitFn = unsafe extern "system" fn(
    editor: *mut u8,
    words: *const super::Words,
    size: *const [u32; 3],
    min: *const [u32; 3],
) -> u64;

/// What the link reads a box's image before the write with.
#[cfg(windows)]
#[derive(Clone, Copy)]
struct Layout {
    read_box: super::ReadBoxFn,
    level: u32,
    /// The editor's `voxel.store` offset: the read dereferences the store
    /// unconditionally, so a commit before the store is attached is skipped.
    store: usize,
}

/// The most voxels a commit's box may hold for the link to read it, 128³:
/// past the box of the largest terrain edit the API takes, aligned to its
/// bricks. A larger box is not raised and counted.
const MAX_BOX: u64 = 1 << 21;

/// The voxels of a commit's box of `size` from `min` whose buffer holds
/// `held` words; `None` for an empty box, one past
/// [`Voxel::END`](enshwick::abi::Voxel::END), one over [`MAX_BOX`] or a
/// buffer that does not hold it.
fn box_count(min: [u32; 3], size: [u32; 3], held: u64) -> Option<usize> {
    let mut count = 1u64;
    for axis in 0..3 {
        let end = min[axis].checked_add(size[axis])?;
        if size[axis] == 0 || end > enshwick::abi::Voxel::END {
            return None;
        }
        count = count.checked_mul(u64::from(size[axis]))?;
    }
    if count > MAX_BOX || count > held {
        return None;
    }
    usize::try_from(count).ok()
}

/// The state of the voxel edits: the link, the server's editor it hears,
/// and the writes waiting for the end of the step.
struct Edits {
    fuse: Fuse,
    #[cfg(windows)]
    next: Next<CommitFn>,
    #[cfg(windows)]
    layout: OnceLock<Layout>,
    /// The server's voxel editor as of the last step, 0 while no server
    /// runs: a client's own copy of the world writes through another
    /// editor, whose writes are no server's edits.
    editor: AtomicUsize,
    waiting: Mutex<(Vec<Edit>, usize)>,
    pending: AtomicBool,
    dropped: Drops,
}

static EDITS: Edits = Edits {
    fuse: Fuse::new("voxel edits"),
    // SAFETY: `CommitFn` is the signature of `voxel.commit`, and this slot is
    // the `next` of the one link on it, `commit`, the only caller of `get`.
    #[cfg(windows)]
    next: unsafe { Next::new() },
    #[cfg(windows)]
    layout: OnceLock::new(),
    editor: AtomicUsize::new(0),
    waiting: Mutex::new((Vec::new(), 0)),
    pending: AtomicBool::new(false),
    dropped: Drops::new(),
};

thread_local! {
    /// The mod whose call runs on this thread, whose writes happen on the
    /// thread of the call; none for the game's own.
    static BY: Cell<ModHandle> = const { Cell::new(ModHandle::NONE) };

    /// The image of a commit's box before the write, kept between commits
    /// so the game's write allocates only for the voxels that changed.
    #[cfg(windows)]
    static BEFORE: core::cell::RefCell<Vec<u16>> = const { core::cell::RefCell::new(Vec::new()) };
}

/// Runs `work`, a call of mod `by` that may write voxels on this thread, so
/// the writes it makes name that mod; nested calls name the innermost. The
/// mod before is back once `work` returns or panics.
pub(crate) fn by<R>(by: ModHandle, work: impl FnOnce() -> R) -> R {
    struct Restore(ModHandle);

    impl Drop for Restore {
        fn drop(&mut self) {
            BY.set(self.0);
        }
    }

    let _outer = Restore(BY.replace(by));
    work()
}

/// The mod whose call writes on this thread now; none for the game's own.
fn current() -> ModHandle {
    BY.get()
}

/// The voxels that differ between `before` and `after`, two images of the
/// box of `size` from `min`, x fastest, then y, then z, in that order.
#[expect(
    clippy::cast_possible_truncation,
    reason = "an offset lies inside the box, whose sides are `u32`s"
)]
pub(crate) fn changes(
    min: [u32; 3],
    size: [u32; 3],
    before: &[u16],
    after: &[u16],
) -> Vec<VoxelChange> {
    let [width, height, _] = size.map(|axis| axis as usize);
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (was, is))| was != is)
        .map(|(index, (&was, &is))| {
            let offset = [
                index % width,
                index / width % height,
                index / (width * height),
            ];
            VoxelChange {
                at: [0, 1, 2].map(|axis| min[axis] + offset[axis] as u32),
                before: super::decode(was),
                after: super::decode(is),
            }
        })
        .collect()
}

/// Keeps `edit` for the end of the step, unless the waiting writes would
/// pass [`MAX_WAITING`] changed voxels.
fn queue(edit: Edit) {
    let mut waiting = EDITS.waiting.relock();
    let (edits, voxels) = &mut *waiting;
    if *voxels + edit.changes.len() > MAX_WAITING {
        drop(waiting);
        dropped();
        return;
    }
    *voxels += edit.changes.len();
    edits.push(edit);
    EDITS.pending.store(true, Ordering::Release);
}

/// Counts a write not raised and logs the first and every hundredth.
#[cold]
fn dropped() {
    if let Some(total) = EDITS.dropped.add(1) {
        crate::log!(
            WARN,
            "voxel edits: {total} writes not raised to mods, past {MAX_WAITING} changed voxels in a step, or in a box the loader could not read"
        );
    }
}

/// The writes of the step, waiting; empties the queue.
fn take() -> Vec<Edit> {
    if !EDITS.pending.load(Ordering::Acquire) {
        return Vec::new();
    }
    let mut waiting = EDITS.waiting.relock();
    EDITS.pending.store(false, Ordering::Release);
    waiting.1 = 0;
    core::mem::take(&mut waiting.0)
}

/// Raises the writes of the step and notes the server's editor for the
/// next; on the game thread, outside the game's systems.
#[cfg(windows)]
pub(crate) fn after_step(loader: &Loader) {
    if EDITS.layout.get().is_some() {
        let editor = super::editor(loader).unwrap_or(0);
        EDITS.editor.store(editor, Ordering::Release);
    }
    for edit in take() {
        let data = VoxelEditData {
            header: Header::of::<VoxelEditData>(),
            by: edit.by,
            min: edit.min,
            size: edit.size,
            changes: edit.changes.as_ptr(),
            count: edit.changes.len() as u64,
        };
        loader.emit(Event::VOXELS_CHANGED, None, (&raw const data).cast());
    }
}

/// Drops the writes waiting for a hosted game that stops, and its editor.
pub(crate) fn forget() {
    EDITS.editor.store(0, Ordering::Release);
    drop(take());
}

/// Links the editor's commit when a mod listens for voxel edits; without it
/// they stay off, and the log and the Mods screen say why.
#[cfg(windows)]
pub(crate) fn link(loader: &'static Loader) {
    if !loader.subscribed(Event::VOXELS_CHANGED) {
        return;
    }
    let feature = "voxel edits";
    let read_box = loader
        .address(pinned::voxel::READ_BOX)
        .and_then(|address| usize::try_from(address).ok());
    let Some(read_box) = read_box else {
        loader.feature_off(
            feature,
            pinned::voxel::READ_BOX.id(),
            "this build has no box read",
        );
        return;
    };
    let (Some(store), Ok(finest)) = (loader.offset(pinned::voxel::STORE), super::finest(loader))
    else {
        let missing = [
            pinned::voxel::STORE,
            pinned::voxel::FINEST_LEVEL,
            pinned::voxel::GRIDS,
            pinned::voxel::GRID_PAGES,
            pinned::voxel::GRID_STRIDE,
            pinned::voxel::GRID_DEPTH,
        ]
        .into_iter()
        .find(|&id| loader.offset(id).is_none())
        .unwrap_or(pinned::voxel::FINEST_LEVEL);
        loader.feature_off(feature, missing.id(), "this build has no voxel grid layout");
        return;
    };
    // SAFETY: the table's `voxel.read_box` is the address of the game
    // function with the `ReadBoxFn` signature on this build, in the image,
    // which stays mapped.
    let read_box = unsafe { core::mem::transmute::<usize, super::ReadBoxFn>(read_box) };
    let target = crate::host::Target::Pinned(pinned::voxel::COMMIT);
    // The link passes every write through until the layout is set, so
    // `after_step` reads no editor while the link is refused.
    if loader.link_feature(
        feature,
        target,
        &EDITS.next,
        commit,
        enshwick::abi::HookFlags::NONE,
    ) && EDITS
        .layout
        .set(Layout {
            read_box,
            level: finest.level,
            store,
        })
        .is_ok()
    {
        crate::log!(INFO, "voxel edits on");
    }
}

/// The link on the editor's commit.
///
/// # Safety
///
/// The game's own arguments to `voxel.commit`, on the thread it called it
/// on: `editor` its voxel editor, `words` its buffer of the box's new words,
/// `size` and `min` the box.
#[cfg(windows)]
unsafe extern "system" fn commit(
    editor: *mut u8,
    words: *const super::Words,
    size: *const [u32; 3],
    min: *const [u32; 3],
) -> u64 {
    let next = EDITS.next.get();
    let server = EDITS.editor.load(Ordering::Acquire);
    if let Some(&layout) = EDITS
        .layout
        .get()
        .filter(|_| server != 0 && editor.addr() == server)
    {
        // SAFETY: the game's own arguments, by this function's `# Safety`,
        // before the game's write.
        let _noted = EDITS.fuse.run("the voxels a write changes", || unsafe {
            note(layout, editor, words, size, min);
        });
    }
    // SAFETY: this link runs only on `voxel.commit`, so the slot holds the
    // next function of that signature; the game's arguments pass unchanged.
    unsafe { next(editor, words, size, min) }
}

/// Queues the voxels the commit's new image of the box `size` and `min`
/// give differs in from the box as the game's box read has it now; a box
/// it cannot read is counted, not raised.
///
/// # Safety
///
/// As [`commit`], before the game's write: `words` still holds the box's
/// new image, the caller's for the call.
#[cfg(windows)]
unsafe fn note(
    layout: Layout,
    editor: *mut u8,
    words: *const super::Words,
    size: *const [u32; 3],
    min: *const [u32; 3],
) {
    use enshwick::raw::{read_at, read_ptr_at};

    if editor.is_null() || words.is_null() || size.is_null() || min.is_null() {
        return;
    }
    // SAFETY: `editor` is the non-null voxel editor the commit writes
    // through, by this function's `# Safety`; the table's `voxel.store`
    // offset holds its store pointer.
    let store: usize = unsafe { crate::memory::read(editor.addr(), layout.store) };
    if store == 0 {
        return;
    }
    // SAFETY: `size` is non-null (checked above) and is the commit's box
    // size, three `u32`s, by this function's `# Safety`.
    let size: [u32; 3] = unsafe { read_at(size.cast(), 0) };
    // SAFETY: `min` is non-null (checked above) and is the commit's lowest
    // corner, three `u32`s, by this function's `# Safety`.
    let min: [u32; 3] = unsafe { read_at(min.cast(), 0) };
    // SAFETY: `words` is non-null (checked above) and is the commit's buffer,
    // a data pointer and a count.
    let held: u64 = unsafe { read_at(words.cast(), core::mem::offset_of!(super::Words, count)) };
    // SAFETY: `words` is non-null (checked above) and is the commit's buffer,
    // whose data pointer lies at offset 0.
    let data = unsafe { read_ptr_at::<u16>(words.cast(), 0) };
    let Some(count) = box_count(min, size, held).filter(|_| !data.is_null()) else {
        dropped();
        return;
    };
    BEFORE.with(|before| {
        let Ok(mut image) = before.try_borrow_mut() else {
            dropped();
            return;
        };
        image.clear();
        image.resize(count, 0);
        let mut out = super::Words {
            data: image.as_mut_ptr(),
            count: count as u64,
        };
        // SAFETY: `read_box` is the table's `voxel.read_box`, called with the
        // editor the commit writes through, whose store is non-zero (checked
        // above), on the commit's own thread: the game's read inside the
        // commit answers at once there (server 26430253, the loader's
        // writes; client 26430625, a player's dig and add). It writes the
        // product of `size` words at `out.data`, which `box_count` gives
        // with overflow checks and `image` holds, for a box below
        // `Voxel::END` on every axis, where its signed loops do not wrap.
        let resident = unsafe {
            (layout.read_box)(
                &raw mut out,
                &raw const size,
                editor,
                layout.level,
                &raw const min,
            )
        };
        if resident == 0 {
            dropped();
            return;
        }
        // SAFETY: the buffer at the non-null `data` holds `held` words, at
        // least `count` (checked by `box_count`), the caller's for the call,
        // and the game has not written it yet.
        let after = unsafe { core::slice::from_raw_parts(data.cast_const(), count) };
        let changes = changes(min, size, &image, after);
        if !changes.is_empty() {
            queue(Edit {
                by: current(),
                min,
                size,
                changes,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rock(fill: u8) -> u16 {
        u16::from_le_bytes([9, fill])
    }

    #[test]
    fn only_the_voxels_that_differ_are_changes_at_their_place() {
        let before = [rock(0xff); 2 * 3 * 4];
        let mut after = before;
        after[0] = 0;
        after[1 + 2 * (2 + 3 * 3)] = rock(0x40);
        let found = changes([100, 50, 200], [2, 3, 4], &before, &after);
        assert_eq!(
            found.iter().map(|c| c.at).collect::<Vec<_>>(),
            [[100, 50, 200], [101, 52, 203]]
        );
        assert_eq!(found[0].after, super::super::decode(0));
        assert_eq!(found[1].before.fill, 0xff);
        assert_eq!(found[1].after.fill, 0x40);
    }

    #[test]
    fn a_box_past_the_api_read_limit_is_read_up_to_its_own_limit() {
        let big = [72, 72, 72];
        assert!(enshwick::abi::Voxel::read_count([0; 3], big).is_none());
        assert_eq!(box_count([8, 0, 8], big, u64::MAX), Some(72 * 72 * 72));
        assert_eq!(box_count([0; 3], [128; 3], MAX_BOX), Some(1 << 21));
    }

    #[test]
    fn a_box_the_link_cannot_read_is_refused() {
        assert_eq!(
            box_count([0; 3], [129, 128, 128], u64::MAX),
            None,
            "over 128³"
        );
        assert_eq!(box_count([0; 3], [8; 3], 511), None, "the buffer is short");
        assert_eq!(box_count([0; 3], [0, 8, 8], u64::MAX), None, "an empty box");
        assert_eq!(
            box_count([enshwick::abi::Voxel::END - 4, 0, 0], [8; 3], u64::MAX),
            None,
            "past the end"
        );
        assert_eq!(box_count([u32::MAX, 0, 0], [8; 3], u64::MAX), None);
    }

    #[test]
    fn a_write_that_changes_nothing_has_no_changes() {
        let image = [rock(0x80); 8];
        assert!(changes([0; 3], [2; 3], &image, &image).is_empty());
    }

    #[test]
    fn a_write_names_the_mod_only_inside_its_call_on_its_thread() {
        assert_eq!(current(), ModHandle::NONE);
        by(ModHandle(3), || {
            assert_eq!(current(), ModHandle(3));
            by(ModHandle(5), || assert_eq!(current(), ModHandle(5)));
            assert_eq!(current(), ModHandle(3));
        });
        assert_eq!(current(), ModHandle::NONE);
    }

    #[test]
    fn a_call_that_panics_leaves_the_game_s_writes_naming_no_mod() {
        let caught = std::panic::catch_unwind(|| by(ModHandle(3), || panic!("a broken edit")));
        assert!(caught.is_err());
        assert_eq!(current(), ModHandle::NONE);
    }

    fn edit(voxels: usize) -> Edit {
        Edit {
            by: ModHandle::NONE,
            min: [0; 3],
            size: [64; 3],
            changes: vec![VoxelChange::default(); voxels],
        }
    }

    fn waiting() -> Vec<usize> {
        take().iter().map(|e| e.changes.len()).collect()
    }

    #[test]
    fn writes_past_the_waiting_limit_are_dropped_and_the_queue_empties_once_taken() {
        forget();
        queue(edit(MAX_WAITING - 1));
        queue(edit(2));
        queue(edit(1));
        assert_eq!(waiting(), [MAX_WAITING - 1, 1]);
        assert!(take().is_empty());
        queue(edit(3));
        queue(edit(MAX_WAITING - 3));
        assert_eq!(waiting(), [3, MAX_WAITING - 3], "the limit itself fits");
        queue(edit(MAX_WAITING));
        assert_eq!(
            waiting(),
            [MAX_WAITING],
            "the count starts again after a take"
        );
    }

    #[test]
    fn a_game_that_stops_drops_its_waiting_writes() {
        queue(edit(4));
        forget();
        assert!(take().is_empty());
    }
}
