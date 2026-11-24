//! The kinds of `HostApi` parameter the fake's entry layer checks before
//! an entry's body runs, as the loader's entry layer reads them: each names
//! its twin in the loader's `host/layer/args.rs` and refuses what that one
//! refuses, in the same order. The body gets the raw value.
//!
//! A check reads a pointer only as far as its twin does, through the same
//! copy helpers, so a null or short struct is refused here as there.

use enshwick_core::__fake::copy;
use enshwick_core::abi::{Headed as HeadedStruct, Status, Str};

/// The refusal of a malformed parameter.
const BAD_ARGS: Option<Status> = Some(Status::BAD_ARGS);

/// A kind read from one of the ABI's parameters.
pub(crate) trait Check<Raw> {
    /// The loader's refusal of `raw`, `None` when it takes it.
    fn refusal(raw: &Raw) -> Option<Status>;
}

/// A kind read from two of the ABI's parameters.
pub(crate) trait Check2<A, B> {
    fn refusal(first: &A, second: &B) -> Option<Status>;
}

/// A kind read from three of the ABI's parameters.
pub(crate) trait Check3<A, B, C> {
    fn refusal(first: &A, second: &B, third: &C) -> Option<Status>;
}

/// A kind read from four of the ABI's parameters.
pub(crate) trait Check4<A, B, C, D> {
    fn refusal(first: &A, second: &B, third: &C, fourth: &D) -> Option<Status>;
}

/// A borrowed string that is UTF-8.
pub(crate) struct Text;

impl Check<Str> for Text {
    fn refusal(raw: &Str) -> Option<Status> {
        copy::text(*raw).is_none().then_some(Status::BAD_ARGS)
    }
}

/// A yes or no, 1 or 0.
pub(crate) struct YesNo;

impl Check<u8> for YesNo {
    fn refusal(raw: &u8) -> Option<Status> {
        (*raw > 1).then_some(Status::BAD_ARGS)
    }
}

/// A mod's callback, never null.
pub(crate) struct Callback;

impl<F> Check<Option<F>> for Callback {
    fn refusal(raw: &Option<F>) -> Option<Status> {
        raw.is_none().then_some(Status::BAD_ARGS)
    }
}

/// A struct that starts with a header, at least its first version long.
pub(crate) struct Headed;

impl<T: HeadedStruct> Check<*const T> for Headed {
    fn refusal(raw: &*const T) -> Option<Status> {
        HeadedSince::<0>::reads(*raw, T::BORN)
    }
}

/// [`Headed`] whose required fields end at `REQUIRED`.
pub(crate) struct HeadedSince<const REQUIRED: usize>;

impl<const REQUIRED: usize> HeadedSince<REQUIRED> {
    /// `None` when `raw` holds a header and `required` bytes, as the
    /// loader's copy reads it.
    fn reads<T: HeadedStruct>(raw: *const T, required: usize) -> Option<Status> {
        // SAFETY: the ABI promises `raw` null or readable for the size its
        // header states, with no other writer during the call; the copy
        // reads no further.
        let read = unsafe { copy::headed_in(raw, required) };
        if read.is_none() { BAD_ARGS } else { None }
    }
}

impl<T: HeadedStruct, const REQUIRED: usize> Check<*const T> for HeadedSince<REQUIRED> {
    fn refusal(raw: &*const T) -> Option<Status> {
        Self::reads(*raw, REQUIRED)
    }
}

/// [`Headed`] that may be null.
pub(crate) struct MaybeHeaded;

impl<T: HeadedStruct> Check<*const T> for MaybeHeaded {
    fn refusal(raw: &*const T) -> Option<Status> {
        if raw.is_null() {
            None
        } else {
            <Headed as Check<*const T>>::refusal(raw)
        }
    }
}

/// A plain value passed by pointer, never null.
pub(crate) struct In;

impl<T> Check<*const T> for In {
    fn refusal(raw: &*const T) -> Option<Status> {
        raw.is_null().then_some(Status::BAD_ARGS)
    }
}

/// Where the mod wants one plain value, never null.
pub(crate) struct Out;

impl<T> Check<*mut T> for Out {
    fn refusal(raw: &*mut T) -> Option<Status> {
        raw.is_null().then_some(Status::BAD_ARGS)
    }
}

/// [`Out`] that may be null.
pub(crate) struct MaybeOut;

impl<T> Check<*mut T> for MaybeOut {
    fn refusal(_raw: &*mut T) -> Option<Status> {
        None
    }
}

/// Where the mod wants a struct that starts with a header, never null.
pub(crate) struct HeadedOut;

impl<T> Check<*mut T> for HeadedOut {
    fn refusal(raw: &*mut T) -> Option<Status> {
        if raw.is_null() {
            return BAD_ARGS;
        }
        // SAFETY: `raw` is non-null, and the ABI promises it starts with the
        // header the mod filled in.
        let room = unsafe { copy::room(raw.cast_const()) };
        if room.is_none() { BAD_ARGS } else { None }
    }
}

/// [`Headed`] and [`HeadedOut`] for one pointer.
pub(crate) struct HeadedInOut;

impl<T: HeadedStruct> Check<*mut T> for HeadedInOut {
    fn refusal(raw: &*mut T) -> Option<Status> {
        <Headed as Check<*const T>>::refusal(&raw.cast_const())
            .or_else(|| <HeadedOut as Check<*mut T>>::refusal(raw))
    }
}

/// A list of aligned plain values and their count.
pub(crate) struct List;

impl<T> Check2<*const T, u32> for List {
    fn refusal(raw: &*const T, count: &u32) -> Option<Status> {
        // SAFETY: the ABI promises the pointer null or at `count` aligned
        // values readable for the call.
        let read = unsafe { copy::list_in(*raw, *count as usize) };
        if read.is_none() { BAD_ARGS } else { None }
    }
}

/// [`List`] that need not be aligned.
pub(crate) struct UnalignedList;

impl<T> Check2<*const T, u32> for UnalignedList {
    fn refusal(raw: &*const T, count: &u32) -> Option<Status> {
        (raw.is_null() && *count != 0).then_some(Status::BAD_ARGS)
    }
}

/// A buffer for the loader to fill and its count, never null.
pub(crate) struct Buffer;

impl<T, C: Copy + TryInto<usize>> Check2<*mut T, C> for Buffer {
    fn refusal(raw: &*mut T, count: &C) -> Option<Status> {
        let fits = (*count).try_into().is_ok_and(|count: usize| {
            count
                .checked_mul(size_of::<T>())
                .is_some_and(|bytes| isize::try_from(bytes).is_ok())
        });
        (raw.is_null() || !fits).then_some(Status::BAD_ARGS)
    }
}

/// [`Buffer`] that may be null.
pub(crate) struct MaybeBuffer;

impl<T, C: Copy + TryInto<usize>> Check2<*mut T, C> for MaybeBuffer {
    fn refusal(raw: &*mut T, count: &C) -> Option<Status> {
        if raw.is_null() {
            None
        } else {
            <Buffer as Check2<*mut T, C>>::refusal(raw, count)
        }
    }
}

/// A list of structs that start with headers and their count.
pub(crate) struct HeadedList;

impl<T: HeadedStruct> Check2<*const T, u32> for HeadedList {
    fn refusal(raw: &*const T, count: &u32) -> Option<Status> {
        // SAFETY: the ABI promises `count` consecutive entries, each starting
        // with its header and readable for the size the first one states.
        let read = unsafe { copy::headed_list_in(*raw, *count, T::BORN) };
        if read.is_none() { BAD_ARGS } else { None }
    }
}

/// Where the mod wants a list of plain values; checked as the answer is
/// written.
pub(crate) struct ListOut;

impl<T> Check3<*mut T, u32, *mut u32> for ListOut {
    fn refusal(_out: &*mut T, _capacity: &u32, _count: &*mut u32) -> Option<Status> {
        None
    }
}

/// Where the mod wants a list of headed entries; checked as the answer is
/// written.
pub(crate) struct SizedListOut;

impl<T> Check4<*mut T, u32, u32, *mut u32> for SizedListOut {
    fn refusal(_out: &*mut T, _stride: &u32, _capacity: &u32, _count: &*mut u32) -> Option<Status> {
        None
    }
}

/// Where the mod wants bytes; checked as the answer is written.
pub(crate) struct BytesOut;

impl Check3<*mut u8, u32, *mut u32> for BytesOut {
    fn refusal(_out: &*mut u8, _capacity: &u32, _len: &*mut u32) -> Option<Status> {
        None
    }
}
