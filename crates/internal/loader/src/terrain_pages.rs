//! What mods hear of the terrain coming into the server's memory and
//! leaving it.
//!
//! The server keeps the ground's voxels by page in one cache
//! its voxel editors share, and a worker thread runs the cache's update: it
//! finishes the loads that reads asked for, then drops the pages used least
//! lately while memory runs short (server 26430253, server copy). The loader
//! hooks both inside that update and only queues the cache entry's handle
//! there. After the step, on the game thread, it finds the pages of the
//! finest column grid of the editor the voxel API reads whose slots hold
//! each handle, and raises `Event::TERRAIN_PAGE_LOADED` or
//! `Event::TERRAIN_PAGE_UNLOADED` for each. Turns of the other editor and of
//! coarser levels match no slot there and raise nothing.

#[cfg(windows)]
use core::ffi::c_void;
#[cfg(windows)]
use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::Mutex;

#[cfg(windows)]
use enshwick::abi::{Event, Header, HookFlags, Registers, Side, TerrainPageData};

#[cfg(windows)]
use crate::fuse::Fuse;
#[cfg(windows)]
use crate::host::Loader;
#[cfg(windows)]
use crate::locks::Relock;
#[cfg(windows)]
use crate::pinned;
#[cfg(windows)]
use crate::wire::Drops;

/// The turns one step keeps at most, 64 KiB of them; more are dropped and
/// counted. The cache turns about 80 pages a second at most across every
/// level and both editors (server 26430253, server copy).
pub const MAX_WAITING: usize = 1 << 13;

/// The events one step raises at most; more are dropped and counted. A
/// handle a few slots share raises once per slot.
pub const MAX_RAISED: usize = 1 << 13;

/// A page's turn in the cache: loaded or dropped, with the handle of its
/// cache entry, the value the column grids' slots hold.
///
/// # Examples
///
/// ```
/// use enshwick_loader::terrain_pages::Turn;
///
/// assert_ne!(Turn { loaded: true, handle: 9 }, Turn { loaded: false, handle: 9 });
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Turn {
    /// Whether the page loaded, else it was dropped.
    pub loaded: bool,
    /// The cache entry's handle.
    pub handle: u32,
}

impl Turn {
    const NONE: Self = Self {
        loaded: false,
        handle: 0,
    };
}

/// The turns waiting for the end of the step, in the order the cache made
/// them, at most [`MAX_WAITING`].
///
/// # Examples
///
/// ```
/// use enshwick_loader::terrain_pages::{Turn, Waiting};
///
/// let mut waiting = Box::new(Waiting::new());
/// assert!(waiting.push(Turn { loaded: true, handle: 9 }));
/// assert_eq!(waiting.len(), 1);
/// ```
#[derive(Debug)]
pub struct Waiting {
    turns: [Turn; MAX_WAITING],
    len: usize,
    /// Turns refused since the last take, counted here so the cache's
    /// thread never logs.
    refused: u64,
}

impl Waiting {
    /// No turns.
    ///
    /// # Examples
    ///
    /// See [`Waiting`].
    #[must_use]
    #[expect(
        clippy::large_stack_arrays,
        clippy::new_without_default,
        reason = "built at compile time for the loader's one static, so a step allocates nothing; elsewhere boxed"
    )]
    pub const fn new() -> Self {
        Self {
            turns: [Turn::NONE; MAX_WAITING],
            len: 0,
            refused: 0,
        }
    }

    /// Keeps `turn` after the others; false, and counted, when
    /// [`MAX_WAITING`] wait.
    ///
    /// # Examples
    ///
    /// See [`Waiting`].
    pub fn push(&mut self, turn: Turn) -> bool {
        let Some(free) = self.turns.get_mut(self.len) else {
            self.refused = self.refused.saturating_add(1);
            return false;
        };
        *free = turn;
        self.len += 1;
        true
    }

    /// The turns waiting.
    ///
    /// # Examples
    ///
    /// See [`Waiting`].
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether no turn waits.
    ///
    /// # Examples
    ///
    /// ```
    /// assert!(enshwick_loader::terrain_pages::Waiting::new().is_empty());
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// What pairing a step's turns with the pages that hold their handles
/// works in, kept between steps so a step allocates nothing.
///
/// # Examples
///
/// ```
/// use enshwick_loader::terrain_pages::{Pairing, Turn, Waiting};
///
/// let mut waiting = Box::new(Waiting::new());
/// waiting.push(Turn { loaded: true, handle: 9 });
/// let mut pairing = Box::new(Pairing::new());
/// pairing.take(&mut waiting);
/// let slots = [0, 9, 4, 9];
/// let (pages, dropped) = pairing.pair(4, |slot| slots[slot as usize]);
/// let pages: Vec<(Turn, u32)> = pages.collect();
/// assert_eq!(pages, [(Turn { loaded: true, handle: 9 }, 1), (Turn { loaded: true, handle: 9 }, 3)]);
/// assert_eq!(dropped, 0);
/// ```
#[derive(Debug)]
pub struct Pairing {
    turns: [Turn; MAX_WAITING],
    taken: usize,
    /// Each turn's handle and place among the turns, sorted.
    keys: [(u32, u32); MAX_WAITING],
    /// Each turn's place and a slot holding its handle.
    pairs: [(u32, u32); MAX_RAISED],
}

impl Pairing {
    /// Nothing taken.
    ///
    /// # Examples
    ///
    /// See [`Pairing`].
    #[must_use]
    #[expect(
        clippy::large_stack_arrays,
        clippy::new_without_default,
        reason = "built at compile time for the loader's one static, so a step allocates nothing; elsewhere boxed"
    )]
    pub const fn new() -> Self {
        Self {
            turns: [Turn::NONE; MAX_WAITING],
            taken: 0,
            keys: [(0, 0); MAX_WAITING],
            pairs: [(0, 0); MAX_RAISED],
        }
    }

    /// Takes the turns of `waiting`, which is empty after it, so the
    /// pairing holds no lock the cache's thread waits on; returns the turns
    /// it refused since the last take.
    ///
    /// # Examples
    ///
    /// See [`Pairing`].
    pub fn take(&mut self, waiting: &mut Waiting) -> u64 {
        let taken = waiting.turns.get(..waiting.len).unwrap_or_default();
        for (into, &turn) in self.turns.iter_mut().zip(taken) {
            *into = turn;
        }
        self.taken = waiting.len;
        waiting.len = 0;
        core::mem::take(&mut waiting.refused)
    }

    /// Pairs each turn taken with every one of the `slots` slots, which
    /// `slot` reads, holding its handle: in the order of the turns, then of
    /// the slots, with the pairs past [`MAX_RAISED`] counted, not kept. A
    /// handle of 0 is an empty slot. Each slot is read once.
    ///
    /// # Examples
    ///
    /// See [`Pairing`].
    pub fn pair(
        &mut self,
        slots: u32,
        mut slot: impl FnMut(u32) -> u32,
    ) -> (impl Iterator<Item = (Turn, u32)> + '_, u64) {
        let Self {
            turns,
            taken,
            keys,
            pairs,
        } = self;
        let turns: &[Turn] = turns.get(..*taken).unwrap_or_default();
        let keys = keys.get_mut(..turns.len()).unwrap_or_default();
        for (key, (place, turn)) in keys.iter_mut().zip((0u32..).zip(turns)) {
            *key = (turn.handle, place);
        }
        keys.sort_unstable();
        let mut kept = 0;
        let mut dropped = 0u64;
        if !keys.is_empty() {
            for index in 0..slots {
                let handle = slot(index);
                if handle == 0 {
                    continue;
                }
                let first = keys.partition_point(|&(key, _)| key < handle);
                for &(_, place) in keys
                    .iter()
                    .skip(first)
                    .take_while(|&&(key, _)| key == handle)
                {
                    if let Some(pair) = pairs.get_mut(kept) {
                        *pair = (place, index);
                        kept += 1;
                    } else {
                        dropped += 1;
                    }
                }
            }
        }
        let pairs = pairs.get_mut(..kept).unwrap_or_default();
        pairs.sort_unstable();
        let pages = pairs.iter().filter_map(|&(place, index)| {
            let turn = turns.get(usize::try_from(place).ok()?)?;
            Some((*turn, index))
        });
        (pages, dropped)
    }
}

/// The column of the page at slot `index` of a grid `width` pages across,
/// whose slots run `z * width + x`.
#[cfg_attr(
    not(any(windows, test)),
    expect(
        dead_code,
        reason = "only the Windows page events and the tests place pages"
    )
)]
const fn page_x(index: u32, width: u32) -> u32 {
    index % width
}

/// The row of the page at slot `index`, as [`page_x`] counts.
#[cfg_attr(
    not(any(windows, test)),
    expect(
        dead_code,
        reason = "only the Windows page events and the tests place pages"
    )
)]
const fn page_z(index: u32, width: u32) -> u32 {
    index / width
}

/// The state of the page events: the queue the cache's thread fills, the
/// pairing the game thread works in, and the turns not raised.
#[cfg(windows)]
struct Pages {
    fuse: Fuse,
    /// Whether both sites linked; until then the sites queue nothing.
    on: AtomicBool,
    waiting: Mutex<Waiting>,
    pending: AtomicBool,
    pairing: Mutex<Pairing>,
    dropped: Drops,
}

#[cfg(windows)]
static PAGES: Pages = Pages {
    fuse: Fuse::new("terrain page events"),
    on: AtomicBool::new(false),
    waiting: Mutex::new(Waiting::new()),
    pending: AtomicBool::new(false),
    pairing: Mutex::new(Pairing::new()),
    dropped: Drops::new(),
};

/// Counts turns or pages not raised and logs the first and every
/// hundredth.
#[cfg(windows)]
#[cold]
fn dropped(count: u64) {
    if let Some(total) = PAGES.dropped.add(count) {
        crate::log!(
            WARN,
            "terrain page events: {total} not raised to mods, past {MAX_WAITING} page turns or {MAX_RAISED} pages in a step, or while the column grid did not read"
        );
    }
}

/// The name the log and the Mods screen give the page events.
#[cfg(windows)]
const FEATURE: &str = "terrain page events";

/// Links both sites in the page cache's update when a mod listens for a
/// terrain page event; without either they stay off, and the log and the
/// Mods screen say why.
#[cfg(windows)]
pub(crate) fn link(loader: &Loader) {
    // Both sites are server only, and a hosted game stays off until its
    // page cache is researched.
    if loader.side != Side::SERVER {
        return;
    }
    if !loader.subscribed(Event::TERRAIN_PAGE_LOADED)
        && !loader.subscribed(Event::TERRAIN_PAGE_UNLOADED)
    {
        return;
    }
    match crate::voxels::finest_slots(loader) {
        Ok(()) => {}
        Err(id) => {
            loader.feature_off(FEATURE, id.id(), "this build has no column grid slots");
            return;
        }
    }
    let sites: [(pinned::Symbol, MidFn); 2] = [
        (pinned::voxel::PAGE_LOADED, page_loaded),
        (pinned::voxel::PAGE_DROPPED, page_dropped),
    ];
    let mut addresses = [0; 2];
    for ((symbol, _), address) in sites.iter().zip(&mut addresses) {
        let Some(found) = loader.address(*symbol) else {
            loader.feature_off(FEATURE, symbol.id(), crate::builds::ABSENT);
            return;
        };
        if !loader.code_holds(*symbol) {
            loader.feature_off(
                FEATURE,
                symbol.id(),
                &crate::builds::Unhooked::Moved.to_string(),
            );
            return;
        }
        *address = found;
    }
    for ((symbol, handler), address) in sites.into_iter().zip(addresses) {
        let link = crate::mid::MidLink {
            owner: crate::mod_index::ModIndex::LOADER,
            handler,
            user: 0,
            priority: i32::MIN,
            flags: HookFlags::NONE,
        };
        let status = loader.mids.relock().add(address, link);
        if !status.is_ok() {
            loader.feature_off(FEATURE, symbol.id(), "the loader refused its hook");
            return;
        }
    }
    PAGES.on.store(true, Ordering::Release);
    crate::log!(INFO, "terrain page events on");
}

#[cfg(windows)]
type MidFn = unsafe extern "system" fn(registers: *mut Registers, user: *mut c_void);

/// The site where the cache marks a finished load: `rsi` holds its entry
/// past the entry's first 8 bytes, the entry's handle first (server
/// 26430253, server copy).
#[cfg(windows)]
unsafe extern "system" fn page_loaded(registers: *mut Registers, _user: *mut c_void) {
    queue(registers, true);
}

/// The site where the cache drops a loaded page, its data freed and its
/// state not yet cleared: `rsi` as at [`page_loaded`] (server 26430253,
/// server copy).
#[cfg(windows)]
unsafe extern "system" fn page_dropped(registers: *mut Registers, _user: *mut c_void) {
    queue(registers, false);
}

/// Queues the turn of the entry in the registers of a page site.
#[cfg(windows)]
fn queue(registers: *mut Registers, loaded: bool) {
    if !PAGES.on.load(Ordering::Acquire) {
        return;
    }
    let _caught = PAGES.fuse.run("the page cache update", || {
        // SAFETY: `registers` is null or the block the mid-hook stub saved on
        // this thread's stack for the handler call, which nothing else
        // touches during it.
        let Some(registers) = (unsafe { registers.as_ref() }) else {
            return;
        };
        let Ok(entry) = usize::try_from(registers.rsi) else {
            return;
        };
        if entry == 0 {
            return;
        }
        // SAFETY: at both sites `rsi` points 8 bytes into the cache entry
        // the update holds locked, whose `u32` handle is there and changes
        // only when the entry is freed, which the update does not do while
        // it holds it; the site's pattern checks the `[rsi+0x40]` state
        // write that names the register (server 26430253, server copy).
        let handle: u32 = unsafe { crate::memory::read(entry, 0) };
        let _kept = PAGES.waiting.relock().push(Turn { loaded, handle });
        PAGES.pending.store(true, Ordering::Release);
    });
}

/// Raises the page turns of the step for the pages of the editor the voxel
/// API reads; on the game thread, outside the game's systems. A step whose
/// queue is empty does nothing.
#[cfg(windows)]
pub(crate) fn after_step(loader: &Loader) {
    if !PAGES.pending.swap(false, Ordering::AcqRel) {
        return;
    }
    let mut pairing = PAGES.pairing.relock();
    let refused = pairing.take(&mut PAGES.waiting.relock());
    if refused > 0 {
        dropped(refused);
    }
    let Ok(grid) = crate::voxels::slots(loader) else {
        dropped(u64::try_from(pairing.taken).unwrap_or(u64::MAX));
        return;
    };
    let (pages, missed) = pairing.pair(grid.count, |index| {
        // SAFETY: `grid` comes from `crate::voxels::slots` on this game
        // thread, after the step: `grid.at` is the finest grid's slot array
        // of `grid.count` aligned `u32`s, which lives as long as the world.
        // The game fills the slots at world load and rewrites one only when
        // its key changes (on a thread not traced, never seen in three
        // 5-minute runs), with an aligned store, so a read sees the old or
        // the new handle; a handle carries its entry's generation, so
        // neither pairs with another page's turn (server 26430253, server
        // copy).
        unsafe { crate::memory::read(grid.at, index as usize * size_of::<u32>()) }
    });
    for (turn, index) in pages {
        let data = TerrainPageData {
            header: Header::of::<TerrainPageData>(),
            x: page_x(index, grid.width),
            z: page_z(index, grid.width),
            size: grid.metres,
        };
        let event = if turn.loaded {
            Event::TERRAIN_PAGE_LOADED
        } else {
            Event::TERRAIN_PAGE_UNLOADED
        };
        loader.emit(event, None, (&raw const data).cast());
    }
    if missed > 0 {
        dropped(missed);
    }
}

/// Drops the turns waiting for a hosted game that stops.
#[cfg(windows)]
pub(crate) fn forget() {
    let mut waiting = PAGES.waiting.relock();
    waiting.len = 0;
    waiting.refused = 0;
    drop(waiting);
    PAGES.pending.store(false, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(loaded: bool, handle: u32) -> Turn {
        Turn { loaded, handle }
    }

    fn paired(turns: &[Turn], slots: &[u32]) -> (Vec<(Turn, u32)>, u64) {
        let mut waiting = Box::new(Waiting::new());
        for &t in turns {
            assert!(waiting.push(t));
        }
        let mut pairing = Box::new(Pairing::new());
        pairing.take(&mut waiting);
        assert!(waiting.is_empty(), "taking empties the queue");
        let count = u32::try_from(slots.len()).unwrap();
        let (pages, dropped) = pairing.pair(count, |i| slots[i as usize]);
        (pages.collect(), dropped)
    }

    #[test]
    fn each_turn_pairs_with_every_slot_holding_its_handle_in_turn_then_slot_order() {
        let slots = [0, 7, 3, 7, 0, 5];
        let (pages, dropped) = paired(&[turn(true, 7), turn(false, 5)], &slots);
        assert_eq!(
            pages,
            [(turn(true, 7), 1), (turn(true, 7), 3), (turn(false, 5), 5)]
        );
        assert_eq!(dropped, 0);
    }

    #[test]
    fn a_page_dropped_and_loaded_in_one_step_raises_both_in_that_order() {
        let (pages, _) = paired(&[turn(false, 3), turn(true, 3)], &[3]);
        assert_eq!(pages, [(turn(false, 3), 0), (turn(true, 3), 0)]);
    }

    #[test]
    fn handles_no_slot_holds_and_empty_slots_raise_nothing() {
        let (pages, dropped) = paired(&[turn(true, 0), turn(true, 11)], &[0, 0, 12]);
        assert!(pages.is_empty());
        assert_eq!(dropped, 0);
    }

    #[test]
    fn without_turns_no_slot_is_read() {
        let mut pairing = Box::new(Pairing::new());
        pairing.take(&mut Box::new(Waiting::new()));
        let (pages, _) = pairing.pair(6400, |_| panic!("a slot was read"));
        assert_eq!(pages.count(), 0);
    }

    #[test]
    fn pages_past_the_step_s_limit_are_counted_not_raised() {
        let slots = vec![7; MAX_RAISED + 10];
        let (pages, dropped) = paired(&[turn(true, 7)], &slots);
        assert_eq!(pages.len(), MAX_RAISED);
        assert_eq!(dropped, 10);
    }

    #[test]
    fn the_queue_refuses_turns_past_its_limit() {
        let mut waiting = Box::new(Waiting::new());
        for handle in 0..MAX_WAITING {
            assert!(waiting.push(turn(true, u32::try_from(handle).unwrap())));
        }
        assert!(!waiting.push(turn(true, 1)));
        assert_eq!(waiting.len(), MAX_WAITING);
        assert!(!waiting.is_empty());
    }

    #[test]
    fn turns_refused_by_a_full_queue_are_counted_for_the_next_take_only() {
        let mut waiting = Box::new(Waiting::new());
        for _ in 0..MAX_WAITING + 3 {
            let _kept = waiting.push(turn(true, 1));
        }
        let mut pairing = Box::new(Pairing::new());
        assert_eq!(pairing.take(&mut waiting), 3);
        assert_eq!(pairing.take(&mut waiting), 0);
    }

    #[test]
    fn a_slot_names_its_page_by_column_then_row_across_the_grid() {
        assert_eq!((page_x(0, 80), page_z(0, 80)), (0, 0));
        assert_eq!((page_x(83, 80), page_z(83, 80)), (3, 1));
        assert_eq!((page_x(6399, 80), page_z(6399, 80)), (79, 79));
    }

    #[test]
    fn a_reused_entry_index_with_another_generation_does_not_pair() {
        let (old, new) = (0x0100_0007, 0x0200_0007);
        let (pages, _) = paired(&[turn(false, old)], &[new]);
        assert!(pages.is_empty());
    }
}
