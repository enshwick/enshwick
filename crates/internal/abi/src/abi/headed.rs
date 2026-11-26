//! Structs that start with a [`Header`]: how each side reads, writes and
//! builds them across versions.

use super::Header;

/// A struct that starts with a [`Header`] and crosses the boundary only as
/// far as both sides know it, through [`read_headed`] and [`write_headed`].
///
/// [`Zeroable`](bytemuck::Zeroable) makes the fields the writer does not know
/// read as zero.
///
/// # Safety
///
/// - The type is `repr(C)` and its first field is a [`Header`] at offset 0.
/// - Any bytes the other side writes for the fields it knows form a valid
///   value of those fields, which holds for the integer, pointer and
///   function pointer fields the ABI uses.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Headed, Header, TickData};
///
/// fn size_of_headed<T: Headed>() -> usize {
///     size_of::<T>()
/// }
/// assert_eq!(size_of_headed::<TickData>(), Header::of::<TickData>().size as usize);
/// ```
pub unsafe trait Headed: bytemuck::Zeroable + Copy {
    /// Where the fields every writer of this [`ABI_MAJOR`](super::ABI_MAJOR) knows end: the
    /// struct's size at minor 0 of the major. A reader of a struct the other
    /// side wrote requires this much, and reads a later field only where
    /// the header's `size` covers it; the layout tests hold it to the golden file.
    const BORN: usize;

    /// A value with this side's [`Header`] and every other field zero, to
    /// set field by field; [`headed!`](crate::abi::headed) builds from it.
    ///
    /// A headed struct gains fields in later minors and is
    /// `#[non_exhaustive]`, so a writer outside this crate never lists them
    /// all; the fields it leaves are zero, as an older writer's are.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Headed, Header, TickData};
    ///
    /// let tick = TickData::blank();
    /// assert_eq!(tick.header, Header::of::<TickData>());
    /// ```
    #[must_use]
    fn blank() -> Self {
        let mut value: Self = bytemuck::Zeroable::zeroed();
        // SAFETY: an implementor's first field is a `Header` at offset 0, as
        // the trait's contract states, and `value` is a live, aligned `Self`.
        unsafe {
            (&raw mut value)
                .cast::<Header>()
                .write(Header::of::<Self>());
        }
        value
    }
}

/// Builds a headed ABI struct as a struct expression would, with the fields
/// it names and every other field zero, as an older writer leaves them.
///
/// The header is [`Header::of`] the type unless the fields name one, and
/// `..base` starts from `base` instead of zero, as in a struct expression. A
/// headed struct is `#[non_exhaustive]`, since a minor appends fields to it,
/// so this, or [`Headed::blank`] and the fields set one by one, is how code
/// outside the ABI crate writes one.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Headed, TickData};
///
/// let tick = enshwick::abi::headed!(TickData { step: 7 });
/// assert_eq!(tick.step, 7);
/// assert_eq!(tick.header, TickData::blank().header);
/// ```
#[macro_export]
macro_rules! headed {
    ($ty:path { $($(#[$attr:meta])* $field:ident $(: $value:expr)?,)* ..$base:expr }) => {{
        let mut value: $ty = $base;
        value.header = $crate::abi::Header::of::<$ty>();
        $($(#[$attr])* { value.$field = $crate::headed!(@value $field $(, $value)?); })*
        value
    }};
    ($ty:path { $($(#[$attr:meta])* $field:ident $(: $value:expr)?),* $(,)? }) => {{
        let mut value = <$ty as $crate::abi::Headed>::blank();
        $($(#[$attr])* { value.$field = $crate::headed!(@value $field $(, $value)?); })*
        value
    }};
    (@value $field:ident, $value:expr) => {
        $value
    };
    (@value $field:ident) => {
        $field
    };
}

/// Implements [`Headed`] for an ABI struct beside its definition and checks at
/// compile time that its [`Header`] sits at offset 0. `born` is the struct's
/// size at minor 0 of this major, once a minor appended a field to it.
macro_rules! impl_headed {
    ($ty:ty) => {
        impl_headed!($ty, born = size_of::<$ty>());
    };
    ($ty:ty, born = $born:expr) => {
        // SAFETY: the struct is `repr(C)`, its first field is the `Header`
        // checked at offset 0 below, and its other fields are integers,
        // pointers and `Option`s of function pointers, valid for any bytes
        // the other side writes.
        unsafe impl $crate::abi::Headed for $ty {
            const BORN: usize = $born;
        }
        const _: () = assert!(core::mem::offset_of!($ty, header) == 0);
        const _: () =
            assert!($born >= size_of::<$crate::abi::Header>() && $born <= size_of::<$ty>());
    };
}

/// Copies a struct that starts with a [`Header`] from the other side, which
/// may know an older or a newer version of it.
///
/// Reads only the bytes the writer's header covers; the fields it does not
/// know stay zero. `None` when `raw` is null or the writer's struct ends
/// before byte `required`, the end of the fields every version has. The copy
/// keeps the writer's header, so check the header's `size` before trusting a
/// field newer than `required` that zero does not default well.
///
/// # Safety
///
/// - `raw` is null, or points at a `T` the other side wrote, readable for the
///   size its header states; no alignment is needed.
/// - No other thread writes that struct during the copy.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{TickData, read_headed};
///
/// let mut old = TickData::new(7);
/// old.header.size = 8;
/// // SAFETY: `old` is a tick readable for the eight bytes its header states.
/// let tick = unsafe { read_headed(&raw const old, 8) }.ok_or("too short")?;
/// assert_eq!(tick.step, 0);
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub unsafe fn read_headed<T: Headed>(raw: *const T, required: usize) -> Option<T> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is non-null and the caller guarantees it is readable for
    // its header, the first eight bytes of every `Headed` type.
    // The other side's headed struct, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let header = unsafe { raw.cast::<Header>().read_unaligned() };
    if !header.covers(required.max(size_of::<Header>())) {
        return None;
    }
    if header.covers(size_of::<T>()) {
        // SAFETY: the caller guarantees `raw` readable for `header.size`
        // bytes, which the check above found at least a whole `T`, and
        // `Headed` makes any bytes the writer wrote a valid `T`.
        // The other side's headed struct, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        return Some(unsafe { raw.read_unaligned() });
    }
    let known = (header.size as usize).min(size_of::<T>());
    let mut out = T::zeroed();
    // SAFETY: the caller guarantees `raw` readable for `header.size` bytes and
    // `out` is a local `T`; `known` is at most both sizes, and the two do not
    // overlap.
    unsafe {
        core::ptr::copy_nonoverlapping(raw.cast::<u8>(), (&raw mut out).cast::<u8>(), known);
    };
    Some(out)
}

/// Writes `value`, a struct that starts with a [`Header`], to the other side,
/// which may know an older or a newer version of it.
///
/// Writes only the bytes both sides know, and in the written header the
/// size of what it wrote. `false`, writing nothing, when `out` is null or its
/// header is shorter than a [`Header`].
///
/// # Safety
///
/// - `out` is null, or points at a `T` whose header states its size, writable
///   for that many bytes; no alignment is needed.
/// - Nothing else reads or writes that struct during the write.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{TickData, write_headed};
///
/// let mut old = TickData::new(0);
/// old.header.size = 8;
/// let new = TickData::new(7);
/// // SAFETY: `old` is a tick writable for the eight bytes its header states.
/// assert!(unsafe { write_headed(&raw mut old, new) });
/// assert_eq!((old.header.size, old.step), (8, 0));
/// ```
#[must_use]
pub unsafe fn write_headed<T: Headed>(out: *mut T, value: T) -> bool {
    if out.is_null() {
        return false;
    }
    // SAFETY: `out` is non-null and the caller guarantees it is readable for
    // its header, the first eight bytes of every `Headed` type.
    // The other side's headed out struct, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let room = unsafe { out.cast::<Header>().read_unaligned() }.size as usize;
    if room < size_of::<Header>() {
        return false;
    }
    let known = room.min(size_of::<T>());
    // SAFETY: the caller guarantees `out` writable for `room` bytes and
    // `value` is a local `T`; `known` is at most both sizes, and the two do
    // not overlap.
    unsafe {
        core::ptr::copy_nonoverlapping((&raw const value).cast::<u8>(), out.cast(), known);
    };
    // `known` is at most `room`, a `u32`.
    let size = u32::try_from(known).unwrap_or(u32::MAX);
    // SAFETY: `size` is the header's first field, inside the `room` bytes the
    // caller guarantees writable.
    unsafe { out.cast::<u32>().write_unaligned(size) };
    true
}
