//! How the engine paces its loops and sizes its task groups, as the loader
//! hands them to mods.

use core::ffi::c_void;

use super::Header;

/// A wait of the engine's event loop on its handles, about to happen, as
/// [`HostApi::loop_wait`](super::HostApi::loop_wait) hands it on.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, LoopWaitData};
///
/// let wait = LoopWaitData { header: Header::of::<LoopWaitData>(), ms: 0, reserved: 0, left_ns: 400_000 };
/// assert!(wait.ms == 0 && wait.left_ns > 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct LoopWaitData {
    /// Size and version of this struct.
    pub header: Header,
    /// What the game is about to wait, in milliseconds: the time left,
    /// truncated.
    pub ms: u32,
    /// Zero.
    pub reserved: u32,
    /// The time left before the loop's deadline, in nanoseconds.
    pub left_ns: i64,
}

impl_headed!(LoopWaitData);

/// A handler of the event loop's waits: the wait, borrowed for the call,
/// and the pointer given at registration. It must not unwind.
///
/// # Examples
///
/// See [`HostApi::loop_wait`](super::HostApi::loop_wait), which takes it.
pub type LoopWaitFn = unsafe extern "system" fn(wait: *const LoopWaitData, user: *mut c_void);

/// A handler of the server loop's wait between two passes: the wait in
/// nanoseconds, which it may change, and the pointer given at registration.
/// It must not unwind.
///
/// # Examples
///
/// See [`HostApi::server_wait`](super::HostApi::server_wait), which takes it.
pub type ServerWaitFn = unsafe extern "system" fn(wait_ns: *mut u64, user: *mut c_void);

/// Which waits [`HostApi::loop_wait`](super::HostApi::loop_wait) found and
/// hooked, as bits.
///
/// # Examples
///
/// ```
/// use enshwick::abi::LoopWaitSites;
///
/// let found = LoopWaitSites(LoopWaitSites::HANDLES.0 | LoopWaitSites::MESSAGES.0);
/// assert!(found.contains(LoopWaitSites::MESSAGES));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LoopWaitSites(pub u32);

impl LoopWaitSites {
    /// None.
    pub const NONE: Self = Self(0);
    /// The wait on handles, on the server and the client.
    pub const HANDLES: Self = Self(1);
    /// The client's wait on handles and window messages.
    pub const MESSAGES: Self = Self(2);

    /// Whether every bit of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`LoopWaitSites`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}
