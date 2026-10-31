//! Services and mod events: their specs and layouts.

use core::ffi::c_void;

use crate::abi::{Header, Str};

/// One field of a service table or mod event, as its layout hash sees it.
///
/// Frozen: written as an array.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{LayoutField, Str};
///
/// let field = LayoutField { name: Str::new("price"), hash: 7 };
/// assert_eq!(field.hash, 7);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LayoutField {
    /// The field's name.
    pub name: Str,
    /// The hash of the field's name, the shape of its type, its size,
    /// alignment and offset.
    pub hash: u64,
}

/// The layout of a service table or mod event, field by field.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Layout;
///
/// let layout = Layout { fields: core::ptr::null(), count: 0, size: 0 };
/// assert_eq!(layout.count, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct Layout {
    /// The fields, in declaration order.
    pub fields: *const LayoutField,
    /// How many `fields` points at.
    pub count: u32,
    /// The size of the whole type.
    pub size: u32,
}

/// A service a mod publishes, or the one it asks for.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Layout, ServiceSpec, Str};
///
/// let spec = ServiceSpec {
///     header: Header::of::<ServiceSpec>(),
///     name: Str::new("you.trading-post:prices"),
///     major: 1,
///     layout: Layout { fields: core::ptr::null(), count: 0, size: 0 },
///     table: core::ptr::null(),
/// };
/// assert_eq!(spec.major, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ServiceSpec {
    /// Size and version of this struct.
    pub header: Header,
    /// The service's name, `<publisher id>:<what>`.
    pub name: Str,
    /// The incompatible version: tables of two majors share nothing.
    pub major: u32,
    /// The table's fields; tables grow by appending.
    pub layout: Layout,
    /// The `'static` table; null when asking.
    pub table: *const c_void,
}

impl_headed!(ServiceSpec);

/// What [`HostApi::service`](super::HostApi::service) found.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ServiceFound;
///
/// let found = ServiceFound::EMPTY;
/// assert!(found.table.is_null());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ServiceFound {
    /// Size and version of this struct.
    pub header: Header,
    /// The publisher's table, valid for the process.
    pub table: *const c_void,
    /// How many fields the publisher's table has.
    pub fields: u32,
    /// The publisher's mod id, valid for the process.
    pub owner: Str,
    /// The publisher's version, valid for the process.
    pub owner_version: Str,
    /// Why nothing was found, valid for the process; empty on success.
    pub reason: Str,
}

impl_headed!(ServiceFound);

impl ServiceFound {
    /// Nothing found yet, for the loader to fill.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        table: core::ptr::null(),
        fields: 0,
        owner: Str::EMPTY,
        owner_version: Str::EMPTY,
        reason: Str::EMPTY,
    };
}

/// A mod event a mod defines, or the one it subscribes to.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{EventSpec, Header, Layout, Str};
///
/// let spec = EventSpec {
///     header: Header::of::<EventSpec>(),
///     name: Str::new("you.trading-post:sold"),
///     layout: Layout { fields: core::ptr::null(), count: 0, size: 0 },
/// };
/// assert_eq!(spec.layout.size, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct EventSpec {
    /// Size and version of this struct.
    pub header: Header,
    /// The event's name, `<defining mod id>:<what>`.
    pub name: Str,
    /// The event data's fields; events grow by appending.
    pub layout: Layout,
}

impl_headed!(EventSpec);

/// A mod's handler for a mod event: the event's data, valid for the call,
/// and the pointer given at subscription.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_mod_event`](crate::Load::on_mod_event).
pub type ModEventFn = unsafe extern "system" fn(data: *const c_void, user: *mut c_void);

/// A described service, a call: one function from an input struct to an
/// answer, which one mod serves and others call, or the call a mod asks for.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{CallSpec, Header, Layout, Str};
///
/// let none = Layout { fields: core::ptr::null(), count: 0, size: 0 };
/// let spec = CallSpec {
///     header: Header::of::<CallSpec>(),
///     name: Str::new("you.trading-post:price-of"),
///     major: 1,
///     input: none,
///     output: none,
/// };
/// assert_eq!(spec.major, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct CallSpec {
    /// Size and version of this struct.
    pub header: Header,
    /// The call's name, `<serving mod id>:<what>`.
    pub name: Str,
    /// The incompatible version: calls of two majors share nothing.
    pub major: u32,
    /// The input's fields and size.
    pub input: Layout,
    /// The answer, one field, and its size.
    pub output: Layout,
}

impl_headed!(CallSpec);

/// A mod's handler for a call it serves.
///
/// It gets the input, valid for the call, the answer to write, as large as
/// the spec's output, and the pointer given at serving, and answers
/// [`Status::OK`](super::Status::OK) only once the whole answer is written.
/// It runs on the caller's thread, any thread, possibly on several at once
/// and inside another call, and must not unwind.
///
/// # Examples
///
/// The safe layer wraps it in `Load::serve_call`, behind the `unstable` feature.
pub type CallFn = unsafe extern "system" fn(
    input: *const c_void,
    output: *mut c_void,
    user: *mut c_void,
) -> super::Status;
