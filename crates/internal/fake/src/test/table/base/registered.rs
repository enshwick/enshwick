//! A mod's callback as the fake keeps it, so it runs as the mod that
//! registered it.

use enshwick_core::abi::ModHandle;

use crate::test::table::TABLE;

/// A callback the mod registered with its pointer and the handle that
/// registered it, so the fake calls it as that mod or data pack, and only
/// through [`run`](Self::run).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Registered<F> {
    callback: F,
    user: usize,
    /// `None` for a test's stand-in for another mod, which runs as the
    /// loader, as another DLL's callback runs outside the pack's scope.
    owner: Option<ModHandle>,
}

impl<F: Copy> Registered<F> {
    pub(crate) fn new(callback: F, user: *mut core::ffi::c_void, owner: ModHandle) -> Self {
        Self {
            callback,
            user: user.expose_provenance(),
            owner: Some(owner),
        }
    }

    /// A test's stand-in for another mod's callback.
    pub(crate) fn unowned(callback: F, user: *mut core::ffi::c_void) -> Self {
        Self {
            callback,
            user: user.expose_provenance(),
            owner: None,
        }
    }

    /// The handle that registered the callback; `None` for a stand-in.
    pub(crate) const fn owner(&self) -> Option<ModHandle> {
        self.owner
    }

    /// Runs `call` with the callback and its pointer as the owner, as the
    /// loader runs a callback in its owner's profile scope.
    pub(crate) fn run<R>(&self, call: impl FnOnce(F, *mut core::ffi::c_void) -> R) -> R {
        let (callback, user) = (
            self.callback,
            core::ptr::with_exposed_provenance_mut(self.user),
        );
        match self.owner {
            // SAFETY: `TABLE` is the fake's table, a static.
            Some(owner) => unsafe {
                enshwick_core::__fake::as_pack(&raw const TABLE, owner, || call(callback, user))
            },
            None => enshwick_core::__fake::as_loader(|| call(callback, user)),
        }
    }
}
