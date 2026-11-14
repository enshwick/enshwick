//! The kinds of `HostApi` parameter the entry layer reads: who calls, what a
//! mod passes in and where it wants the answer.
//!
//! Each kind's `from_abi` is unsafe only for the ABI's promise about such a
//! parameter, stated in its `# Safety`. A value that points into the mod's
//! memory borrows the call, so it never outlives it.

use core::marker::PhantomData;
use std::sync::atomic::Ordering;

use enshwick_core::abi::{ModHandle, Status, Str};

use super::{Call, Refused, checked};
use crate::engine::loader::{Loader, ModRecord, loader};
use crate::engine::mod_index::ModIndex;
use crate::host::memory;

/// A parameter kind the entry layer reads from one of the ABI's parameters.
pub(crate) trait Arg {
    /// The parameter as the ABI passes it.
    type Raw;
    /// What the entry's body gets.
    type Value<'call>
    where
        Self: 'call;

    /// Reads `raw` for `call`.
    ///
    /// # Errors
    ///
    /// The refusal a malformed value or an unknown caller gets.
    ///
    /// # Safety
    ///
    /// `raw` holds what the ABI promises for a parameter of this kind, as
    /// the kind's own docs state, for the whole call.
    unsafe fn from_abi(raw: Self::Raw, call: &Call) -> Result<Self::Value<'_>, Refused>;
}

/// A parameter kind read from two of the ABI's parameters.
pub(crate) trait Arg2 {
    type First;
    type Second;
    type Value<'call>
    where
        Self: 'call;

    /// Reads the two parameters for `call`.
    ///
    /// # Errors
    ///
    /// The refusal a malformed value gets.
    ///
    /// # Safety
    ///
    /// As [`Arg::from_abi`], for both parameters.
    unsafe fn from_abi(
        first: Self::First,
        second: Self::Second,
        call: &Call,
    ) -> Result<Self::Value<'_>, Refused>;
}

/// A parameter kind read from three of the ABI's parameters.
pub(crate) trait Arg3 {
    type First;
    type Second;
    type Third;
    type Value<'call>
    where
        Self: 'call;

    /// Reads the three parameters for `call`.
    ///
    /// # Errors
    ///
    /// The refusal a malformed value gets.
    ///
    /// # Safety
    ///
    /// As [`Arg::from_abi`], for all three parameters.
    unsafe fn from_abi(
        first: Self::First,
        second: Self::Second,
        third: Self::Third,
        call: &Call,
    ) -> Result<Self::Value<'_>, Refused>;
}

/// A parameter kind read from four of the ABI's parameters.
pub(crate) trait Arg4 {
    type First;
    type Second;
    type Third;
    type Fourth;
    type Value<'call>
    where
        Self: 'call;

    /// Reads the four parameters for `call`.
    ///
    /// # Errors
    ///
    /// The refusal a malformed value gets.
    ///
    /// # Safety
    ///
    /// As [`Arg::from_abi`], for all four parameters.
    unsafe fn from_abi(
        first: Self::First,
        second: Self::Second,
        third: Self::Third,
        fourth: Self::Fourth,
        call: &Call,
    ) -> Result<Self::Value<'_>, Refused>;
}

const BAD_ARGS: Refused = Refused::with(Status::BAD_ARGS);

/// The calling mod, which must be on: its loader, index and record.
///
/// A handle the loader never gave out is [`Status::BAD_ARGS`], and a mod
/// that is off [`Status::FAULTED`], the one answer every entry gives it.
#[derive(Clone, Copy)]
pub(crate) struct Mod {
    pub(crate) loader: &'static Loader,
    pub(crate) index: ModIndex,
    pub(crate) record: &'static ModRecord,
}

impl Mod {
    /// The loaded mod of `me`, on or off.
    fn loaded(me: ModHandle, call: &Call) -> Result<Self, Refused> {
        let loader = loader().ok_or(BAD_ARGS)?;
        let (index, record) = loader.record(me).ok_or(BAD_ARGS)?;
        call.owned_by(loader, index, record);
        Ok(Self {
            loader,
            index,
            record,
        })
    }
}

impl Mod {
    /// The outcome of the mod's claim on `feature`, which it shares with
    /// other mods as `mode` says, noted for the conflict report.
    ///
    /// # Errors
    ///
    /// The claim's own refusal.
    pub(crate) fn claim<E: Into<Refused>>(
        &self,
        feature: &'static str,
        mode: crate::engine::conflicts::Mode,
        outcome: Result<(), E>,
    ) -> Result<(), Refused> {
        let outcome = outcome.map_err(Into::into);
        let status = outcome
            .as_ref()
            .map_or_else(Refused::status, |()| Status::OK);
        crate::engine::conflicts::note(feature, self.index, mode, status);
        outcome
    }
}

impl Arg for Mod {
    type Raw = ModHandle;
    type Value<'call>
        = Self
    where
        Self: 'call;

    unsafe fn from_abi(raw: ModHandle, call: &Call) -> Result<Self, Refused> {
        let found = Self::loaded(raw, call)?;
        if found.record.off.load(Ordering::Acquire) {
            return Err(Refused::with(Status::FAULTED));
        }
        Ok(found)
    }
}

/// The calling mod, on or off: for the entries a mod that is off still
/// calls, such as its log and the list of mods.
pub(crate) struct AnyMod;

impl Arg for AnyMod {
    type Raw = ModHandle;
    type Value<'call>
        = Mod
    where
        Self: 'call;

    unsafe fn from_abi(raw: ModHandle, call: &Call) -> Result<Mod, Refused> {
        Mod::loaded(raw, call)
    }
}

/// A mod that is on, as [`Mod`], or the loader's own copy of the API: the
/// body gets the loader.
pub(crate) struct Caller;

impl Arg for Caller {
    type Raw = ModHandle;
    type Value<'call>
        = &'static Loader
    where
        Self: 'call;

    unsafe fn from_abi(raw: ModHandle, call: &Call) -> Result<&'static Loader, Refused> {
        // SAFETY: a handle is a plain value; `Asker` reads nothing through it.
        unsafe { Asker::from_abi(raw, call) }.map(|asker| asker.loader)
    }
}

/// [`Caller`], with the handle it called by and, when a mod calls, its
/// record.
#[derive(Clone, Copy)]
pub(crate) struct Asker {
    pub(crate) loader: &'static Loader,
    pub(crate) handle: ModHandle,
    pub(crate) record: Option<&'static ModRecord>,
}

impl Arg for Asker {
    type Raw = ModHandle;
    type Value<'call>
        = Self
    where
        Self: 'call;

    unsafe fn from_abi(raw: ModHandle, call: &Call) -> Result<Self, Refused> {
        if raw == crate::host::LOADER_HANDLE {
            return Ok(Self {
                loader: loader().ok_or(BAD_ARGS)?,
                handle: raw,
                record: None,
            });
        }
        // SAFETY: a handle is a plain value; `Mod` reads nothing through it.
        let found = unsafe { Mod::from_abi(raw, call) }?;
        Ok(Self {
            loader: found.loader,
            handle: raw,
            record: Some(found.record),
        })
    }
}

/// A borrowed string: [`Status::BAD_ARGS`] when it is not UTF-8.
///
/// The ABI promises its bytes readable and unchanged for the call.
pub(crate) struct Text;

impl Arg for Text {
    type Raw = Str;
    type Value<'call>
        = &'call str
    where
        Self: 'call;

    unsafe fn from_abi(raw: Str, _: &Call) -> Result<&str, Refused> {
        memory::text(raw).ok_or(BAD_ARGS)
    }
}

/// A struct that starts with a header, copied as far as the mod's version
/// reaches: [`Status::BAD_ARGS`] for null or one shorter than its first
/// version.
///
/// The ABI promises it null or readable for the size its header states,
/// with no other writer during the call; no alignment is needed.
pub(crate) struct Headed<T>(PhantomData<T>);

impl<T: enshwick_core::abi::Headed> Arg for Headed<T> {
    type Raw = *const T;
    type Value<'call>
        = T
    where
        Self: 'call;

    unsafe fn from_abi(raw: *const T, _: &Call) -> Result<T, Refused> {
        // SAFETY: the ABI's promise for this kind is `headed_in`'s.
        unsafe { memory::headed_in(raw, T::BORN) }.ok_or(BAD_ARGS)
    }
}

/// [`Headed`] for a struct whose fields every reader requires end at
/// `REQUIRED`, before its first version's end: the later ones read zero
/// from a mod built before them.
pub(crate) struct HeadedSince<T, const REQUIRED: usize>(PhantomData<T>);

impl<T: enshwick_core::abi::Headed, const REQUIRED: usize> Arg for HeadedSince<T, REQUIRED> {
    type Raw = *const T;
    type Value<'call>
        = T
    where
        Self: 'call;

    unsafe fn from_abi(raw: *const T, _: &Call) -> Result<T, Refused> {
        // SAFETY: the ABI's promise for this kind is `headed_in`'s.
        unsafe { memory::headed_in(raw, REQUIRED) }.ok_or(BAD_ARGS)
    }
}

/// [`Headed`] that a mod may leave null: `None` then.
pub(crate) struct MaybeHeaded<T>(PhantomData<T>);

impl<T: enshwick_core::abi::Headed> Arg for MaybeHeaded<T> {
    type Raw = *const T;
    type Value<'call>
        = Option<T>
    where
        Self: 'call;

    unsafe fn from_abi(raw: *const T, call: &Call) -> Result<Option<T>, Refused> {
        if raw.is_null() {
            return Ok(None);
        }
        // SAFETY: the ABI's promise for this kind is `Headed`'s for a
        // non-null pointer.
        unsafe { Headed::<T>::from_abi(raw, call) }.map(Some)
    }
}

/// A plain value the mod passes by pointer, copied: [`Status::BAD_ARGS`] for
/// null.
///
/// The ABI promises it null or readable for a `T`, unchanged for the call;
/// no alignment is needed.
pub(crate) struct In<T>(PhantomData<T>);

impl<T: Copy> Arg for In<T> {
    type Raw = *const T;
    type Value<'call>
        = T
    where
        Self: 'call;

    unsafe fn from_abi(raw: *const T, _: &Call) -> Result<T, Refused> {
        if raw.is_null() {
            return Err(BAD_ARGS);
        }
        // SAFETY: `raw` is non-null, checked just before, and the ABI
        // promises it readable for a `T`, unchanged for the call.
        // The mod's value, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        Ok(unsafe { raw.read_unaligned() })
    }
}

/// Where the mod wants one plain value: [`Status::BAD_ARGS`] for null,
/// before the body runs.
///
/// The ABI promises it null or writable for a `T`, aligned or not, with
/// nothing else touching it during the call.
pub(crate) struct Out<T>(PhantomData<T>);

/// The place an [`Out`] names, for the one answer the body writes.
pub(crate) struct Slot<'call, T> {
    out: *mut T,
    call: PhantomData<&'call mut T>,
}

impl<T> Arg for Out<T> {
    type Raw = *mut T;
    type Value<'call>
        = Slot<'call, T>
    where
        Self: 'call;

    unsafe fn from_abi(raw: *mut T, _: &Call) -> Result<Slot<'_, T>, Refused> {
        if raw.is_null() {
            return Err(BAD_ARGS);
        }
        Ok(Slot {
            out: raw,
            call: PhantomData,
        })
    }
}

impl<T> Slot<'_, T> {
    /// Writes the answer.
    ///
    /// # Errors
    ///
    /// None: the slot is non-null and writable; `Result` for the body's `?`.
    pub(crate) fn put(self, value: T) -> Result<(), Refused> {
        // SAFETY: `Out::from_abi` found `out` non-null, and the ABI promises
        // it writable for a `T` nothing else touches during the call, which
        // this slot borrows; `put` needs no alignment.
        checked(unsafe { memory::put(self.out, value) })
    }
}

/// Where the mod wants a struct that starts with a header, written as far as
/// both sides know it: [`Status::BAD_ARGS`] for null or a header shorter than
/// a header, before the body runs.
///
/// The ABI promises it null or the mod's version of `T`, writable for the
/// size its header states, untouched by others during the call; no
/// alignment is needed.
pub(crate) struct HeadedOut<T>(PhantomData<T>);

/// The place a [`HeadedOut`] names.
pub(crate) struct HeadedSlot<'call, T> {
    out: *mut T,
    room: usize,
    call: PhantomData<&'call mut T>,
}

impl<T: enshwick_core::abi::Headed> Arg for HeadedOut<T> {
    type Raw = *mut T;
    type Value<'call>
        = HeadedSlot<'call, T>
    where
        Self: 'call;

    unsafe fn from_abi(raw: *mut T, _: &Call) -> Result<HeadedSlot<'_, T>, Refused> {
        if raw.is_null() {
            return Err(BAD_ARGS);
        }
        // SAFETY: `raw` is non-null, checked just before, and the ABI
        // promises it starts with the header the mod filled in.
        let room = unsafe { memory::room(raw) }.ok_or(BAD_ARGS)?;
        Ok(HeadedSlot {
            out: raw,
            room,
            call: PhantomData,
        })
    }
}

impl<T: enshwick_core::abi::Headed> HeadedSlot<'_, T> {
    /// Whether the mod's struct holds every field of `T`'s first version,
    /// for an answer the mod could not do without.
    pub(crate) fn holds_born(&self) -> bool {
        self.room >= T::BORN
    }

    /// Writes the answer as far as the mod's header reaches.
    ///
    /// # Errors
    ///
    /// None after the checks of [`HeadedOut`]; `Result` for the body's `?`.
    pub(crate) fn put(self, value: T) -> Result<(), Refused> {
        // SAFETY: `HeadedOut::from_abi` found `out` non-null with a header,
        // and the ABI promises it writable for the size that header states,
        // untouched by others during the call this slot borrows.
        checked(unsafe { memory::put_headed(self.out, value) })
    }
}

/// A struct that starts with a header, read as [`Headed`] reads it and
/// written back as [`HeadedOut`] writes it.
///
/// The ABI promises what both kinds promise, for one pointer.
pub(crate) struct HeadedInOut<T>(PhantomData<T>);

impl<T: enshwick_core::abi::Headed> Arg for HeadedInOut<T> {
    type Raw = *mut T;
    type Value<'call>
        = (T, HeadedSlot<'call, T>)
    where
        Self: 'call;

    unsafe fn from_abi(raw: *mut T, call: &Call) -> Result<(T, HeadedSlot<'_, T>), Refused> {
        // SAFETY: the ABI's promise for this kind holds both `Headed`'s and
        // `HeadedOut`'s.
        let value = unsafe { Headed::<T>::from_abi(raw, call) }?;
        // SAFETY: the ABI's promise for this kind holds `HeadedOut`'s: the
        // same pointer writable for the size its header states.
        let slot = unsafe { HeadedOut::<T>::from_abi(raw, call) }?;
        Ok((value, slot))
    }
}

/// A list of plain values, a pointer and a count: [`Status::BAD_ARGS`] for a
/// null pointer with a nonzero count.
///
/// The ABI promises the pointer null or at `count` aligned values readable
/// and unchanged for the call.
pub(crate) struct List<T>(PhantomData<T>);

impl<T> Arg2 for List<T> {
    type First = *const T;
    type Second = u32;
    type Value<'call>
        = &'call [T]
    where
        Self: 'call;

    unsafe fn from_abi(raw: *const T, count: u32, _: &Call) -> Result<&[T], Refused> {
        // SAFETY: the ABI's promise for this kind is `list_in`'s, for the
        // call these values borrow.
        unsafe { memory::list_in(raw, count as usize) }.ok_or(BAD_ARGS)
    }
}

/// [`List`] for a list the ABI does not promise aligned, read one value at a
/// time: [`Status::BAD_ARGS`] for a null pointer with a nonzero count.
///
/// The ABI promises the pointer null or at `count` values readable and
/// unchanged for the call; no alignment is needed.
pub(crate) struct UnalignedList<T>(PhantomData<T>);

/// The values an [`UnalignedList`] names, copied out one at a time.
#[derive(Clone, Copy)]
pub(crate) struct Values<'call, T> {
    first: *const T,
    count: usize,
    call: PhantomData<&'call [T]>,
}

impl<T: Copy> Values<'_, T> {
    /// How many values the mod passed.
    pub(crate) const fn len(&self) -> usize {
        self.count
    }

    /// Each value, in the mod's order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = T> + '_ {
        (0..self.count).map(|i| {
            // SAFETY: `i` is below `count`, and `UnalignedList::from_abi`
            // holds the ABI's promise of `count` values at `first`, so the
            // offset stays inside that list.
            let at = unsafe { self.first.add(i) };
            // SAFETY: `at` is inside the list, readable for the call this view
            // borrows; an unaligned read needs no alignment.
            // The mod's list, not game memory.
            // ast-grep-ignore: game-memory-through-raw
            unsafe { at.read_unaligned() }
        })
    }
}

impl<T: Copy> Arg2 for UnalignedList<T> {
    type First = *const T;
    type Second = u32;
    type Value<'call>
        = Values<'call, T>
    where
        Self: 'call;

    unsafe fn from_abi(raw: *const T, count: u32, _: &Call) -> Result<Values<'_, T>, Refused> {
        if raw.is_null() && count != 0 {
            return Err(BAD_ARGS);
        }
        Ok(Values {
            first: raw,
            count: count as usize,
            call: PhantomData,
        })
    }
}

/// A buffer the mod hands over for the loader to fill, a pointer and a
/// count of `C`: [`Status::BAD_ARGS`] for a null pointer or a count past the
/// address space.
///
/// The ABI promises the pointer null or at `count` initialised, aligned
/// values, writable and untouched by others during the call.
pub(crate) struct Buffer<T, C = u32>(PhantomData<(T, C)>);

impl<T, C: TryInto<usize> + 'static> Arg2 for Buffer<T, C> {
    type First = *mut T;
    type Second = C;
    type Value<'call>
        = &'call mut [T]
    where
        Self: 'call;

    unsafe fn from_abi(raw: *mut T, count: C, _: &Call) -> Result<&mut [T], Refused> {
        let count: usize = count.try_into().map_err(|_| BAD_ARGS)?;
        let fits = count
            .checked_mul(size_of::<T>())
            .is_some_and(|bytes| isize::try_from(bytes).is_ok());
        if raw.is_null() || !fits {
            return Err(BAD_ARGS);
        }
        // SAFETY: `raw` is non-null, checked just before, and the ABI
        // promises `count` initialised, aligned values there, writable and
        // untouched by others for the call this slice borrows.
        Ok(unsafe { core::slice::from_raw_parts_mut(raw, count) })
    }
}

/// [`Buffer`] that a mod may leave null when it wants nothing written:
/// `None` then.
pub(crate) struct MaybeBuffer<T, C = u32>(PhantomData<(T, C)>);

impl<T, C: TryInto<usize> + 'static> Arg2 for MaybeBuffer<T, C> {
    type First = *mut T;
    type Second = C;
    type Value<'call>
        = Option<&'call mut [T]>
    where
        Self: 'call;

    unsafe fn from_abi(raw: *mut T, count: C, call: &Call) -> Result<Option<&mut [T]>, Refused> {
        if raw.is_null() {
            return Ok(None);
        }
        // SAFETY: the ABI's promise for this kind is `Buffer`'s for a
        // non-null pointer.
        unsafe { Buffer::<T, C>::from_abi(raw, count, call) }.map(Some)
    }
}

/// A list of structs that start with headers, a pointer and a count, each
/// copied as [`Headed`] copies one: [`Status::BAD_ARGS`] for a null pointer
/// with entries, entries of different sizes or one too short.
///
/// The ABI promises `count` consecutive entries, each starting with its
/// header and readable for the size the first one states, with no other
/// writer during the call.
pub(crate) struct HeadedList<T>(PhantomData<T>);

impl<T: enshwick_core::abi::Headed> Arg2 for HeadedList<T> {
    type First = *const T;
    type Second = u32;
    type Value<'call>
        = Vec<T>
    where
        Self: 'call;

    unsafe fn from_abi(raw: *const T, count: u32, _: &Call) -> Result<Vec<T>, Refused> {
        // SAFETY: the ABI's promise for this kind is `headed_list_in`'s.
        unsafe { memory::headed_list_in(raw, count, T::BORN) }.ok_or(BAD_ARGS)
    }
}

/// Where the mod wants a list of plain values: the list, its capacity and
/// where the total goes. A null list only asks the total.
///
/// The ABI promises the list null or `capacity` writable, aligned values and
/// the total null or a writable, aligned `u32`, untouched by others during
/// the call.
pub(crate) struct ListOut<T>(PhantomData<T>);

/// The places a [`ListOut`] names.
pub(crate) struct ListSlot<'call, T> {
    out: *mut T,
    capacity: u32,
    count: *mut u32,
    call: PhantomData<&'call mut T>,
}

impl<T> Arg3 for ListOut<T> {
    type First = *mut T;
    type Second = u32;
    type Third = *mut u32;
    type Value<'call>
        = ListSlot<'call, T>
    where
        Self: 'call;

    unsafe fn from_abi(
        out: *mut T,
        capacity: u32,
        count: *mut u32,
        _: &Call,
    ) -> Result<ListSlot<'_, T>, Refused> {
        Ok(ListSlot {
            out,
            capacity,
            count,
            call: PhantomData,
        })
    }
}

impl<T: Copy> ListSlot<'_, T> {
    /// Writes as many of `items` as fit and their total.
    ///
    /// # Errors
    ///
    /// [`Status::BAD_ARGS`] for a null total.
    pub(crate) fn put(self, items: &[T]) -> Result<(), Refused> {
        // SAFETY: the ABI's promise for `ListOut` is `put_list`'s, for the
        // call this slot borrows.
        checked(unsafe { memory::put_list(items, self.out, self.capacity, self.count) })
    }
}

/// Where the mod wants a list of structs that start with headers, each as
/// large as the first one's header says: the list, its capacity and where
/// the total goes. A null list only asks the total.
///
/// The ABI promises the list null or `capacity` consecutive entries, each
/// starting with its header and writable for the size the first one states,
/// and the total null or a writable, aligned `u32`, untouched by others
/// during the call.
pub(crate) struct HeadedListOut<T>(PhantomData<T>);

/// The places a [`HeadedListOut`] names.
pub(crate) struct HeadedListSlot<'call, T> {
    out: *mut T,
    capacity: u32,
    count: *mut u32,
    call: PhantomData<&'call mut T>,
}

impl<T: enshwick_core::abi::Headed> Arg3 for HeadedListOut<T> {
    type First = *mut T;
    type Second = u32;
    type Third = *mut u32;
    type Value<'call>
        = HeadedListSlot<'call, T>
    where
        Self: 'call;

    unsafe fn from_abi(
        out: *mut T,
        capacity: u32,
        count: *mut u32,
        _: &Call,
    ) -> Result<HeadedListSlot<'_, T>, Refused> {
        Ok(HeadedListSlot {
            out,
            capacity,
            count,
            call: PhantomData,
        })
    }
}

impl<T: enshwick_core::abi::Headed> HeadedListSlot<'_, T> {
    /// Writes as many of `items` as fit, as far as both sides know an entry,
    /// and their total.
    ///
    /// # Errors
    ///
    /// [`Status::BAD_ARGS`] for a first entry shorter than a header or a
    /// null total.
    pub(crate) fn put(self, items: &[T]) -> Result<(), Refused> {
        // SAFETY: the ABI's promise for `HeadedListOut` is
        // `put_headed_list`'s, for the call this slot borrows.
        checked(unsafe { memory::put_headed_list(items, self.out, self.capacity, self.count) })
    }
}

/// Where the mod wants a list of headed entries of the size it states: the
/// list, the size of an entry, its capacity and where the total goes. A null
/// list only asks the total.
///
/// The ABI promises the list null or writable for `capacity` entries of that
/// size and the total null or a writable, aligned `u32`, untouched by others
/// during the call; no alignment is needed for the list.
pub(crate) struct SizedListOut<T>(PhantomData<T>);

/// The places a [`SizedListOut`] names.
pub(crate) struct SizedListSlot<'call, T> {
    out: *mut T,
    stride: u32,
    capacity: u32,
    count: *mut u32,
    call: PhantomData<&'call mut T>,
}

impl<T> Arg4 for SizedListOut<T> {
    type First = *mut T;
    type Second = u32;
    type Third = u32;
    type Fourth = *mut u32;
    type Value<'call>
        = SizedListSlot<'call, T>
    where
        Self: 'call;

    unsafe fn from_abi(
        out: *mut T,
        stride: u32,
        capacity: u32,
        count: *mut u32,
        _: &Call,
    ) -> Result<SizedListSlot<'_, T>, Refused> {
        Ok(SizedListSlot {
            out,
            stride,
            capacity,
            count,
            call: PhantomData,
        })
    }
}

impl<T: Copy> SizedListSlot<'_, T> {
    /// Writes as many of `items` as fit, as far as both sides know an entry,
    /// and their total.
    ///
    /// # Errors
    ///
    /// [`Status::BAD_ARGS`] for an entry size shorter than a header or a
    /// null total.
    pub(crate) fn put(self, items: &[T]) -> Result<(), Refused> {
        // SAFETY: the ABI's promise for `SizedListOut` is `put_sized_list`'s,
        // for the call this slot borrows.
        checked(unsafe {
            memory::put_sized_list(items, self.out, self.stride, self.capacity, self.count)
        })
    }
}

/// Where the mod wants bytes: a buffer, its capacity and where the length
/// goes. A null buffer only asks the length.
///
/// The ABI promises the buffer null or writable for `capacity` bytes apart
/// from the loader's, and the length null or a writable, aligned `u32`,
/// untouched by others during the call.
pub(crate) struct BytesOut;

/// The places a [`BytesOut`] names.
pub(crate) struct BytesSlot<'call> {
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
    call: PhantomData<&'call mut u8>,
}

impl Arg3 for BytesOut {
    type First = *mut u8;
    type Second = u32;
    type Third = *mut u32;
    type Value<'call>
        = BytesSlot<'call>
    where
        Self: 'call;

    unsafe fn from_abi(
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
        _: &Call,
    ) -> Result<BytesSlot<'_>, Refused> {
        Ok(BytesSlot {
            out,
            capacity,
            len,
            call: PhantomData,
        })
    }
}

impl BytesSlot<'_> {
    /// The places at `out`, `capacity` and `len`, for a test of code that
    /// writes through a slot.
    ///
    /// # Safety
    ///
    /// As [`BytesOut`]'s promise, for as long as the slot lives.
    #[cfg(test)]
    pub(crate) unsafe fn from_raw(out: *mut u8, capacity: u32, len: *mut u32) -> Self {
        Self {
            out,
            capacity,
            len,
            call: PhantomData,
        }
    }

    /// Whether the mod only asks the length, passing no buffer.
    pub(crate) fn asks_length(&self) -> bool {
        self.out.is_null()
    }

    /// Writes `data` and its length.
    ///
    /// # Errors
    ///
    /// [`Status::BAD_ARGS`] for a short or null buffer, the length written
    /// so the mod can grow its buffer; [`Status::REFUSED`] past `u32::MAX`
    /// bytes.
    pub(crate) fn put(&self, data: &[u8]) -> Result<(), Refused> {
        // SAFETY: the ABI's promise for `BytesOut` is `copy_out`'s, for the
        // call this slot borrows; `data` is the loader's own.
        checked(unsafe { memory::copy_out(data, self.out, self.capacity, self.len) })
    }
}
