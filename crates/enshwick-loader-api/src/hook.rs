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
/// fn takes<F: enshwick_loader_api::FnPtr>() {}
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
/// enshwick_loader_api::detour! {
///     /// Counts upgrades.
///     static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade;
/// }
///
/// fn upgrade(next: enshwick_loader_api::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
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
    /// enshwick_loader_api::detour! {
    ///     static TICK: extern "system" fn() = tick;
    /// }
    ///
    /// fn tick(next: enshwick_loader_api::Next<extern "system" fn()>) {
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
/// One atomic load and an indirect call; copy it freely.
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
    /// # Examples
    ///
    /// See [`Hook`].
    #[inline]
    #[must_use]
    pub fn get(self) -> F {
        let address = self.slot.load(Ordering::Acquire);
        // SAFETY: the loader stores a function of type `F` in the slot before
        // the detour can run; `F` is a function pointer the size of `usize`.
        unsafe { core::mem::transmute_copy(&address) }
    }
}

/// Declares a hook: a static [`Hook`] and the entry point the loader links into
/// the chain.
///
/// The body is an ordinary function taking a [`Next`] and the target's
/// arguments. A panic in it is caught: the mod reports a fault, the loader
/// unlinks the mod, and this call falls through to the rest of the chain, so
/// the game runs vanilla instead of crashing.
///
/// Optional settings follow the function: `priority = <i32>` (lower runs
/// earlier) and `flags = <HookFlags>`. [`#[hook]`](macro@crate::hook) writes the
/// same declaration from the body's signature.
///
/// # Examples
///
/// ```
/// enshwick_loader_api::detour! {
///     /// The lookup every survivor caller goes through.
///     pub static LOOKUP: unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32 =
///         lookup, priority = -10, flags = enshwick_loader_api::abi::HookFlags::CALLER_FRAME;
/// }
///
/// fn lookup(
///     next: enshwick_loader_api::Next<unsafe extern "system" fn(*mut u8, *const u32) -> u32>,
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
            $($unsafe)? extern "system" fn entry($($arg: $ty),*) $(-> $ret)? {
                let next = $name.next();
                match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| $body(next, $($arg),*))) {
                    Ok(value) => value,
                    Err(_) => {
                        $crate::__private::fault_in(stringify!($name));
                        $crate::detour!(@call [$($unsafe)?] next ($($arg),*))
                    }
                }
            }
            $crate::Hook::new(
                stringify!($name),
                entry,
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
