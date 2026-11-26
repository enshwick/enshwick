//! Extension points: their specs, links and verdicts.

use core::ffi::c_void;
use core::sync::atomic::AtomicU32;

use crate::abi::{Header, Layout, Str};

/// How the links of an extension point act on its answer, as the defining
/// mod chooses.
///
/// # Examples
///
/// ```
/// use enshwick::abi::PointMode;
///
/// assert_ne!(PointMode::OBSERVE, PointMode::OVERRIDE);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct PointMode(pub u32);

impl PointMode {
    /// Links see the answer and change nothing.
    pub const OBSERVE: Self = Self(0);
    /// Links may change the answer; the defining mod may ignore the change.
    pub const ADVISE: Self = Self(1);
    /// Links may change the answer, and the defining mod takes it.
    pub const OVERRIDE: Self = Self(2);
}

/// Flags of a [`PointSpec`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::PointFlags;
///
/// assert!(PointFlags::REFUSE.contains(PointFlags::REFUSE));
/// assert!(!PointFlags::NONE.contains(PointFlags::REFUSE));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct PointFlags(pub u32);

impl PointFlags {
    /// No flag.
    pub const NONE: Self = Self(0);
    /// A link may refuse: the chain stops and the answer is a refusal.
    pub const REFUSE: Self = Self(1);

    /// Whether every flag of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`PointFlags`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// An extension point a mod defines, or the one a link attaches to.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Layout, PointFlags, PointMode, PointSpec, Str};
///
/// let empty = Layout { fields: core::ptr::null(), count: 0, size: 0 };
/// let spec = enshwick::abi::headed!(PointSpec {
///     name: Str::new("you.trading-post:price"),
///     mode: PointMode::OVERRIDE,
///     flags: PointFlags::REFUSE,
///     input: empty,
///     output: empty,
/// });
/// assert_eq!(spec.mode, PointMode::OVERRIDE);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct PointSpec {
    /// Size and version of this struct.
    pub header: Header,
    /// The point's name, `<defining mod id>:<point>`.
    pub name: Str,
    /// How links act on the answer.
    pub mode: PointMode,
    /// Point flags.
    pub flags: PointFlags,
    /// The fields of the input links read; inputs grow by appending.
    pub input: Layout,
    /// The answer, one field: its type, size and alignment.
    pub output: Layout,
}

impl_headed!(PointSpec);

/// A link a mod attaches to another mod's extension point, with its place
/// in the chain.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PointLink};
///
/// let link = enshwick::abi::headed!(PointLink {
///     point: core::ptr::null(),
///     before: core::ptr::null(),
///     after: core::ptr::null(),
///     before_count: 0,
///     after_count: 0,
///     priority: 0,
/// });
/// assert_eq!(link.priority, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct PointLink {
    /// Size and version of this struct.
    pub header: Header,
    /// The point, as the attaching mod declares it.
    pub point: *const PointSpec,
    /// Ids of mods whose links run after this one.
    pub before: *const Str,
    /// Ids of mods whose links run before this one.
    pub after: *const Str,
    /// How many ids `before` points at.
    pub before_count: u32,
    /// How many ids `after` points at.
    pub after_count: u32,
    /// Among links the ids leave free, lower runs earlier.
    pub priority: i32,
}

impl_headed!(PointLink);

/// What [`HostApi::define_point`](super::HostApi::define_point) hands back.
///
/// # Examples
///
/// ```
/// use enshwick::abi::PointHandle;
///
/// assert!(PointHandle::EMPTY.live.is_null());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct PointHandle {
    /// Size and version of this struct.
    pub header: Header,
    /// The id [`HostApi::run_point`](super::HostApi::run_point) takes.
    pub id: u32,
    /// How many links run, valid for the process: 0 until every mod is
    /// initialised and whenever none is live, so the defining mod skips
    /// [`HostApi::run_point`](super::HostApi::run_point).
    pub live: *const AtomicU32,
}

impl_headed!(PointHandle);

impl PointHandle {
    /// Nothing defined yet, for the loader to fill.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        id: 0,
        live: core::ptr::null(),
    };
}

/// A link's answer, and the chain's.
///
/// # Examples
///
/// ```
/// use enshwick::abi::PointVerdict;
///
/// assert_ne!(PointVerdict::PASS, PointVerdict::SET);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct PointVerdict(pub u32);

impl PointVerdict {
    /// The answer stays as it was.
    pub const PASS: Self = Self(0);
    /// The link wrote a new answer over the value.
    pub const SET: Self = Self(1);
    /// The link refused: the chain stops.
    pub const REFUSE: Self = Self(2);
    /// The link could not answer, as when its handler panicked: the loader
    /// turns this link off, leaves the answer as it was and asks the next.
    pub const FAILED: Self = Self(3);
}

/// A mod's link on an extension point: the input, valid for the call, the
/// answer so far, which the link may overwrite and then answers
/// [`PointVerdict::SET`], and the pointer given at attaching.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread that asks the point.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::extend_point`](../struct.Load.html#method.extend_point) and
/// [`Load::observe_point`](../struct.Load.html#method.observe_point).
pub type PointFn = unsafe extern "system" fn(
    input: *const c_void,
    value: *mut c_void,
    user: *mut c_void,
) -> PointVerdict;
