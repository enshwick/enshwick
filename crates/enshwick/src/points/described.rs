//! Extension points described at run time, such as by the schema a script
//! carries: the same link as [`Load::extend_point`](crate::Load::extend_point),
//! with the input and the answer as bytes in the owner's layout.

use core::ffi::c_void;

use crate::abi::{Header, Layout, LayoutField, PointFlags, PointVerdict, Str};
use crate::load::hand_over;
use crate::points::{Mode, Order};
use crate::{Load, Off};

/// A field of a described input or answer: its name and the layout hash the
/// owner's build wrote for it.
///
/// # Examples
///
/// See [`DescribedPoint`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DescribedField {
    /// The field's name; `output` for the answer.
    pub name: String,
    /// Its layout hash, as the owner's schema gives it.
    pub hash: u64,
}

impl DescribedField {
    /// The field `name` with the layout hash `hash`.
    ///
    /// # Examples
    ///
    /// See [`DescribedPoint`].
    #[must_use]
    pub fn new(name: &str, hash: u64) -> Self {
        Self {
            name: name.to_owned(),
            hash,
        }
    }
}

/// Another mod's extension point as its schema describes it, for a link
/// that reads the input and writes the answer as bytes.
///
/// # Examples
///
/// ```
/// use enshwick::points::{DescribedField, DescribedPoint, DescribedVerdict, Mode, Order};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let price = DescribedPoint::new("you.trading-post:price", Mode::Override)
///         .with_input(vec![DescribedField::new("item", 5_938_679_357_011_966_802)], 4)
///         .with_output(14_918_346_923_295_848_190, 4);
///     // SAFETY: every answer set is a `u32`, the point's answer type, for
///     // which any bytes are a valid value.
///     unsafe {
///         load.extend_described_point(&price, &Order::new(), |input, _answer| {
///             let item = u32::from_le_bytes(input[..4].try_into().unwrap_or_default());
///             if item == 7 { DescribedVerdict::Set(10u32.to_le_bytes().to_vec()) } else { DescribedVerdict::Pass }
///         })
///     }
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DescribedPoint {
    /// The point's name, `<owner id>:<point>`.
    pub name: String,
    /// How links act on the answer.
    pub mode: Mode,
    /// Whether a link may refuse.
    pub refusable: bool,
    /// The input's fields this link reads, in the owner's order.
    pub input: Vec<DescribedField>,
    /// The size of those fields, the bytes a link reads.
    pub input_size: u32,
    /// The answer's layout hash.
    pub output_hash: u64,
    /// The answer's size in bytes.
    pub output_size: u32,
}

impl DescribedPoint {
    /// The point `name` in `mode`, with no input or answer yet.
    ///
    /// # Examples
    ///
    /// See [`DescribedPoint`].
    #[must_use]
    pub fn new(name: &str, mode: Mode) -> Self {
        Self {
            name: name.to_owned(),
            mode,
            refusable: false,
            input: Vec::new(),
            input_size: 0,
            output_hash: 0,
            output_size: 0,
        }
    }

    /// The same with the input's `fields`, `size` bytes.
    ///
    /// # Examples
    ///
    /// See [`DescribedPoint`].
    #[must_use]
    pub fn with_input(mut self, fields: Vec<DescribedField>, size: u32) -> Self {
        self.input = fields;
        self.input_size = size;
        self
    }

    /// The same with the answer's layout `hash` and `size`.
    ///
    /// # Examples
    ///
    /// See [`DescribedPoint`].
    #[must_use]
    pub const fn with_output(mut self, hash: u64, size: u32) -> Self {
        self.output_hash = hash;
        self.output_size = size;
        self
    }

    /// The same, refusable or not.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::points::{DescribedPoint, Mode};
    ///
    /// assert!(DescribedPoint::new("you.post:price", Mode::Override).refusable(true).refusable);
    /// ```
    #[must_use]
    pub const fn refusable(mut self, refusable: bool) -> Self {
        self.refusable = refusable;
        self
    }
}

/// What a described link answers.
///
/// # Examples
///
/// See [`DescribedPoint`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DescribedVerdict {
    /// The answer stays.
    Pass,
    /// The answer becomes these bytes, the answer's size.
    Set(Vec<u8>),
    /// The link refuses, on a refusable point.
    Refuse,
}

/// A described link with the sizes it reads and writes.
struct Link<F> {
    input_size: usize,
    output_size: usize,
    changes: bool,
    refusable: bool,
    run: F,
}

impl Load<'_> {
    /// Links `link` into another mod's extension point described by
    /// `point`, as [`extend_point`](Self::extend_point) does for a type:
    /// the loader checks the layout hashes against the owner's and orders
    /// the link by `order`. `link` gets the input's bytes, `input_size` of
    /// them, and the answer so far.
    ///
    /// At the seal the loader leaves the link idle, with the reason in the
    /// log, when a hash differs, the description reads past the owner's
    /// input or its answer has another size.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the link, as after `init` or for a
    /// second link of this mod on the point.
    ///
    /// # Safety
    ///
    /// - The bytes of every [`DescribedVerdict::Set`] the link answers are a
    ///   valid value of the owner's answer type: a `bool` is 0 or 1, an enum
    ///   one of its variants, every field of a struct likewise.
    ///
    /// # Examples
    ///
    /// See [`DescribedPoint`].
    pub unsafe fn extend_described_point<F>(
        &mut self,
        point: &DescribedPoint,
        order: &Order,
        link: F,
    ) -> Result<(), Off>
    where
        F: Fn(&[u8], &[u8]) -> DescribedVerdict + Send + Sync + 'static,
    {
        let input: Vec<LayoutField> = point
            .input
            .iter()
            .map(|f| LayoutField {
                name: Str::new(&f.name),
                hash: f.hash,
            })
            .collect();
        let output = [LayoutField {
            name: Str::new("output"),
            hash: point.output_hash,
        }];
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let spec = crate::abi::PointSpec {
            header: Header::of::<crate::abi::PointSpec>(),
            name: Str::new(&point.name),
            mode: point.mode.to_abi(),
            flags: if point.refusable {
                PointFlags::REFUSE
            } else {
                PointFlags::NONE
            },
            input: Layout {
                fields: input.as_ptr(),
                count: count(input.len()),
                size: point.input_size,
            },
            output: Layout {
                fields: output.as_ptr(),
                count: 1,
                size: point.output_size,
            },
        };
        let before: Vec<Str> = order.before.iter().map(|id| Str::new(id)).collect();
        let after: Vec<Str> = order.after.iter().map(|id| Str::new(id)).collect();
        let raw = crate::abi::PointLink {
            header: Header::of::<crate::abi::PointLink>(),
            point: &raw const spec,
            before: before.as_ptr(),
            after: after.as_ptr(),
            before_count: count(before.len()),
            after_count: count(after.len()),
            priority: order.priority,
        };
        let host = self.host();
        let link = Link {
            input_size: usize::try_from(point.input_size).unwrap_or(0),
            output_size: usize::try_from(point.output_size).unwrap_or(0),
            changes: point.mode.changes(),
            refusable: point.refusable,
            run: link,
        };
        hand_over(
            link,
            |user| {
                (host.api().attach_point)(host.handle(), &raw const raw, run_described::<F>, user)
            },
            &format!("point {}", point.name),
        )
    }
}

/// Runs a described link; `user` is the [`Link`] boxed by
/// [`Load::extend_described_point`], alive for the process.
unsafe extern "system" fn run_described<F>(
    input: *const c_void,
    value: *mut c_void,
    user: *mut c_void,
) -> PointVerdict
where
    F: Fn(&[u8], &[u8]) -> DescribedVerdict,
{
    // SAFETY: `user` is the leaked `Box<Link<F>>` this function was attached
    // with.
    let link = unsafe { &*user.cast::<Link<F>>() };
    if input.is_null() || value.is_null() {
        return PointVerdict::PASS;
    }
    // SAFETY: at the seal the loader checked the owner's input starts with
    // every described field and is at least `input_size` bytes, and the
    // input lives for the call.
    let input = unsafe { core::slice::from_raw_parts(input.cast::<u8>(), link.input_size) };
    // SAFETY: at the seal the loader checked the owner's answer has the
    // described layout and exactly `output_size` bytes, alive and written by
    // nothing else for the call.
    let answer = unsafe { core::slice::from_raw_parts_mut(value.cast::<u8>(), link.output_size) };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (link.run)(input, answer))) {
        Ok(DescribedVerdict::Set(bytes)) if link.changes && bytes.len() == answer.len() => {
            answer.copy_from_slice(&bytes);
            PointVerdict::SET
        }
        Ok(DescribedVerdict::Refuse) if link.refusable && link.changes => PointVerdict::REFUSE,
        Ok(_) => PointVerdict::PASS,
        Err(_) => PointVerdict::PANICKED,
    }
}
