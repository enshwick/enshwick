//! Hooks: detours, mid-function hooks and the registers they see.

use core::ffi::c_void;
use core::sync::atomic::AtomicUsize;

/// Flags of a [`HookSpec`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::HookFlags;
///
/// let flags = HookFlags(HookFlags::EXCLUSIVE.0 | HookFlags::CALLER_FRAME.0);
/// assert!(flags.contains(HookFlags::CALLER_FRAME));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HookFlags(pub u32);

impl HookFlags {
    /// No flag.
    pub const NONE: Self = Self(0);
    /// No other mod may link on this target.
    pub const EXCLUSIVE: Self = Self(1);
    /// The link reads the game's call site through
    /// [`HostApi::caller_frame`](super::HostApi::caller_frame).
    pub const CALLER_FRAME: Self = Self(2);

    /// Whether every flag of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`HookFlags`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// A request to link a detour into the chain of a game function.
///
/// The loader points `next` at the following link or at the original
/// function when it arms the chain; until then `next` holds 0 and the detour
/// is never called.
///
/// # Examples
///
/// ```
/// use core::sync::atomic::{AtomicU32, AtomicUsize};
/// use enshwick::abi::{HookFlags, HookSpec};
///
/// static NEXT: AtomicUsize = AtomicUsize::new(0);
/// extern "system" fn detour() {}
///
/// let spec = HookSpec {
///     target: 0x1_4009_0a60,
///     detour: detour as usize,
///     next: &NEXT,
///     priority: 0,
///     flags: HookFlags::NONE,
/// };
/// assert_eq!(spec.priority, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HookSpec {
    /// The absolute address of the game function.
    pub target: u64,
    /// The detour, with the target's signature and the `system` ABI.
    pub detour: usize,
    /// A static slot the loader keeps pointing at what comes next.
    pub next: *const AtomicUsize,
    /// Lower runs earlier; equal priorities run in load order.
    pub priority: i32,
    /// Hook flags.
    pub flags: HookFlags,
}

/// The game caller's context at entry of a hook chain: its stack pointer and
/// callee-saved registers, for links that tell callers apart.
///
/// # Examples
///
/// ```
/// use enshwick::abi::CallerFrame;
///
/// let frame = CallerFrame { rsp: 0x1000, ..CallerFrame::default() };
/// assert_eq!(frame.stack(), 0x1008);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CallerFrame {
    /// The stack pointer at entry; it points at the return address.
    pub rsp: u64,
    /// `rbx` at entry.
    pub rbx: u64,
    /// `rbp` at entry.
    pub rbp: u64,
    /// `rdi` at entry.
    pub rdi: u64,
    /// `rsi` at entry.
    pub rsi: u64,
    /// `r12` at entry.
    pub r12: u64,
    /// `r13` at entry.
    pub r13: u64,
    /// `r14` at entry.
    pub r14: u64,
    /// `r15` at entry.
    pub r15: u64,
}

impl CallerFrame {
    /// The caller's stack above the return address, where it keeps its locals.
    ///
    /// # Examples
    ///
    /// See [`CallerFrame`].
    #[must_use]
    pub const fn stack(&self) -> u64 {
        self.rsp + 8
    }

    /// The address the hooked function returns to, which names its caller.
    ///
    /// # Safety
    ///
    /// - `rsp` is readable for eight bytes; no alignment is needed. A frame
    ///   the loader passed meets this until the hooked function returns:
    ///   `rsp` points at its return address on the calling thread's stack.
    /// - No other thread writes those eight bytes during the read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::CallerFrame;
    ///
    /// let stack = [0x1_4004_05cb_u64, 0];
    /// let frame = CallerFrame { rsp: stack.as_ptr() as u64, ..CallerFrame::default() };
    /// // SAFETY: `rsp` points at a readable `u64`.
    /// assert_eq!(unsafe { frame.return_address() }, 0x1_4004_05cb);
    /// ```
    #[must_use]
    pub unsafe fn return_address(&self) -> u64 {
        // SAFETY: the caller guarantees `rsp` readable for eight bytes with no
        // concurrent writer; the read is unaligned.
        unsafe { (self.rsp as *const u64).read_unaligned() }
    }
}

/// The general-purpose registers and flags at a mid-function hook, as the
/// interrupted game code had them.
///
/// The loader saves them before it calls the handlers of a hook placed with
/// [`HostApi::hook_at`](super::HostApi::hook_at) and loads them back afterwards, so a handler that
/// writes a field changes what the game code sees next. `rsp` is read-only:
/// the stack stays where it was.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Registers;
///
/// let mut registers = Registers { rcx: 7, ..Registers::default() };
/// registers.rax = registers.rcx * 2;
/// assert_eq!(registers.rax, 14);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Registers {
    /// `rax`.
    pub rax: u64,
    /// `rcx`.
    pub rcx: u64,
    /// `rdx`.
    pub rdx: u64,
    /// `rbx`.
    pub rbx: u64,
    /// The stack pointer at the hooked instruction; writes are ignored.
    pub rsp: u64,
    /// `rbp`.
    pub rbp: u64,
    /// `rsi`.
    pub rsi: u64,
    /// `rdi`.
    pub rdi: u64,
    /// `r8`.
    pub r8: u64,
    /// `r9`.
    pub r9: u64,
    /// `r10`.
    pub r10: u64,
    /// `r11`.
    pub r11: u64,
    /// `r12`.
    pub r12: u64,
    /// `r13`.
    pub r13: u64,
    /// `r14`.
    pub r14: u64,
    /// `r15`.
    pub r15: u64,
    /// `rflags`.
    pub rflags: u64,
}

/// A handler of a mid-function hook: the saved registers, writable, and the
/// pointer it was registered with. It must not unwind.
pub type MidHookFn = unsafe extern "system" fn(registers: *mut Registers, user: *mut c_void);

/// A request to run a handler at an instruction inside a game function.
///
/// Every mod's handler on one address runs, by priority and then load order,
/// each seeing the writes of the ones before; then the instructions the jump
/// replaced run and the function goes on.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{HookFlags, MidHookSpec, Registers};
///
/// unsafe extern "system" fn handler(registers: *mut Registers, _user: *mut core::ffi::c_void) {
///     // SAFETY: the loader passes its saved registers, valid for the call.
///     if let Some(registers) = unsafe { registers.as_mut() } {
///         registers.rax = 0;
///     }
/// }
///
/// let spec = MidHookSpec {
///     address: 0x1_4009_0a6c,
///     handler,
///     user: core::ptr::null_mut(),
///     priority: 0,
///     flags: HookFlags::NONE,
/// };
/// assert_eq!(spec.priority, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MidHookSpec {
    /// The absolute address of the instruction to run the handler before.
    pub address: u64,
    /// The handler.
    pub handler: MidHookFn,
    /// Passed to the handler on every call.
    pub user: *mut c_void,
    /// Lower runs earlier; equal priorities run in load order.
    pub priority: i32,
    /// [`HookFlags::EXCLUSIVE`] or none.
    pub flags: HookFlags,
}
