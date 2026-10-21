//! Typed services and events between mods.
//!
//! A service is a `'static` table of functions one mod publishes for others;
//! a mod event is plain data one mod sends to every mod that subscribed.
//! Both are declared once, with [`#[service]`](macro@crate::service) and
//! [`#[event]`](macro@crate::event), in a crate or file both mods compile,
//! and the loader checks at load time that both sides agree on the layout,
//! field by field. Tables and events grow by appending fields: a user built
//! against an older declaration keeps working with a newer publisher, and a
//! user that needs a field the publisher lacks is refused with the field's
//! name. A change to a field already published is a new major version.

use core::marker::PhantomData;

use crate::Host;
use crate::abi::{Layout, LayoutField, Str};

/// A table of functions a mod publishes for others.
///
/// # Safety
///
/// Implemented by [`#[service]`](macro@crate::service) only: [`LAYOUT`]
/// must list every field of the `repr(C)` type in order, with its offset,
/// size and alignment in the hash.
///
/// [`LAYOUT`]: Service::LAYOUT
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::services::Service;
///
/// #[enshwick_loader_api::service(name = "trading-post.prices", version = 1)]
/// pub struct Prices {
///     pub price: extern "system" fn(item: u32) -> u32,
/// }
///
/// assert_eq!(Prices::NAME, "trading-post.prices");
/// assert_eq!(Prices::LAYOUT.len(), 1);
/// ```
pub unsafe trait Service: Sync + 'static {
    /// The service's name, `<publisher id>.<what>`.
    const NAME: &'static str;
    /// The incompatible version; a changed field bumps it.
    const MAJOR: u32;
    /// The table's fields, in order.
    const LAYOUT: &'static [LayoutField];
}

/// Plain data one mod sends to the mods that subscribed to it.
///
/// # Safety
///
/// Implemented by [`#[event]`](macro@crate::event) only: [`LAYOUT`] must
/// list every field of the `repr(C)` type in order, with its offset, size
/// and alignment in the hash.
///
/// [`LAYOUT`]: ModEvent::LAYOUT
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::services::ModEvent;
///
/// #[enshwick_loader_api::event(name = "trading-post.sold")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Sold {
///     pub item: u32,
///     pub price: u32,
/// }
///
/// assert_eq!(Sold::NAME, "trading-post.sold");
/// assert_eq!(Sold::LAYOUT.len(), 2);
/// ```
pub unsafe trait ModEvent: Copy + 'static {
    /// The event's name, `<defining mod id>.<what>`.
    const NAME: &'static str;
    /// The data's fields, in order.
    const LAYOUT: &'static [LayoutField];
}

/// The hash of one field for a layout: FNV-1a over the field as written,
/// `name: type` without its whitespace, then its size, alignment and offset.
///
/// The macros call it; the type counts as written, so both sides name it
/// the same way, which one shared declaration gives. Whitespace does not
/// count, since compilers print the same tokens with different spacing.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::services::layout_hash;
///
/// let a = layout_hash("price: u32", 4, 4, 0);
/// assert_ne!(a, layout_hash("price: u32", 4, 4, 4));
/// assert_ne!(a, layout_hash("price: i32", 4, 4, 0));
/// assert_eq!(
///     layout_hash("best: Option<u32>", 8, 4, 0),
///     layout_hash("best : Option < u32 >", 8, 4, 0)
/// );
/// ```
#[must_use]
pub const fn layout_hash(written: &str, size: usize, align: usize, offset: usize) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let bytes = written.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_whitespace() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(PRIME);
        }
        i += 1;
    }
    let numbers = [size as u64, align as u64, offset as u64];
    let mut n = 0;
    while n < numbers.len() {
        let value = numbers[n].to_le_bytes();
        let mut b = 0;
        while b < value.len() {
            hash ^= value[b] as u64;
            hash = hash.wrapping_mul(PRIME);
            b += 1;
        }
        n += 1;
    }
    hash
}

/// Why a published layout does not serve the one a mod asks for.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::services::{Mismatch, compare};
///
/// let wanted = [("price", 1), ("refund", 2)];
/// let offered = [("price", 1)];
/// assert!(matches!(compare(&wanted, &offered), Err(Mismatch::Newer { .. })));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Mismatch {
    /// The asking side was built against a longer declaration.
    #[error(
        "the publisher has {offered} of the {wanted} fields this mod needs: `{first}` and later are newer than the publisher"
    )]
    Newer {
        /// Fields the asking side needs.
        wanted: usize,
        /// Fields the publisher has.
        offered: usize,
        /// The first field the publisher lacks.
        first: String,
    },
    /// A field both sides have differs.
    #[error(
        "field {index} is `{wanted}` here and `{offered}` at the publisher, with another type, size or place"
    )]
    Field {
        /// The field's position, from 0.
        index: usize,
        /// Its name on the asking side.
        wanted: String,
        /// Its name at the publisher.
        offered: String,
    },
}

/// Checks that the `offered` layout starts with the `wanted` one, each as
/// `(name, hash)` per field.
///
/// # Errors
///
/// [`Mismatch`] with the first field that differs or is missing.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::services::compare;
///
/// let older = [("price", 1)];
/// let newer = [("price", 1), ("refund", 2)];
/// assert!(compare(&older, &newer).is_ok());
/// assert!(compare(&newer, &older).is_err());
/// ```
pub fn compare(wanted: &[(&str, u64)], offered: &[(&str, u64)]) -> Result<(), Mismatch> {
    for (index, (want, offer)) in wanted.iter().zip(offered).enumerate() {
        if want.1 != offer.1 {
            return Err(Mismatch::Field {
                index,
                wanted: want.0.to_owned(),
                offered: offer.0.to_owned(),
            });
        }
    }
    match wanted.get(offered.len()) {
        Some(first) => Err(Mismatch::Newer {
            wanted: wanted.len(),
            offered: offered.len(),
            first: first.0.to_owned(),
        }),
        None => Ok(()),
    }
}

/// The fields of a layout the other side handed over, as `(name, hash)`.
///
/// `None` when a pointer is null with fields, or a name is not UTF-8.
///
/// # Safety
///
/// `layout.fields` points at `layout.count` fields whose names are valid
/// for the call.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Layout, LayoutField, Str};
/// use enshwick_loader_api::services::read_layout;
///
/// let fields = [LayoutField { name: Str::new("price"), hash: 1 }];
/// let layout = Layout { fields: fields.as_ptr(), count: 1, size: 8 };
/// // SAFETY: `fields` lives for the call.
/// let read = unsafe { read_layout(&layout) }.ok_or("bad layout")?;
/// assert_eq!(read, [("price".to_owned(), 1)]);
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub unsafe fn read_layout(layout: &Layout) -> Option<Vec<(String, u64)>> {
    if layout.count == 0 {
        return Some(Vec::new());
    }
    if layout.fields.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees `count` readable fields.
    let fields = unsafe { core::slice::from_raw_parts(layout.fields, layout.count as usize) };
    fields
        .iter()
        // SAFETY: the caller guarantees the names are valid for the call.
        .map(|f| unsafe { f.name.as_str() }.map(|name| (name.to_owned(), f.hash)))
        .collect()
}

/// The ABI view of a declared layout.
pub(crate) fn layout(fields: &'static [LayoutField], size: usize) -> Layout {
    Layout {
        fields: fields.as_ptr(),
        count: u32::try_from(fields.len()).unwrap_or(u32::MAX),
        size: u32::try_from(size).unwrap_or(u32::MAX),
    }
}

/// The name of a declared service or event, for the ABI.
pub(crate) fn name(name: &'static str) -> Str {
    Str::new(name)
}

/// Sends the mod event `E` that this mod defined, from any thread.
///
/// From [`Load::define_event`](crate::Load::define_event). Sending is a
/// direct call of every subscriber's handler on the sending thread, with no
/// queue and no allocation; before every mod is initialised it reaches no
/// one.
///
/// # Examples
///
/// ```
/// #[enshwick_loader_api::event(name = "trading-post.sold")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Sold {
///     pub item: u32,
/// }
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let sold = load.define_event::<Sold>()?;
///     sold.send(&Sold { item: 7 });
///     Ok(())
/// }
/// ```
pub struct Emitter<E> {
    host: Host,
    id: u32,
    _event: PhantomData<fn(&E)>,
}

impl<E> Clone for Emitter<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E> Copy for Emitter<E> {}

impl<E> core::fmt::Debug for Emitter<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Emitter")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<E: ModEvent> Emitter<E> {
    pub(crate) fn new(host: Host, id: u32) -> Self {
        Self {
            host,
            id,
            _event: PhantomData,
        }
    }

    /// Hands `event` to every subscriber, on this thread, before returning.
    ///
    /// # Examples
    ///
    /// See [`Emitter`].
    #[inline]
    pub fn send(&self, event: &E) {
        let _status = (self.host.api().send_event)(
            self.host.handle(),
            self.id,
            core::ptr::from_ref(event).cast(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Mismatch, compare, layout_hash};

    #[crate::event(name = "probe.direct")]
    #[derive(Clone, Copy, Debug)]
    struct Direct {
        best: Option<u32>,
        pair: [u8; 2],
    }

    macro_rules! through {
        ($best:ty, $pair:ty) => {
            #[crate::event(name = "probe.through")]
            #[derive(Clone, Copy, Debug)]
            struct Through {
                best: $best,
                pair: $pair,
            }
        };
    }
    through!(Option<u32>, [u8; 2]);

    #[test]
    fn a_type_passed_through_a_macro_hashes_as_written_out() {
        use super::ModEvent;
        let hashes = |layout: &[crate::abi::LayoutField]| -> Vec<u64> {
            layout.iter().map(|field| field.hash).collect()
        };
        assert_eq!(hashes(Direct::LAYOUT), hashes(Through::LAYOUT));
    }

    #[test]
    fn an_older_user_reads_a_newer_table() {
        let older = [("a", 1), ("b", 2)];
        let newer = [("a", 1), ("b", 2), ("c", 3)];
        assert_eq!(compare(&older, &newer), Ok(()));
    }

    #[test]
    fn a_newer_user_names_the_first_missing_field() {
        let older = [("a", 1)];
        let newer = [("a", 1), ("b", 2), ("c", 3)];
        assert_eq!(
            compare(&newer, &older),
            Err(Mismatch::Newer {
                wanted: 3,
                offered: 1,
                first: "b".to_owned()
            })
        );
    }

    #[test]
    fn a_changed_field_is_named() {
        let here = [("a", 1), ("b", 2)];
        let there = [("a", 1), ("b", 9)];
        assert_eq!(
            compare(&here, &there),
            Err(Mismatch::Field {
                index: 1,
                wanted: "b".to_owned(),
                offered: "b".to_owned()
            })
        );
    }

    #[test]
    fn the_hash_sees_type_size_and_place() {
        let base = layout_hash("a: u32", 4, 4, 0);
        assert_ne!(base, layout_hash("a: u64", 4, 4, 0));
        assert_ne!(base, layout_hash("a: u32", 8, 4, 0));
        assert_ne!(base, layout_hash("a: u32", 4, 8, 0));
        assert_ne!(base, layout_hash("a: u32", 4, 4, 8));
        assert_eq!(base, layout_hash("a: u32", 4, 4, 0));
    }
}
