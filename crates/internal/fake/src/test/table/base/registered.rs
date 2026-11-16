//! A mod's callback as the fake keeps it, so it runs as the mod that
//! registered it.

use core::ffi::c_void;

use enshwick_core::abi::{
    CallFn, ConfigCheckFn, ModHandle, PointVerdict, ResourcePatchFn, Status, Str,
};

use crate::test::table::{TABLE, with};

/// A callback type with a stand-in that answers as the loader does where no
/// callback is there, which [`Registered::run`] calls in place of a mod
/// that is off.
pub(crate) trait Callback: Copy {
    /// The stand-in: it answers as no callback and touches nothing it is
    /// passed.
    const SILENT: Self;
}

/// An answer that means "no opinion": a filter passes, a handler does
/// nothing, a point's link passes.
pub(crate) trait Silent {
    /// That answer.
    const SILENT: Self;
}

impl Silent for () {
    const SILENT: Self = ();
}

impl Silent for u8 {
    const SILENT: Self = enshwick_core::abi::FILTER_PASS;
}

impl Silent for u32 {
    const SILENT: Self = 0;
}

impl Silent for i32 {
    const SILENT: Self = 0;
}

impl Silent for PointVerdict {
    const SILENT: Self = Self::PASS;
}

/// The stand-in for every callback type of one arity whose answer is
/// [`Silent`].
macro_rules! silent_of_arity {
    ($stub:ident: $($arg:ident),+) => {
        unsafe extern "system" fn $stub<$($arg,)+ R: Silent>($(_: $arg),+) -> R {
            R::SILENT
        }

        impl<$($arg,)+ R: Silent> Callback for unsafe extern "system" fn($($arg),+) -> R {
            const SILENT: Self = $stub::<$($arg,)+ R>;
        }
    };
}

silent_of_arity!(silent1: A);
silent_of_arity!(silent2: A, B);
silent_of_arity!(silent3: A, B, C);
silent_of_arity!(silent4: A, B, C, D);
silent_of_arity!(silent5: A, B, C, D, E);
silent_of_arity!(silent6: A, B, C, D, E, G);

/// An off mod's settings check: none runs, so the text stands.
unsafe extern "system" fn no_check(_text: Str, _why: *mut Str, _user: *mut c_void) -> Status {
    Status::OK
}

impl Callback for ConfigCheckFn {
    const SILENT: Self = no_check;
}

/// An off mod's served call: it answers that the server is off.
unsafe extern "system" fn no_server(
    _input: *const c_void,
    _output: *mut c_void,
    _user: *mut c_void,
) -> Status {
    Status::FAULTED
}

impl Callback for CallFn {
    const SILENT: Self = no_server;
}

/// An off mod's resource patch: it builds nothing.
unsafe extern "system" fn no_patch(
    _base: *const u8,
    _base_len: u32,
    _user: *mut c_void,
    _out: *mut *mut u8,
    _out_len: *mut u32,
) -> Status {
    Status::REFUSED
}

impl Callback for ResourcePatchFn {
    const SILENT: Self = no_patch;
}

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
    pub(crate) fn new(callback: F, user: *mut c_void, owner: ModHandle) -> Self {
        Self {
            callback,
            user: user.expose_provenance(),
            owner: Some(owner),
        }
    }

    /// A test's stand-in for another mod's callback.
    pub(crate) fn unowned(callback: F, user: *mut c_void) -> Self {
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
    /// loader runs a callback in its owner's profile scope; with the
    /// [`Callback::SILENT`] stand-in when the owner is off, as the loader
    /// calls no callback of a mod that is off.
    pub(crate) fn run<R>(&self, call: impl FnOnce(F, *mut c_void) -> R) -> R
    where
        F: Callback,
    {
        let callback = if self.owner.is_some_and(is_off) {
            F::SILENT
        } else {
            self.callback
        };
        let user = core::ptr::with_exposed_provenance_mut(self.user);
        match self.owner {
            // SAFETY: `TABLE` is the fake's table, a static.
            Some(owner) => unsafe {
                enshwick_core::__fake::as_pack(&raw const TABLE, owner, || call(callback, user))
            },
            None => enshwick_core::__fake::as_loader(|| call(callback, user)),
        }
    }
}

/// Whether the fake turned the mod or data pack `handle` off.
pub(crate) fn is_off(handle: ModHandle) -> bool {
    with(false, |s| s.off.contains(&handle))
}

/// Turns the mod or data pack `handle` off, as the loader does after a
/// failed init or ready or a fault: none of its callbacks runs again.
pub(crate) fn turn_off(handle: ModHandle) {
    with((), |s| {
        if !s.off.contains(&handle) {
            s.off.push(handle);
        }
    });
}
