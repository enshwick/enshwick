//! Typed links in the loader's hook chains.

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
/// Implementors are function pointers of the size of `usize`.
///
/// # Examples
///
/// ```
/// fn takes<F: enshwick::FnPtr>() {}
/// takes::<unsafe extern "system" fn(*mut u64) -> u32>();
/// ```
pub unsafe trait FnPtr: Copy + Send + Sync + 'static + sealed::Sealed {}

macro_rules! fn_ptr {
    ($($arg:ident),*) => {
        impl<R: 'static, $($arg: 'static),*> sealed::Sealed for extern "system" fn($($arg),*) -> R {}
        // SAFETY: a function pointer has the size of `usize` on every target the
        // game runs on.
        unsafe impl<R: 'static, $($arg: 'static),*> FnPtr for extern "system" fn($($arg),*) -> R {}
        impl<R: 'static, $($arg: 'static),*> sealed::Sealed
            for unsafe extern "system" fn($($arg),*) -> R {}
        // SAFETY: as above.
        unsafe impl<R: 'static, $($arg: 'static),*> FnPtr
            for unsafe extern "system" fn($($arg),*) -> R {}
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
/// fn upgrade(next: enshwick::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
///     // SAFETY: the original takes the same context.
///     unsafe { next.get()(ctx) }
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
    /// fn tick(next: enshwick::Next<extern "system" fn()>) {
    ///     next.get()();
    /// }
    ///
    /// let entry: extern "system" fn() = TICK.entry();
    /// # let _ = entry;
    /// ```
    #[must_use]
    pub const fn entry(&self) -> F {
        self.detour
    }

    /// The rest of the chain: the next link or the original function.
    ///
    /// # Examples
    ///
    /// See [`Hook`].
    #[must_use]
    pub fn next(&'static self) -> Next<F> {
        Next {
            slot: &self.next,
            _f: core::marker::PhantomData,
        }
    }

    /// Runs `work` that a detour does after calling `next`, so that a panic
    /// in it turns the mod off without running the rest of the chain again.
    ///
    /// A panic in the body falls through to the rest of the chain, which is
    /// right before `next` and wrong after it: the game's function would run
    /// twice. Work after `next` belongs in here. A panic in `work` reports a
    /// fault, as a panic in the body does, and gives `None`; the detour then
    /// returns what `next` gave. The guard costs nothing until a panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::{AtomicU32, Ordering};
    ///
    /// static RAISED: AtomicU32 = AtomicU32::new(0);
    ///
    /// enshwick::detour! {
    ///     static RAISE: extern "system" fn(level: u32) -> u32 = raise;
    /// }
    ///
    /// fn raise(next: enshwick::Next<extern "system" fn(u32) -> u32>, level: u32) -> u32 {
    ///     let raised = next.get()(level);
    ///     let _counted = RAISE.after_next(|| RAISED.fetch_add(raised - level, Ordering::Relaxed));
    ///     raised
    /// }
    /// # let _ = &RAISE;
    /// ```
    pub fn after_next<R>(&self, work: impl FnOnce() -> R) -> Option<R> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
            .inspect_err(|_| fault_after_next(self.name))
            .ok()
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

#[cold]
#[inline(never)]
fn fault_after_next(hook: &str) {
    if let Some(host) = crate::host() {
        host.fault(&format!("panic in hook {hook} after next"));
    }
}

fn address<F: FnPtr>(f: F) -> usize {
    // SAFETY: `FnPtr` implementors are function pointers the size of `usize`.
    unsafe { core::mem::transmute_copy(&f) }
}

/// What a detour calls to continue: the next link, or the game's original
/// function.
///
/// One atomic load and an indirect call; copy it freely. Work a detour does
/// after calling it goes in [`Hook::after_next`].
///
/// # Examples
///
/// See [`Hook`].
#[derive(Debug, Clone, Copy)]
pub struct Next<F: FnPtr> {
    slot: &'static AtomicUsize,
    _f: core::marker::PhantomData<F>,
}

impl<F: FnPtr> Next<F> {
    /// The function to call to continue the chain.
    ///
    /// # Panics
    ///
    /// When the loader has not linked the hook, as when a unit test calls the
    /// detour's body itself.
    ///
    /// # Examples
    ///
    /// See [`Hook`].
    #[inline]
    #[must_use]
    pub fn get(self) -> F {
        let address = self.slot.load(Ordering::Acquire);
        assert!(
            address != 0,
            "Next::get on a hook the loader has not linked"
        );
        // SAFETY: the loader stores a function of type `F` in the slot before
        // the detour can run, and the slot is not empty; `F` is a function
        // pointer the size of `usize`.
        unsafe { core::mem::transmute_copy(&address) }
    }
}

/// Declares a hook: a static [`Hook`] and the entry point the loader links into
/// the chain.
///
/// The body is an ordinary function taking a [`Next`] and the target's
/// arguments. A panic in it is caught: the mod reports a fault, the loader
/// unlinks the mod, and this call falls through to the rest of the chain, so
/// the game runs vanilla instead of crashing. The fall-through cannot tell
/// whether the body already called `next`, and calls the rest of the chain
/// again if it did: the game's function runs twice. Work after `next` goes
/// in [`Hook::after_next`], whose panic turns the mod off without that.
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
///     next: enshwick::Next<unsafe extern "system" fn(*mut u8, *const u32) -> u32>,
///     bases: *mut u8,
///     npc: *const u32,
/// ) -> u32 {
///     // SAFETY: forwarded unchanged to the original.
///     unsafe { next.get()(bases, npc) }
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
            const BODY: fn($crate::Next<$($unsafe)? extern "system" fn($($ty),*) $(-> $ret)?>, $($ty),*) $(-> $ret)? = $body;
            $($unsafe)? extern "system" fn __enshwick_entry($($arg: $ty),*) $(-> $ret)? {
                let next = $name.next();
                match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| BODY(next, $($arg),*))) {
                    ::core::result::Result::Ok(value) => value,
                    // Whether the body called `next` is not known here: tracking it
                    // would cost every call a thread-local write, and the value it
                    // returned is gone anyway.
                    ::core::result::Result::Err(_) => {
                        $crate::__private::fault_in(::core::stringify!($name));
                        $crate::detour!(@call [$($unsafe)?] next ($($arg),*))
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
        unsafe { $next.get()($($arg),*) }
    };
    (@call [] $next:ident ($($arg:ident),*)) => {
        $next.get()($($arg),*)
    };
}

#[cfg(test)]
mod tests {
    use super::Hook;
    use crate::abi::HookFlags;

    extern "system" fn original() {}

    static UNLINKED: Hook<extern "system" fn()> =
        Hook::new("UNLINKED", original, 0, HookFlags::NONE);

    #[test]
    #[should_panic(expected = "not linked")]
    fn an_unlinked_next_panics_instead_of_calling_null() {
        let _never = UNLINKED.next().get();
    }
}
