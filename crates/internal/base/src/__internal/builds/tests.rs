use core::cell::RefCell;

use super::*;

/// A key of the table: its index.
type At = usize;

/// A build running: its key, whether a served read, whether it read another
/// served key, and the buffers it lent.
type Frame = (At, bool, bool, Vec<Box<[u8]>>);

/// What the game holds of a key: the base kept, the bytes and who built them.
type HeldBytes = (Option<Vec<u8>>, Vec<u8>, Vec<u8>);

/// What a patch of the table does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Does {
    /// Adds one to each byte.
    Add,
    /// Answers this status.
    Answer(Status),
    /// Answers OK with bytes no buffer holds.
    Stray,
    /// Reads the served key at this index.
    Read(At),
    /// Re-keys to the served key at this index: answers it as served.
    Rekey(At),
}

#[derive(Clone, Copy, Debug)]
struct Patch {
    owner: u8,
    does: Does,
    on: bool,
}

#[derive(Debug)]
struct Entry {
    key: ResourceKey,
    base: ResourceKey,
    patches: Vec<Patch>,
    asked: bool,
    /// The base kept, the bytes the game holds and who built them.
    held: Option<HeldBytes>,
    read_by: Vec<At>,
}

/// An in-memory side: buffers live in the frames, as both real sides keep
/// them.
#[derive(Default)]
struct Table {
    entries: RefCell<Vec<Entry>>,
    sealed: bool,
    archive: Vec<(ResourceKey, Vec<u8>)>,
    frames: RefCell<Vec<Frame>>,
    failed: RefCell<Vec<(At, u8, PatchFailure)>>,
    logged: RefCell<Vec<Unbuilt>>,
}

const fn key(n: u8) -> ResourceKey {
    ResourceKey {
        guid: crate::content::Guid([n; 16]),
        type_hash: 1,
        part: 0,
    }
}

fn entry(n: u8, patches: &[(u8, Does)]) -> Entry {
    Entry {
        key: key(n),
        base: key(n),
        patches: patches
            .iter()
            .map(|&(owner, does)| Patch {
                owner,
                does,
                on: true,
            })
            .collect(),
        asked: false,
        held: None,
        read_by: Vec::new(),
    }
}

impl Table {
    fn lend(&self, bytes: Vec<u8>) -> View {
        let mut frames = self.frames.borrow_mut();
        let buffers = &mut frames.last_mut().expect("inside a build").3;
        buffers.push(bytes.into_boxed_slice());
        let lent = buffers.last().expect("pushed");
        View {
            at: lent.as_ptr().expose_provenance(),
            len: u32::try_from(lent.len()).expect("small"),
        }
    }

    fn bytes(view: View) -> Vec<u8> {
        let at = core::ptr::with_exposed_provenance::<u8>(view.at);
        // SAFETY: every view the tests make lies in a live buffer or base.
        unsafe { core::slice::from_raw_parts(at, view.len as usize) }.to_vec()
    }
}

// SAFETY: `apply` answers only outputs `lend` put in the running frame,
// whose buffers live until `leave` pops it.
unsafe impl Patches for Table {
    type Key = At;
    type Owner = u8;
    type Patch = Patch;
    type Frame = ();
    type Bytes = Vec<u8>;

    fn building(&self, key: At) -> bool {
        self.frames.borrow().iter().any(|frame| frame.0 == key)
    }

    fn patches(&self, key: At) -> impl Iterator<Item = Patch> {
        self.entries.borrow()[key].patches.clone().into_iter()
    }

    fn owner(&self, patch: &Patch) -> u8 {
        patch.owner
    }

    fn on(&self, patch: &Patch) -> bool {
        patch.on
    }

    fn rekey(&self, patch: &Patch) -> Option<ResourceKey> {
        match patch.does {
            Does::Rekey(new) => Some(self.entries.borrow()[new].key),
            _ => None,
        }
    }

    fn enter(&self, key: At, build: Build) {
        self.frames
            .borrow_mut()
            .push((key, build != Build::Game, false, Vec::new()));
    }

    unsafe fn apply(&self, patch: &Patch, input: View, _rebuild: bool) -> (Status, Option<View>) {
        match patch.does {
            Does::Add => {
                let added = Self::bytes(input).iter().map(|b| b + 1).collect();
                (Status::OK, Some(self.lend(added)))
            }
            Does::Answer(status) => (status, None),
            Does::Stray => (Status::OK, None),
            Does::Read(other) | Does::Rekey(other) => {
                let other = self.entries.borrow()[other].key;
                let read = read_served(self, &other);
                (Status::OK, read.ok().map(|bytes| self.lend(bytes)))
            }
        }
    }

    fn failed(&self, key: At, patch: &Patch, why: PatchFailure) {
        self.failed.borrow_mut().push((key, patch.owner, why));
    }

    fn leave(&self, (): (), last: Option<View>) -> (bool, Option<Vec<u8>>) {
        let bytes = last.map(Self::bytes);
        let frame = self.frames.borrow_mut().pop().expect("entered");
        (frame.2, bytes)
    }

    fn unbuilt(&self, _key: At, why: Unbuilt) {
        self.logged.borrow_mut().push(why);
    }
}

// SAFETY: `base` answers the bytes of the kept `Vec`, which nothing changes
// while it lives.
unsafe impl ServedTable for Table {
    type Kept = Vec<u8>;

    fn sealed(&self) -> bool {
        self.sealed
    }

    fn find(&self, key: &ResourceKey) -> Result<At, ResourceKey> {
        self.entries
            .borrow()
            .iter()
            .position(|e| e.key == *key)
            .ok_or(*key)
    }

    fn served_by(&self, owner: u8, key: &ResourceKey) -> Option<At> {
        self.entries
            .borrow()
            .iter()
            .position(|e| e.key == *key && e.patches.iter().any(|p| p.owner == owner))
    }

    fn keys(&self, key: At) -> (ResourceKey, ResourceKey) {
        let entries = self.entries.borrow();
        (entries[key].key, entries[key].base)
    }

    fn archived(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        self.archive
            .iter()
            .find(|(have, _)| have == key)
            .map(|(_, bytes)| bytes.clone())
    }

    fn keep_base(&self, key: At) {
        self.entries.borrow_mut()[key].asked = true;
    }

    fn keeps_base(&self, key: At) -> KeepBase {
        KeepBase {
            asked: self.entries.borrow()[key].asked,
            ..KeepBase::default()
        }
    }

    fn note_read(&self, key: At) {
        let readers: Vec<At> = self
            .frames
            .borrow_mut()
            .iter_mut()
            .filter(|frame| !frame.1)
            .map(|frame| {
                frame.2 = true;
                frame.0
            })
            .collect();
        let mut entries = self.entries.borrow_mut();
        for reader in readers {
            if !entries[key].read_by.contains(&reader) {
                entries[key].read_by.push(reader);
            }
        }
    }

    fn held(&self, key: At) -> Option<Held<Vec<u8>, u8>> {
        let entries = self.entries.borrow();
        let (base, held, by) = entries[key].held.as_ref()?;
        Some(Held {
            base: base.clone(),
            len: held.len(),
            by: by.clone(),
        })
    }

    fn base(&self, kept: &Vec<u8>) -> (usize, usize) {
        (kept.as_ptr().expose_provenance(), kept.len())
    }

    fn write(&self, key: At, _kept: Vec<u8>, built: Built<Vec<u8>, u8>) -> usize {
        let mut entries = self.entries.borrow_mut();
        let held = &mut entries[key].held.as_mut().expect("held").1;
        let changed = held
            .iter()
            .zip(&built.bytes)
            .filter(|(a, b)| a != b)
            .count();
        *held = built.bytes;
        changed
    }

    fn readers(&self, key: At) -> Vec<At> {
        self.entries.borrow()[key].read_by.clone()
    }

    fn to_vec(&self, built: Built<Vec<u8>, u8>) -> Vec<u8> {
        built.bytes
    }
}

fn table(entries: Vec<Entry>) -> Table {
    Table {
        entries: RefCell::new(entries),
        sealed: true,
        ..Table::default()
    }
}

fn view(bytes: &[u8]) -> (usize, usize) {
    (bytes.as_ptr().expose_provenance(), bytes.len())
}

/// [`super::build`] over a base the test holds.
fn build(
    table: &Table,
    key: At,
    base: (usize, usize),
    kind: Build,
    only: Option<&[u8]>,
) -> Result<Built<Vec<u8>, u8>, BuildResourceError> {
    // SAFETY: every base the tests pass is a `view` of bytes they hold for
    // the call, or over 4 GiB, which `build` refuses before reading.
    unsafe { super::build(table, key, base, kind, only) }
}

/// [`super::build_first`] over a base the test holds.
fn build_first(table: &Table, key: At, base: (usize, usize)) -> FirstBuild<Vec<u8>, u8> {
    // SAFETY: every base the tests pass is a `view` of bytes they hold for
    // the call.
    unsafe { super::build_first(table, key, base) }
}

#[test]
fn a_rebuild_runs_the_patches_of_the_mods_that_built_the_bytes_on_or_off() {
    assert!(patch_runs(Some(&[1, 2][..]), &2, || false));
    assert!(!patch_runs(Some(&[1][..]), &2, || true));
}

#[test]
fn a_first_build_or_served_read_runs_only_patches_still_on() {
    assert!(patch_runs::<u32>(None, &2, || true));
    assert!(!patch_runs::<u32>(None, &2, || false));
}

#[test]
fn the_first_build_keeps_its_base_for_any_of_the_three_reasons() {
    let none = KeepBase::default();
    assert!(!none.keeps());
    for reason in [
        KeepBase {
            asked: true,
            ..none
        },
        KeepBase {
            read_served: true,
            ..none
        },
        KeepBase {
            asked_icons: true,
            ..none
        },
    ] {
        assert!(reason.keeps(), "{reason:?}");
    }
}

#[test]
fn each_outcome_answers_its_status() {
    let answers = [
        (BuildResourceError::NotServed, Status::BAD_ARGS),
        (BuildResourceError::Building, Status::BUILDING),
        (BuildResourceError::NoBase, Status::MISSING),
        (BuildResourceError::NotBuilt, Status::MISSING),
        (BuildResourceError::SizeChanged, Status::REFUSED),
        (BuildResourceError::Unsealed, Status::REFUSED),
        (BuildResourceError::NotInArchive, Status::MISSING),
    ];
    for (error, status) in answers {
        assert_eq!(error.status(), status, "{error:?}");
    }
}

#[test]
fn the_games_first_build_runs_while_a_served_read_of_its_key_runs_on_this_thread() {
    let table = table(vec![entry(1, &[(1, Does::Add)])]);
    table.enter(0, Build::Scratch);
    let first = build_first(&table, 0, view(&[5]));
    drop(table.leave((), None));
    assert_eq!(first.built.expect("built").bytes, [6]);
    assert!(first.holds);
}

#[test]
fn patches_build_in_order_on_the_output_before_and_record_who_built() {
    let table = table(vec![entry(1, &[(1, Does::Add), (2, Does::Add)])]);
    let built = build(&table, 0, view(&[5, 6]), Build::Game, None).expect("built");
    assert_eq!(
        (built.bytes, built.by, built.reads),
        (vec![7, 8], vec![1, 2], false)
    );
}

#[test]
fn a_failed_patch_fails_in_a_held_build_and_is_skipped_in_a_scratch_one() {
    let patches = [
        (1, Does::Answer(Status::REFUSED)),
        (2, Does::Stray),
        (3, Does::Answer(Status::FAULTED)),
        (4, Does::Add),
    ];
    let table = table(vec![entry(1, &patches)]);
    let built = build(&table, 0, view(&[0]), Build::Game, None).expect("built");
    assert_eq!(built.by, [4]);
    assert_eq!(
        table.failed.take(),
        [
            (0, 1, PatchFailure::Refused(Status::REFUSED)),
            (0, 2, PatchFailure::Refused(Status::BAD_ARGS)),
            (0, 3, PatchFailure::Panicked),
        ],
        "bytes no buffer holds are a bad output"
    );
    build(&table, 0, view(&[0]), Build::Scratch, None).expect("built");
    assert_eq!(
        table.failed.take(),
        [(0, 3, PatchFailure::Panicked)],
        "a scratch build reports only the panic"
    );
}

#[test]
fn a_build_runs_the_patches_still_on_and_a_rebuild_those_of_its_builders() {
    let table = table(vec![entry(1, &[(1, Does::Add), (2, Does::Add)])]);
    table.entries.borrow_mut()[0].patches[0].on = false;
    let first = build(&table, 0, view(&[0]), Build::Game, None).expect("built");
    assert_eq!(first.by, [2]);
    let rebuilt = build(&table, 0, view(&[0]), Build::Game, Some(&[1])).expect("built");
    assert_eq!(rebuilt.by, [1], "on or off");
}

#[test]
fn nothing_built_and_a_huge_base_are_outcomes() {
    let table = table(vec![entry(1, &[(1, Does::Answer(Status::REFUSED))])]);
    assert_eq!(
        build(&table, 0, view(&[0]), Build::Scratch, None).err(),
        Some(BuildResourceError::NotBuilt)
    );
    let huge = usize::try_from(u64::from(u32::MAX) + 1).expect("64-bit");
    assert_eq!(
        build(&table, 0, (0, huge), Build::Game, None).err(),
        Some(BuildResourceError::NotBuilt)
    );
    assert_eq!(table.logged.take(), [Unbuilt::TooLarge]);
}

#[test]
fn the_first_build_keeps_its_base_when_asked_or_reading_and_holds_a_failed_vanilla_key() {
    let mut new = entry(2, &[(1, Does::Answer(Status::REFUSED))]);
    new.base = key(1);
    let mut table = table(vec![
        entry(1, &[(1, Does::Add)]),
        new,
        entry(3, &[(1, Does::Read(0))]),
        entry(4, &[(1, Does::Answer(Status::REFUSED))]),
    ]);
    table.archive = vec![(key(1), vec![0])];
    let plain = build_first(&table, 0, view(&[0]));
    assert!(plain.built.is_ok() && plain.holds && !plain.keep_base);
    table.keep_base(0);
    assert!(build_first(&table, 0, view(&[0])).keep_base, "asked");
    let added = build_first(&table, 1, view(&[0]));
    assert!(
        added.built.is_err() && !added.holds,
        "a new key no patch built"
    );
    let reads = build_first(&table, 2, view(&[0]));
    assert!(reads.keep_base, "read another served key");
    let vanilla = build_first(&table, 3, view(&[0]));
    assert!(vanilla.built.is_err() && vanilla.holds && !vanilla.keep_base);
}

#[test]
fn a_rebuild_is_asked_during_init_and_answers_each_outcome_after() {
    let mut table = table(vec![entry(1, &[(1, Does::Add)])]);
    table.sealed = false;
    assert_eq!(
        rebuild(&table, 2, &key(1)),
        Err(BuildResourceError::NotServed)
    );
    assert_eq!(rebuild(&table, 1, &key(1)), Ok(()));
    assert!(table.entries.borrow()[0].asked, "kept for a rebuild");

    table.sealed = true;
    assert_eq!(rebuild(&table, 1, &key(1)), Ok(()), "never built yet");
    table.entries.borrow_mut()[0].held = Some((None, vec![0], vec![1]));
    assert_eq!(rebuild(&table, 1, &key(1)), Err(BuildResourceError::NoBase));
    table.entries.borrow_mut()[0].held = Some((Some(vec![0]), vec![9, 9], vec![1]));
    assert_eq!(
        rebuild(&table, 1, &key(1)),
        Err(BuildResourceError::SizeChanged)
    );
    assert_eq!(
        table.logged.take(),
        [Unbuilt::SizeChanged { built: 1, held: 2 }]
    );
    table.entries.borrow_mut()[0].held = Some((Some(vec![0]), vec![9], vec![2]));
    assert_eq!(
        rebuild(&table, 1, &key(1)),
        Err(BuildResourceError::NotBuilt),
        "no builder of the held bytes runs"
    );
}

#[test]
fn a_rebuild_that_changed_bytes_rebuilds_its_readers_once_each() {
    let table = table(vec![
        entry(1, &[(1, Does::Add)]),
        entry(2, &[(1, Does::Read(0))]),
        entry(3, &[(1, Does::Read(1))]),
    ]);
    {
        let mut entries = table.entries.borrow_mut();
        entries[0].held = Some((Some(vec![0]), vec![9], vec![1]));
        entries[1].held = Some((Some(vec![0]), vec![9], vec![1]));
        entries[2].held = Some((Some(vec![0]), vec![9], vec![1]));
        entries[0].read_by = vec![1, 2];
        entries[1].read_by = vec![2, 0];
    }
    let mut done = Vec::new();
    assert_eq!(rebuild_in(&table, 0, &mut done), Ok(()));
    assert_eq!(done, [key(1), key(3), key(2)], "each key once, cycles end");
    let held = |at: At| {
        table.entries.borrow()[at]
            .held
            .as_ref()
            .map(|h| h.1.clone())
    };
    assert_eq!(held(0), Some(vec![1]));
}

#[test]
fn a_rebuild_of_a_key_the_game_never_built_still_rebuilds_its_readers() {
    let table = table(vec![
        entry(1, &[(1, Does::Add)]),
        entry(2, &[(1, Does::Read(0))]),
    ]);
    {
        let mut entries = table.entries.borrow_mut();
        entries[1].held = Some((Some(vec![0]), vec![9], vec![1]));
        entries[0].read_by = vec![1];
    }
    let mut done = Vec::new();
    assert_eq!(rebuild_in(&table, 0, &mut done), Ok(()));
    assert_eq!(done, [key(1), key(2)], "served reads saw it, so its reader");
}

#[test]
fn a_reader_whose_build_runs_on_this_thread_is_left() {
    let table = table(vec![
        entry(1, &[(1, Does::Add)]),
        entry(2, &[(1, Does::Read(0))]),
    ]);
    {
        let mut entries = table.entries.borrow_mut();
        entries[0].held = Some((Some(vec![0]), vec![9], vec![1]));
        entries[1].held = Some((Some(vec![0]), vec![9], vec![1]));
        entries[0].read_by = vec![1];
    }
    table
        .frames
        .borrow_mut()
        .push((1, false, false, Vec::new()));
    let mut done = Vec::new();
    assert_eq!(rebuild_in(&table, 0, &mut done), Ok(()));
    assert_eq!(
        done,
        [key(1)],
        "its build reads the key as it is when it does"
    );
}

#[test]
fn a_served_read_builds_falls_back_to_the_archive_and_refuses_before_the_seal() {
    let mut new = entry(2, &[(1, Does::Answer(Status::REFUSED))]);
    new.base = key(1);
    let mut table = table(vec![
        entry(1, &[(1, Does::Add)]),
        new,
        entry(3, &[(1, Does::Answer(Status::REFUSED))]),
    ]);
    table.archive = vec![(key(1), vec![4]), (key(3), vec![6])];
    assert_eq!(read_served(&table, &key(1)), Ok(vec![5]));
    assert_eq!(
        read_served(&table, &key(3)),
        Ok(vec![6]),
        "patched in place"
    );
    assert_eq!(
        read_served(&table, &key(2)),
        Err(BuildResourceError::NotInArchive),
        "a new key no patch built"
    );
    assert_eq!(
        read_served(&table, &key(9)),
        Err(BuildResourceError::NotInArchive)
    );
    table.enter(0, Build::Game);
    assert_eq!(
        read_served(&table, &key(1)),
        Err(BuildResourceError::Building)
    );
    drop(table.leave((), None));
    table.sealed = false;
    assert_eq!(
        read_served(&table, &key(1)),
        Err(BuildResourceError::Unsealed)
    );
}

#[test]
fn rekeys_in_a_ring_rotate_and_keep_every_other_patch_of_the_key_they_read() {
    let mut table = table(vec![
        entry(1, &[(1, Does::Rekey(1))]),
        entry(2, &[(2, Does::Rekey(2)), (4, Does::Add)]),
        entry(3, &[(3, Does::Rekey(0))]),
    ]);
    table.archive = vec![(key(1), vec![10]), (key(2), vec![20]), (key(3), vec![30])];
    assert_eq!(
        read_served(&table, &key(1)),
        Ok(vec![21]),
        "the second without its re-key"
    );
    assert_eq!(
        read_served(&table, &key(2)),
        Ok(vec![31]),
        "the third, then its own change"
    );
    assert_eq!(read_served(&table, &key(3)), Ok(vec![10]));
    assert!(table.failed.take().is_empty());
}

#[test]
fn a_build_without_rekeys_leaves_them_out_and_fails_no_patch() {
    let table = table(vec![entry(
        1,
        &[(1, Does::Answer(Status::REFUSED)), (2, Does::Rekey(0))],
    )]);
    let built = build(&table, 0, view(&[7]), Build::Unkeyed, None);
    assert_eq!(
        built.err(),
        Some(BuildResourceError::NotBuilt),
        "the re-key left out"
    );
    assert!(
        table.failed.take().is_empty(),
        "skipped as in a served read"
    );
}
