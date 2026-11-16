//! A running system's query context for tests: rows of entities and
//! iterator words, walked by test functions in place of the game's.
//!
//! The double moves the context's own row counter as the game's iterator
//! does: `next` takes the row after the counter, so a walk that does not
//! put the counter back to 0 first finds no row, and asking a row's entity
//! before `next` took it answers 0.

use std::cell::RefCell;

use super::Walk;

/// Where the double keeps the row counter in its context.
const CONTEXT_ROW: usize = 8;
/// Where its descriptor keeps the pointer to the row entities.
const ENTITY_RECORDS: usize = 0;
/// The bytes of one row's entity record.
const RECORD_SIZE: usize = size_of::<u32>();

std::thread_local! {
    /// The iterator words of each row of the double this thread walks.
    static WORDS: RefCell<Vec<Vec<u64>>> = const { RefCell::new(Vec::new()) };
}

/// A query's descriptor, as far as the walk reads it.
#[repr(C)]
struct Descriptor {
    records: *const u32,
}

/// A query context, as far as the walk reads it.
#[repr(C)]
struct Context {
    descriptor: *const Descriptor,
    row: u32,
}

/// A query context over rows a test lays out; one per thread at a time.
pub(crate) struct Query {
    context: Box<Context>,
    _descriptor: Box<Descriptor>,
    _records: Vec<u32>,
}

impl Query {
    /// A context whose query holds `rows`, each an entity and its iterator
    /// words, with its row counter at `row`.
    pub(crate) fn new(rows: Vec<(u32, Vec<u64>)>, row: u32) -> Self {
        let (records, words): (Vec<u32>, Vec<Vec<u64>>) = rows.into_iter().unzip();
        WORDS.set(words);
        let descriptor = Box::new(Descriptor {
            records: records.as_ptr(),
        });
        let context = Box::new(Context {
            descriptor: &raw const *descriptor,
            row,
        });
        Self {
            context,
            _descriptor: descriptor,
            _records: records,
        }
    }

    /// The context, as a hooked system gets it.
    pub(crate) fn ctx(&mut self) -> *mut u64 {
        (&raw mut *self.context).cast()
    }

    /// The row counter now.
    pub(crate) fn row(&self) -> u32 {
        self.context.row
    }

    /// The walk over this double's test functions and layout.
    pub(crate) const fn walk() -> Walk {
        Walk::for_test(init, next, CONTEXT_ROW, ENTITY_RECORDS, RECORD_SIZE)
    }
}

/// Clears the iterator, as the game's init does.
unsafe extern "system" fn init(_: *mut u64, iterator: *mut u64, size: u64) {
    let words = usize::try_from(size).unwrap_or(0) / size_of::<u64>();
    // SAFETY: the walk hands an iterator of `size` writable bytes.
    unsafe { core::ptr::write_bytes(iterator, 0, words) };
}

/// Moves the context's row counter to the next row and copies its words,
/// 0 past the last row.
unsafe extern "system" fn next(ctx: *mut u64, iterator: *mut u64, size: u64) -> u64 {
    // SAFETY: the walk hands the double's own context.
    let context = unsafe { &mut *ctx.cast::<Context>() };
    let at = context.row as usize;
    WORDS.with_borrow(|rows| {
        let Some(row) = rows.get(at) else {
            return 0;
        };
        let words = (usize::try_from(size).unwrap_or(0) / size_of::<u64>()).min(row.len());
        // SAFETY: the walk hands an iterator of `size` writable bytes, and
        // `words` is at most that many words of the row.
        unsafe { core::ptr::copy_nonoverlapping(row.as_ptr(), iterator, words) };
        context.row += 1;
        1
    })
}
