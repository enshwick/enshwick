//! Typed links in the loader's hook chains.

use core::cell::Cell;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::abi::HookFlags;

mod sealed {
    pub trait Sealed {}
}

/// A function pointer with the `system` ABI that a hook can stand in for.
///
/// Implemented for `extern "system" fn` and `unsafe extern "system" fn` with up
/// to twelve arguments; sealed, since the loader relies on the layout.
///
/// # Safety
///
/// - Implementors are function pointers of the size of `usize`.
///
/// # Examples
///
/// ```
/// fn takes<F: enshwick::FnPtr>() {}
/// takes::<unsafe extern "system" fn(*mut u64) -> u32>();
/// ```
pub unsafe trait FnPtr: Copy + Send + Sync + 'static + sealed::Sealed {
    /// What the function returns, `()` when it returns nothing.
    type Output: 'static;
}

macro_rules! fn_ptr {
    ($($arg:ident),*) => {
        impl<R: 'static, $($arg: 'static),*> sealed::Sealed for extern "system" fn($($arg),*) -> R {}
        // SAFETY: a function pointer has the size of `usize` on every target
        // the game runs on.
        unsafe impl<R: 'static, $($arg: 'static),*> FnPtr for extern "system" fn($($arg),*) -> R {
            type Output = R;
        }
        impl<R: 'static, $($arg: 'static),*> sealed::Sealed
            for unsafe extern "system" fn($($arg),*) -> R {}
        // SAFETY: an unsafe function pointer has the size of `usize` on every
        // target the game runs on.
        unsafe impl<R: 'static, $($arg: 'static),*> FnPtr
            for unsafe extern "system" fn($($arg),*) -> R {
            type Output = R;
        }
    };
}

fn_ptr!();
fn_ptr!(A);
fn_ptr!(A, B);
fn_ptr!(A, B, C);
fn_ptr!(A, B, C, D);
fn_ptr!(A, B, C, D, E);
fn_ptr!(A, B, C, D, E, G);
fn_ptr!(A, B, C, D, E, G, H);
fn_ptr!(A, B, C, D, E, G, H, I);
fn_ptr!(A, B, C, D, E, G, H, I, J);
fn_ptr!(A, B, C, D, E, G, H, I, J, K);
fn_ptr!(A, B, C, D, E, G, H, I, J, K, L);
fn_ptr!(A, B, C, D, E, G, H, I, J, K, L, M);

const _: () = assert!(size_of::<extern "system" fn()>() == size_of::<usize>());

/// A mod's link for one game function: its detour and the slot through which
/// it reaches the rest of the chain.
///
/// Declared with [`detour!`](crate::detour) or [`#[hook]`](macro@crate::hook),
/// registered with
/// [`Load::hook`](crate::Load::hook).
///
/// # Examples
///
/// ```
/// enshwick::detour! {
///     /// Counts upgrades.
///     static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade;
/// }
///
/// fn upgrade(next: enshwick::Next<'_, unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
///     // SAFETY: the original takes the same context.
///     unsafe { next.call(ctx) }
/// }
///
/// assert_eq!(UPGRADE.name(), "UPGRADE");
/// ```
#[derive(Debug)]
pub struct Hook<F: FnPtr> {
    name: &'static str,
    detour: F,
    next: AtomicUsize,
    priority: i32,
    flags: HookFlags,
}

impl<F: FnPtr> Hook<F> {
    /// A link whose entry point is `detour`; used by [`detour!`](crate::detour).
    ///
    /// # Examples
    ///
    /// See [`Hook`].
    #[must_use]
    pub const fn new(name: &'static str, detour: F, priority: i32, flags: HookFlags) -> Self {
        Self {
            name,
            detour,
            next: AtomicUsize::new(0),
            priority,
            flags,
        }
    }

    /// The name the link was declared with, used in log lines.
    ///
    /// # Examples
    ///
    /// See [`Hook`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The entry point the loader links into the chain: the detour with its
    /// panic guard, as the game calls it.
    ///
    /// # Examples
    ///
    /// ```
    /// enshwick::detour! {
    ///     static TICK: extern "system" fn() = tick;
    /// }
    ///
    /// fn tick(next: enshwick::Next<'_, extern "system" fn()>) {
    ///     next.call();
    /// }
    ///
    /// let entry: extern "system" fn() = TICK.entry();
    /// # let _ = entry;
    /// ```
    #[must_use]
    pub const fn entry(&self) -> F {
        self.detour
    }

    /// The rest of the chain for one call of the entry, which keeps what
    /// `next` returned in `frame`; used by [`detour!`](crate::detour).
    #[doc(hidden)]
    #[must_use]
    pub fn next<'frame>(&'static self, frame: &'frame Frame<F::Output>) -> Next<'frame, F> {
        Next {
            slot: &self.next,
            frame,
            _f: core::marker::PhantomData,
        }
    }

    pub(crate) fn spec(&'static self, target: u64) -> crate::abi::HookSpec {
        crate::abi::HookSpec {
            target,
            detour: address(self.detour),
            next: &raw const self.next,
            priority: self.priority,
            flags: self.flags,
        }
    }
}

fn address<F: FnPtr>(f: F) -> usize {
    // SAFETY: `FnPtr` implementors are function pointers the size of `usize`.
    unsafe { core::mem::transmute_copy(&f) }
}

/// What a detour calls to continue: the next link, or the game's original
/// function.
///
/// [`call`](Self::call) keeps what the chain returned for the one call of the
/// detour this belongs to: when the body panics after calling it, the detour
/// returns that value instead of running the game's function again. One
/// atomic load, one store on the stack and an indirect call; copy it freely.
///
/// # Examples
///
/// See [`Hook`].
#[derive(Debug, Clone, Copy)]
pub struct Next<'frame, F: FnPtr> {
    slot: &'static AtomicUsize,
    frame: &'frame Frame<F::Output>,
    _f: core::marker::PhantomData<F>,
}

impl<F: FnPtr> Next<'_, F> {
    #[inline]
    fn function(self) -> F {
        let address = self.slot.load(Ordering::Acquire);
        assert!(
            address != 0,
            "Next::call on a hook the loader has not linked"
        );
        // SAFETY: the loader stores a function of type `F` in the slot before
        // the detour can run, and the slot is not empty; `F` is a function
        // pointer the size of `usize`.
        unsafe { core::mem::transmute_copy(&address) }
    }
}

macro_rules! next_call {
    ($(#[$attr:meta])* $($arg:ident: $ty:ident),*) => {
        impl<R: Copy + 'static, $($ty: 'static),*> Next<'_, extern "system" fn($($ty),*) -> R> {
            /// Runs the rest of the chain and keeps what it returned for this
            /// call of the detour.
            ///
            /// # Panics
            ///
            /// When the loader has not linked the hook, as when a unit test
            /// calls the detour's body itself.
            ///
            /// # Examples
            ///
            /// See [`Hook`].
            #[inline]
            $(#[$attr])*
            pub fn call(self, $($arg: $ty),*) -> R {
                let returned = self.function()($($arg),*);
                self.frame.returned.set(Some(returned));
                returned
            }
        }

        impl<R: Copy + 'static, $($ty: 'static),*> Next<'_, unsafe extern "system" fn($($ty),*) -> R> {
            /// Runs the rest of the chain and keeps what it returned for this
            /// call of the detour.
            ///
            /// # Safety
            ///
            /// - The arguments meet the contract of the game function the
            ///   hook stands in for, as the arguments of the game's own call do.
            ///
            /// # Panics
            ///
            /// When the loader has not linked the hook, as when a unit test
            /// calls the detour's body itself.
            ///
            /// # Examples
            ///
            /// See [`Hook`].
            #[inline]
            $(#[$attr])*
            pub unsafe fn call(self, $($arg: $ty),*) -> R {
                // SAFETY: the caller passes arguments that meet the target's
                // contract; the rest of the chain is a function of that type.
                let returned = unsafe { self.function()($($arg),*) };
                self.frame.returned.set(Some(returned));
                returned
            }
        }
    };
}

next_call!();
next_call!(a: A);
next_call!(a: A, b: B);
next_call!(a: A, b: B, c: C);
next_call!(a: A, b: B, c: C, d: D);
next_call!(a: A, b: B, c: C, d: D, e: E);
next_call!(a: A, b: B, c: C, d: D, e: E, g: G);
next_call!(#[expect(clippy::too_many_arguments, reason = "the game function's own arguments")] a: A, b: B, c: C, d: D, e: E, g: G, h: H);
next_call!(#[expect(clippy::too_many_arguments, reason = "the game function's own arguments")] a: A, b: B, c: C, d: D, e: E, g: G, h: H, i: I);
next_call!(#[expect(clippy::too_many_arguments, reason = "the game function's own arguments")] a: A, b: B, c: C, d: D, e: E, g: G, h: H, i: I, j: J);
next_call!(#[expect(clippy::too_many_arguments, reason = "the game function's own arguments")] a: A, b: B, c: C, d: D, e: E, g: G, h: H, i: I, j: J, k: K);
next_call!(#[expect(clippy::too_many_arguments, reason = "the game function's own arguments")] a: A, b: B, c: C, d: D, e: E, g: G, h: H, i: I, j: J, k: K, l: L);
next_call!(#[expect(clippy::too_many_arguments, reason = "the game function's own arguments")] a: A, b: B, c: C, d: D, e: E, g: G, h: H, i: I, j: J, k: K, l: L, m: M);

/// What one call of a detour's entry keeps on its stack: the value the rest
/// of the chain returned, once the body called it.
#[doc(hidden)]
pub struct Frame<R> {
    returned: Cell<Option<R>>,
}

impl<R> Frame<R> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            returned: Cell::new(None),
        }
    }

    /// What the rest of the chain returned, `None` when the body never
    /// called it.
    pub fn take(&self) -> Option<R> {
        self.returned.take()
    }
}

impl<R> Default for Frame<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> core::fmt::Debug for Frame<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frame").finish_non_exhaustive()
    }
}

/// Declares a hook: a static [`Hook`] and the entry point the loader links into
/// the chain.
///
/// The body is an ordinary function taking a [`Next`] and the target's
/// arguments. A panic in it is caught: the mod reports a fault, the loader
/// unlinks the mod, and the game runs vanilla instead of crashing. The game's
/// function runs once at most: when the body panics before
/// [`Next::call`], this call falls through to the rest of the chain; when it
/// panics after, it returns what the chain gave.
///
/// Optional settings follow the function: `priority = <i32>` (lower runs
/// earlier) and `flags = <HookFlags>`. [`#[hook]`](macro@crate::hook) writes the
/// same declaration from the body's signature.
///
/// # Examples
///
/// ```
/// enshwick::detour! {
///     /// The lookup every survivor caller goes through.
///     pub static LOOKUP: unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32 =
///         lookup, priority = -10, flags = enshwick::abi::HookFlags::CALLER_FRAME;
/// }
///
/// fn lookup(
///     next: enshwick::Next<'_, unsafe extern "system" fn(*mut u8, *const u32) -> u32>,
///     bases: *mut u8,
///     npc: *const u32,
/// ) -> u32 {
///     // SAFETY: `bases` and `npc` are the arguments the game called the
///     // lookup with on this thread, passed on unchanged, so the rest of the
///     // chain gets what the game's own call gave it.
///     unsafe { next.call(bases, npc) }
/// }
/// # let _ = &LOOKUP;
/// ```
#[macro_export]
macro_rules! detour {
    (
        $(#[$meta:meta])*
        $vis:vis static $name:ident : unsafe extern "system" fn($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)?
            = $body:path $(, priority = $priority:expr)? $(, flags = $flags:expr)? $(,)?;
    ) => {
        $crate::detour!(@emit [$(#[$meta])*] $vis $name [unsafe] ($($arg : $ty),*) ($($ret)?) $body
            [$($priority)?] [$($flags)?]);
    };
    (
        $(#[$meta:meta])*
        $vis:vis static $name:ident : extern "system" fn($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)?
            = $body:path $(, priority = $priority:expr)? $(, flags = $flags:expr)? $(,)?;
    ) => {
        $crate::detour!(@emit [$(#[$meta])*] $vis $name [] ($($arg : $ty),*) ($($ret)?) $body
            [$($priority)?] [$($flags)?]);
    };
    (@emit [$($meta:tt)*] $vis:vis $name:ident [$($unsafe:tt)?] ($($arg:ident : $ty:ty),*) ($($ret:ty)?) $body:path
        [$($priority:expr)?] [$($flags:expr)?]) => {
        $($meta)*
        $vis static $name: $crate::Hook<$($unsafe)? extern "system" fn($($ty),*) $(-> $ret)?> = {
            // The arguments go to the body and, after a panic, to `next` again.
            const _: () = {
                $($crate::__private::hook_argument::<$ty>();)*
            };
            const BODY: fn($crate::Next<'_, $($unsafe)? extern "system" fn($($ty),*) $(-> $ret)?>, $($ty),*) $(-> $ret)? = $body;
            $($unsafe)? extern "system" fn __enshwick_entry($($arg: $ty),*) $(-> $ret)? {
                let frame = $crate::__private::Frame::new();
                let next = $name.next(&frame);
                match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| BODY(next, $($arg),*))) {
                    ::core::result::Result::Ok(value) => value,
                    ::core::result::Result::Err(_) => {
                        $crate::__private::fault_in(::core::stringify!($name));
                        match frame.take() {
                            ::core::option::Option::Some(returned) => returned,
                            ::core::option::Option::None => {
                                $crate::detour!(@call [$($unsafe)?] next ($($arg),*))
                            }
                        }
                    }
                }
            }
            $crate::Hook::new(
                ::core::stringify!($name),
                __enshwick_entry,
                $crate::__private::or_default(&[$($priority)?], 0),
                $crate::__private::or_default(&[$($flags)?], $crate::abi::HookFlags::NONE),
            )
        };
    };
    (@call [unsafe] $next:ident ($($arg:ident),*)) => {
        // SAFETY: the rest of the chain takes the arguments the game passed.
        unsafe { $next.call($($arg),*) }
    };
    (@call [] $next:ident ($($arg:ident),*)) => {
        $next.call($($arg),*)
    };
}

#[cfg(test)]
mod tests {
    use super::{Frame, Hook};
    use crate::abi::HookFlags;

    extern "system" fn original() {}

    static UNLINKED: Hook<extern "system" fn()> =
        Hook::new("UNLINKED", original, 0, HookFlags::NONE);

    #[test]
    #[should_panic(expected = "not linked")]
    fn an_unlinked_next_panics_instead_of_calling_null() {
        let frame = Frame::new();
        UNLINKED.next(&frame).call();
    }
}
