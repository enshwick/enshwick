//! The shape of a type that crosses between mods: what the layout checks of
//! services, mod events and extension points hash for every field.
//!
//! A field's hash covers its name, place and size, and the [`Shape`] of its
//! type: for a struct every field inside it, for an enum every variant with
//! its discriminant, for a newtype the named values it lists, for `bool`
//! its two values. Two mods built against declarations whose meaning
//! differs anywhere down the tree, even at the same size, are refused with
//! the name of the outer field. [`#[derive(Shape)]`](macro@crate::Shape)
//! writes the implementation for a type of the mod's own.

use core::ffi::c_void;
use core::marker::PhantomData;
use core::num::{NonZeroU8, NonZeroU16, NonZeroU32, NonZeroU64};
use core::ptr::NonNull;

/// A type with a structural hash, so a field of it can be checked between
/// mods.
///
/// # Safety
///
/// - Implemented with [`#[derive(Shape)]`](macro@crate::Shape).
/// - [`SHAPE`](Shape::SHAPE) changes whenever the type's layout or the
///   meaning of its values changes: every field with its offset, every
///   variant with its discriminant, every named value.
///
/// # Examples
///
/// ```
/// use enshwick::Shape;
///
/// #[derive(Clone, Copy, Debug, Shape)]
/// #[repr(u8)]
/// pub enum Coin {
///     Copper = 1,
///     Silver = 2,
/// }
///
/// assert_ne!(Coin::SHAPE, u8::SHAPE);
/// ```
pub unsafe trait Shape {
    /// The type's structural hash.
    const SHAPE: u64;

    /// What a point schema calls the type: a primitive's name such as
    /// `u32` or `bool`, `enshwick.<name>` for a type of this crate, or the
    /// type's own name; empty for a type a schema cannot describe.
    const SCHEMA_NAME: &'static str = "";

    /// The `[[type]]` tables of a point schema for the type and every named
    /// type inside it; empty for a primitive or a type of this crate.
    const SCHEMA_TYPES: &'static str = "";
}

/// Builds a [`Shape`] hash, FNV-1a over the parts in order.
///
/// # Examples
///
/// ```
/// use enshwick::shape::ShapeHasher;
///
/// let a = ShapeHasher::new("enum").text("Coin").number(1).finish();
/// assert_ne!(a, ShapeHasher::new("enum").text("Coin").number(2).finish());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeHasher(u64);

impl ShapeHasher {
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    /// A hash that starts with `kind`, such as `struct` or `enum`.
    ///
    /// # Examples
    ///
    /// See [`ShapeHasher`].
    #[must_use]
    pub const fn new(kind: &str) -> Self {
        Self(0xcbf2_9ce4_8422_2325).text(kind)
    }

    /// Adds `text` and its length, so `ab`, `c` and `a`, `bc` differ.
    ///
    /// # Examples
    ///
    /// See [`ShapeHasher`].
    #[must_use]
    pub const fn text(self, text: &str) -> Self {
        let mut hash = self.number(text.len() as u64).0;
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(Self::PRIME);
            i += 1;
        }
        Self(hash)
    }

    /// Adds `number`, such as an offset, a discriminant or another shape.
    ///
    /// # Examples
    ///
    /// See [`ShapeHasher`].
    #[must_use]
    pub const fn number(self, number: u64) -> Self {
        let mut hash = self.0;
        let bytes = number.to_le_bytes();
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(Self::PRIME);
            i += 1;
        }
        Self(hash)
    }

    /// Adds the size and alignment of `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::shape::ShapeHasher;
    ///
    /// let a = ShapeHasher::new("struct").layout::<u32>().finish();
    /// assert_ne!(a, ShapeHasher::new("struct").layout::<u64>().finish());
    /// ```
    #[must_use]
    pub const fn layout<T>(self) -> Self {
        self.number(size_of::<T>() as u64)
            .number(align_of::<T>() as u64)
    }

    /// Adds a field's offset.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::shape::ShapeHasher;
    ///
    /// let a = ShapeHasher::new("struct").offset(0).finish();
    /// assert_ne!(a, ShapeHasher::new("struct").offset(4).finish());
    /// ```
    #[must_use]
    pub const fn offset(self, offset: usize) -> Self {
        self.number(offset as u64)
    }

    /// Adds a discriminant or a named value, sign and all.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::shape::ShapeHasher;
    ///
    /// let a = ShapeHasher::new("enum").value(-1).finish();
    /// assert_ne!(a, ShapeHasher::new("enum").value(1).finish());
    /// ```
    #[must_use]
    pub const fn value(self, value: i128) -> Self {
        let bits = value.to_le_bytes();
        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        let mut i = 0;
        while i < 8 {
            low[i] = bits[i];
            high[i] = bits[i + 8];
            i += 1;
        }
        self.number(u64::from_le_bytes(low))
            .number(u64::from_le_bytes(high))
    }

    /// The hash.
    ///
    /// # Examples
    ///
    /// See [`ShapeHasher`].
    #[must_use]
    pub const fn finish(self) -> u64 {
        self.0
    }
}

macro_rules! leaf {
    ($($ty:ty),* $(,)?) => {$(
        // SAFETY: a primitive's shape is its name, which fixes its layout
        // and values.
        unsafe impl Shape for $ty {
            const SHAPE: u64 = ShapeHasher::new(stringify!($ty)).finish();
        }
    )*};
}

macro_rules! number {
    ($($ty:ty),* $(,)?) => {$(
        // SAFETY: a number's shape is its name, which fixes its layout and
        // values.
        unsafe impl Shape for $ty {
            const SHAPE: u64 = ShapeHasher::new(stringify!($ty)).finish();
            const SCHEMA_NAME: &'static str = stringify!($ty);
        }
    )*};
}

number!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

leaf!(
    (),
    u128,
    usize,
    i128,
    isize,
    char,
    str,
    c_void,
    NonZeroU8,
    NonZeroU16,
    NonZeroU32,
    NonZeroU64,
);

// SAFETY: `bool` has the values 0 and 1, both in the hash.
unsafe impl Shape for bool {
    const SHAPE: u64 = ShapeHasher::new("bool")
        .text("false")
        .number(0)
        .text("true")
        .number(1)
        .finish();
    const SCHEMA_NAME: &'static str = "bool";
}

// SAFETY: an array is its element's shape repeated `N` times.
unsafe impl<T: Shape, const N: usize> Shape for [T; N] {
    const SHAPE: u64 = ShapeHasher::new("array")
        .number(T::SHAPE)
        .number(N as u64)
        .finish();
}

// SAFETY: a slice is its element's shape.
unsafe impl<T: Shape> Shape for [T] {
    const SHAPE: u64 = ShapeHasher::new("slice").number(T::SHAPE).finish();
}

// SAFETY: `Option` adds its tag or niche to the shape inside it.
unsafe impl<T: Shape> Shape for Option<T> {
    const SHAPE: u64 = ShapeHasher::new("Option").number(T::SHAPE).finish();
}

// SAFETY: `PhantomData` holds nothing.
unsafe impl<T: ?Sized> Shape for PhantomData<T> {
    const SHAPE: u64 = ShapeHasher::new("PhantomData").finish();
}

macro_rules! pointer {
    ($($kind:literal $ty:ty),* $(,)?) => {$(
        // SAFETY: a pointer is the shape of what it points at.
        unsafe impl<T: Shape + ?Sized> Shape for $ty {
            const SHAPE: u64 = ShapeHasher::new($kind).number(T::SHAPE).finish();
        }
    )*};
}

pointer!(
    "*const" *const T,
    "*mut" *mut T,
    "&" &T,
    "&mut" &mut T,
    "NonNull" NonNull<T>,
);

macro_rules! function {
    ($($arg:ident)*) => {
        function!(@abi "Rust" fn($($arg),*) -> R; $($arg)*);
        function!(@abi "C" extern "C" fn($($arg),*) -> R; $($arg)*);
        function!(@abi "system" extern "system" fn($($arg),*) -> R; $($arg)*);
        function!(@abi "unsafe Rust" unsafe fn($($arg),*) -> R; $($arg)*);
        function!(@abi "unsafe C" unsafe extern "C" fn($($arg),*) -> R; $($arg)*);
        function!(@abi "unsafe system" unsafe extern "system" fn($($arg),*) -> R; $($arg)*);
    };
    (@abi $abi:literal $ty:ty; $($arg:ident)*) => {
        // SAFETY: a function pointer is its calling convention, its
        // arguments' shapes in order and its return's shape.
        unsafe impl<R: Shape, $($arg: Shape),*> Shape for $ty {
            const SHAPE: u64 = ShapeHasher::new("fn")
                .text($abi)
                .number(R::SHAPE)
                $(.number($arg::SHAPE))*
                .finish();
        }
    };
}

function!();
function!(A);
function!(A B);
function!(A B C);
function!(A B C D);
function!(A B C D E);
function!(A B C D E F);
function!(A B C D E F G);
function!(A B C D E F G H);

#[cfg(test)]
mod tests {
    use super::Shape;

    #[test]
    fn primitives_differ() {
        let shapes = [
            u8::SHAPE,
            i8::SHAPE,
            bool::SHAPE,
            u32::SHAPE,
            f32::SHAPE,
            <[u8; 4]>::SHAPE,
            <[u8; 2]>::SHAPE,
            <*const u32>::SHAPE,
            <*mut u32>::SHAPE,
            <extern "system" fn(u32) -> u32>::SHAPE,
            <extern "C" fn(u32) -> u32>::SHAPE,
            <extern "system" fn(u64) -> u32>::SHAPE,
            <extern "system" fn(u32, u32) -> u32>::SHAPE,
        ];
        for (i, a) in shapes.iter().enumerate() {
            for b in &shapes[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
