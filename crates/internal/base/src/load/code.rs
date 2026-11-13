use core::ffi::c_void;

use crate::abi::{HookFlags, MidHookSpec, Registers};
use crate::hook::{FnPtr, Hook};

use super::{BaseLoad, MidHook, Off, ok};

impl BaseLoad {
    /// As `Load::hook`.
    ///
    /// # Safety
    ///
    /// - `target` is the first byte of a game function, not the middle of one.
    /// - That function has the type `F`: the same arguments, return type and
    ///   calling convention. The loader checks neither, and the game calls
    ///   the hook's body with what it passes.
    /// - When `F` is a safe `fn` type, the function is safe to call with any
    ///   arguments, since the body calls it through `next` without `unsafe`.
    #[doc(hidden)]
    pub unsafe fn hook<F: FnPtr>(
        &mut self,
        hook: &'static Hook<F>,
        target: u64,
    ) -> Result<(), Off> {
        let spec = hook.spec(target);
        let status = (self.api().hook)(self.host.handle(), &raw const spec);
        ok(status, (), || format!("hook {}", hook.name()))
    }

    /// As `Load::code`.
    #[doc(hidden)]
    pub fn code(&self) -> Result<&[u8], Off> {
        let api = self.api();
        let (mut start, mut len) = (0u64, 0u64);
        let status = (api.code_range)(&raw mut start, &raw mut len);
        ok(status, (), || "code section".to_owned())?;
        let (Ok(start), Ok(len)) = (usize::try_from(start), usize::try_from(len)) else {
            return Err(Off::new("code section: out of range"));
        };
        if start == 0 {
            return Err(Off::new("code section: missing"));
        }
        // SAFETY: the loader names the game's code section, mapped readable
        // for the life of the process; nothing writes into it before the
        // loader arms, after every `init` and so after this borrow.
        Ok(unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance(start), len) })
    }

    /// As `Load::find_code`.
    #[doc(hidden)]
    pub fn find_code(&self, pattern: &str) -> Result<u64, Off> {
        let parsed = crate::code::Pattern::parse(pattern)
            .map_err(|e| Off::new(format!("code pattern {pattern:?}: {e}")))?;
        let code = self.code()?;
        let at = parsed
            .find_unique(code)
            .map_err(|e| Off::new(format!("code pattern {pattern:?}: {e}")))?;
        Ok(code.as_ptr().addr() as u64 + at as u64)
    }

    /// As `Load::patch_code`.
    ///
    /// # Safety
    ///
    /// - `replacement` is valid machine code at `address` for every path the
    ///   game takes through those bytes, including jumps into them.
    /// - The patched code keeps every register, stack and memory state the
    ///   game code after it relies on; the loader checks the bytes it
    ///   replaces, never what the new ones do.
    #[doc(hidden)]
    pub unsafe fn patch_code(
        &mut self,
        address: u64,
        expected: &[u8],
        replacement: &[u8],
    ) -> Result<(), Off> {
        let api = self.api();
        if expected.len() != replacement.len() {
            return Err(Off::new(format!(
                "code patch at {address:#x}: {} bytes expected, {} given",
                expected.len(),
                replacement.len()
            )));
        }
        let status = (api.patch_code)(
            self.host.handle(),
            address,
            expected.as_ptr(),
            replacement.as_ptr(),
            expected.len(),
        );
        ok(status, (), || format!("code patch at {address:#x}"))
    }

    /// As `Load::hook_at`.
    ///
    /// # Safety
    ///
    /// - `address` is the first byte of an instruction in a game function.
    /// - No code jumps into the 5 bytes from `address` other than to
    ///   `address` itself.
    /// - Every register `handler` writes holds a value the game code at
    ///   `address` can take: a wrong pointer or count in a register is a
    ///   wrong pointer or count in the game.
    #[doc(hidden)]
    pub unsafe fn hook_at<F>(&mut self, address: u64, handler: F) -> Result<(), Off>
    where
        F: Fn(&mut Registers) + Send + Sync + 'static,
    {
        let api = self.api();
        let user = Box::into_raw(Box::new(MidHook { address, handler }));
        let spec = MidHookSpec {
            address,
            handler: run_mid_hook::<F>,
            user: user.cast(),
            priority: 0,
            flags: HookFlags::NONE,
        };
        let status = (api.hook_at)(self.host.handle(), &raw const spec);
        if !status.is_ok() {
            // SAFETY: refused, so the loader keeps no copy of the pointer.
            drop(unsafe { Box::from_raw(user) });
        }
        ok(status, (), || format!("hook at {address:#x}"))
    }
}

pub(super) unsafe extern "system" fn run_mid_hook<F: Fn(&mut Registers)>(
    registers: *mut Registers,
    user: *mut c_void,
) {
    // SAFETY: `user` is the leaked `MidHook<F>` this function was registered with.
    let hook = unsafe { &*user.cast::<MidHook<F>>() };
    // SAFETY: the loader passes its saved registers, valid and unaliased for the call.
    let Some(registers) = (unsafe { registers.as_mut() }) else {
        return;
    };
    // The handler works on a copy, so a panic halfway leaves the game's
    // registers whole.
    let mut copy = *registers;
    match crate::host::caught(|| (hook.handler)(&mut copy)) {
        Ok(()) => *registers = copy,
        Err(_panicked) => {
            if let Some(host) = crate::host() {
                host.fault(&format!("panic in the hook at {:#x}", hook.address));
            }
        }
    }
}
