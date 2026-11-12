//! The base stock lists the game hands out, with every inventory a filter
//! refuses left out.

use std::cell::RefCell;

use enshwick_core::abi::Interaction;
use enshwick_core::raw::{read_at, write_at};

use super::{INTERACTIONS_FUSE, LOADED, allowed, interaction};
use crate::engine::chains::Next;

// SAFETY: `StockFn` is the signature of the build table's
// `access.base_stock`, and the parent module's `link` uses this slot only as
// that function's next.
pub(super) static STOCK_NEXT: Next<StockFn> = unsafe { Next::new() };

pub(super) type StockFn =
    unsafe extern "system" fn(*mut u8, *mut [u64; 4], *mut u8, *mut u8, *const f32, u32) -> *mut u8;

/// Filtered copies of the base stock lists one thread handed out in one
/// world step, by their ids.
#[derive(Default)]
struct Stock {
    step: u64,
    lists: rustc_hash::FxHashSet<Box<[u32]>>,
}

thread_local! {
    /// This thread's filtered stock lists of the current step: the same
    /// list for the same ids within the step. The game's callers walk a list
    /// before they return, so a new step retires the old step's lists.
    static STOCK: RefCell<Stock> = RefCell::new(Stock::default());
}

/// A copy of `list` that stays unchanged at its address for the rest of
/// this step and [`crate::engine::retire::GRACE`] steps after: its address and
/// length.
fn kept_list(list: &[u32]) -> (*const u32, usize) {
    kept_in_step(list, crate::engine::retire::current())
}

/// [`kept_list`] in world step `step`.
fn kept_in_step(list: &[u32], step: u64) -> (*const u32, usize) {
    STOCK.with_borrow_mut(|kept| {
        if kept.step != step {
            let old = std::mem::take(&mut kept.lists);
            if !old.is_empty() {
                crate::engine::retire::retire(old);
            }
            kept.step = step;
        }
        if let Some(found) = kept.lists.get(list) {
            return (found.as_ptr(), found.len());
        }
        let copy: Box<[u32]> = list.into();
        let at = (copy.as_ptr(), copy.len());
        kept.lists.insert(copy);
        at
    })
}

pub(super) unsafe extern "system" fn stock(
    out: *mut u8,
    lists: *mut [u64; 4],
    server: *mut u8,
    client: *mut u8,
    position: *const f32,
    player: u32,
) -> *mut u8 {
    if crate::world::transactions::holds().quiet() {
        // SAFETY: this detour runs only as the `access.base_stock` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { STOCK_NEXT.get()(out, lists, server, client, position, player) };
    }
    // Asked by the client's own interface, which sees the client's world;
    // read before the chain calls on.
    let ui = LOADED.get().is_some_and(|(loader, layout, _)| {
        if layout.stock_ui_return == 0 {
            return false;
        }
        let frame = (loader.caller_frame)();
        // SAFETY: the link asks for `CALLER_FRAME` when `stock_ui_return` is
        // set, so the frame was taken at entry of this chain and `rsp`,
        // non-zero by the check before it, points at the live return
        // address.
        frame.rsp != 0 && unsafe { frame.return_address() } == layout.stock_ui_return
    });
    // SAFETY: this detour runs only as the `access.base_stock` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let result = unsafe { STOCK_NEXT.get()(out, lists, server, client, position, player) };
    // SAFETY: `out` is non-null by the check before it and is the base
    // stock's result, whose first byte is the game's error code, 0 for
    // success.
    if ui || out.is_null() || lists.is_null() || unsafe { read_at::<u8>(out, 0) } != 0 {
        return result;
    }
    // SAFETY: `lists` is non-null by the check before it and is the base stock's
    // lists argument, which the game has just filled, on its thread.
    let _done = INTERACTIONS_FUSE.run("the stock", || unsafe { filter_stock(lists, player) });
    result
}

/// Leaves out of the stock lists every inventory a filter refuses.
///
/// # Safety
///
/// - `lists` is the non-null, writable `[u64; 4]` the game's base stock has
///   just filled: the stock ids and count, then the fuel ids and count.
/// - Each ids pointer is null or readable for its count of `u32` ids, alive
///   for the call.
/// - Called on the base stock's thread, which alone writes `lists`
///   meanwhile.
unsafe fn filter_stock(lists: *mut [u64; 4], player: u32) {
    // SAFETY: the caller guarantees `lists` is the readable `[u64; 4]` the
    // game has just filled.
    let mut found: [u64; 4] = unsafe { read_at(lists.cast_const().cast(), 0) };
    let ids = |ptr: u64, count: u64| {
        let ptr = core::ptr::with_exposed_provenance::<u8>(usize::try_from(ptr).unwrap_or(0));
        let count = if ptr.is_null() {
            0
        } else {
            usize::try_from(count).unwrap_or(0)
        };
        (0..count).map(move |i| {
            // SAFETY: `ptr` is non-null whenever `count` is, and the caller
            // guarantees it holds `count` readable u32 ids; `i` is below
            // `count`.
            unsafe { read_at::<u32>(ptr, i * 4) }
        })
    };
    let (stock, fuel) = (ids(found[0], found[1]), ids(found[2], found[3]));
    let refused = |id: u32| {
        // SAFETY: a null context, which `interaction` takes: the call carries
        // no item or position.
        !allowed(&unsafe { interaction(Interaction::STOCK, player, id, core::ptr::null_mut()) })
    };
    let Some(first) = stock.clone().chain(fuel.clone()).position(refused) else {
        return;
    };
    // Never filter in place: every player reads the same arrays.
    let mut index = 0;
    let mut copy = Vec::new();
    for (source, at) in [(stock, 0), (fuel, 2)] {
        copy.clear();
        for id in source {
            let keep = match index.cmp(&first) {
                core::cmp::Ordering::Less => true,
                core::cmp::Ordering::Equal => false,
                core::cmp::Ordering::Greater => !refused(id),
            };
            index += 1;
            if keep {
                copy.push(id);
            }
        }
        let (kept, len) = kept_list(&copy);
        found[at] = kept.expose_provenance() as u64;
        found[at + 1] = len as u64;
    }
    // SAFETY: the caller guarantees `lists` is writable and only this thread
    // writes it; the copies it now names stay unchanged until the world has
    // stepped past this caller, which reads them before it returns.
    unsafe { write_at(lists.cast(), 0, found) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ids at a kept list's address.
    fn ids((at, len): (*const u32, usize)) -> Vec<u32> {
        // SAFETY: `at` and `len` come from `kept_in_step` in the same step on
        // this thread, so the copy is in this thread's cache, unchanged.
        unsafe { core::slice::from_raw_parts(at, len) }.to_vec()
    }

    #[test]
    fn a_filtered_stock_list_keeps_its_address_within_a_step() {
        let first = kept_in_step(&[3, 5, 8], 1_000);
        let other = kept_in_step(&[3, 5], 1_000);
        let again = kept_in_step(&[3, 5, 8], 1_000);
        assert_eq!(first, again);
        assert_eq!((ids(first), ids(other)), (vec![3, 5, 8], vec![3, 5]));
    }

    #[test]
    fn a_new_step_retires_the_old_lists_and_copies_again() {
        let first = kept_in_step(&[4, 6], 2_000);
        assert_eq!(ids(first), [4, 6]);
        let next = kept_in_step(&[4, 6], 2_001);
        assert_eq!(ids(next), [4, 6]);
        STOCK.with_borrow(|kept| assert_eq!((kept.step, kept.lists.len()), (2_001, 1)));
    }
}
