use core::ffi::c_void;

use crate::abi::{HookFlags, MidHookSpec, Registers};
use crate::hook::{FnPtr, Hook};

use super::{Load, MidHook, Off, ok};

impl Load<'_> {
    /// Links `hook` into the chain of the game function at `target`.
    ///
    /// # Safety
    ///
    /// - `target` is the first byte of a game function, not the middle of one.
    /// - That function has the type `F`: the same arguments, return type and
    ///   calling convention. The loader checks neither, and the game calls
    ///   the hook's body with what it passes.
    /// - When `F` is a safe `fn` type, the function is safe to call with any
    ///   arguments, since the body calls it through `next` without `unsafe`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the link, for instance because another
    /// mod holds the target exclusively, or the hook is linked already.
    ///
    /// # Examples
    ///
    /// ```
    /// enshwick::detour! {
    ///     static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade;
    /// }
    ///
    /// fn upgrade(next: enshwick::Next<'_, unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    ///     // SAFETY: the original takes the same context.
    ///     unsafe { next.call(ctx) }
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let target = load.system("upgrade_building_range")?;
    ///     // SAFETY: the system takes the building context, as `UPGRADE` does.
    ///     unsafe { load.hook(&UPGRADE, target) }
    /// }
    /// ```
    pub unsafe fn hook<F: FnPtr>(
        &mut self,
        hook: &'static Hook<F>,
        target: u64,
    ) -> Result<(), Off> {
        let spec = hook.spec(target);
        let status = (self.api().hook)(self.host.handle(), &raw const spec);
        ok(status, (), || format!("hook {}", hook.name()))
    }

    /// The game's code section (`.text`) as loaded, before any hook or patch
    /// is written; the slice starts at its absolute address. It borrows the
    /// `Load`, since arming writes into the code after `init`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader cannot name the code section.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let code = load.code()?;
    ///     enshwick::info!("code at {:p}, {} bytes", code.as_ptr(), code.len());
    ///     Ok(())
    /// }
    /// ```
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

    /// The absolute address of the one place in the game's code that matches
    /// `pattern`: hex bytes separated by spaces, `??` for any byte (see
    /// [`Pattern`](crate::code::Pattern)). Unlike a pinned symbol it survives
    /// a game update that leaves those bytes alone.
    ///
    /// # Errors
    ///
    /// [`Off`] when the pattern is malformed, matches nothing or matches more
    /// than one place.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let site = load.find_code("B9 00 02 00 00 66 3B C1 0F 84 ?? ?? ?? ??")?;
    ///     assert_ne!(site, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn find_code(&self, pattern: &str) -> Result<u64, Off> {
        let parsed = crate::code::Pattern::parse(pattern)
            .map_err(|e| Off::new(format!("code pattern {pattern:?}: {e}")))?;
        let code = self.code()?;
        let at = parsed
            .find_unique(code)
            .map_err(|e| Off::new(format!("code pattern {pattern:?}: {e}")))?;
        Ok(code.as_ptr().addr() as u64 + at as u64)
    }

    /// Replaces game code at `address`: the bytes there must read `expected`
    /// and become `replacement`, of the same length. The loader checks the
    /// bytes again when it arms, writes the patch together with every hook,
    /// and logs it. A patch stays for the process, even when the mod is
    /// turned off later.
    ///
    /// # Safety
    ///
    /// - `replacement` is valid machine code at `address` for every path the
    ///   game takes through those bytes, including jumps into them.
    /// - The patched code keeps every register, stack and memory state the
    ///   game code after it relies on; the loader checks the bytes it
    ///   replaces, never what the new ones do.
    ///
    /// # Errors
    ///
    /// [`Off`] when the lengths differ, the bytes are
    /// not in the game's code or another patch covers any of them. At arming,
    /// a patch whose bytes no longer read `expected` or that overlaps a hooked
    /// function's first bytes is skipped and the mod turned off. A mod's
    /// patches go in together: when one is skipped or one of its hooks fails,
    /// none is written.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // `cmp eax, 8; jb` becomes `cmp eax, 16; jb`.
    ///     let site = load.find_code("83 F8 08 72 ??")?;
    ///     // SAFETY: an 8-bit immediate becomes another; the code stays valid.
    ///     unsafe { load.patch_code(site + 2, &[0x08], &[0x10]) }
    /// }
    /// ```
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

    /// Runs `handler` before the instruction at `address`, inside a game
    /// function, with the registers the game code has there.
    ///
    /// Writes to any field but `rsp` take effect when the game code goes on.
    /// Every mod's handler on one address runs, in load order, each seeing the
    /// writes of the ones before. A panic in `handler` turns the mod off and
    /// leaves the registers as they were; the game goes on unchanged and the
    /// handler is skipped from then on.
    ///
    /// `address` must start an instruction, and the instructions in the 5
    /// bytes from it must not be jumped into from elsewhere: the loader
    /// replaces them with a jump.
    ///
    /// # Safety
    ///
    /// - `address` is the first byte of an instruction in a game function.
    /// - No code jumps into the 5 bytes from `address` other than to
    ///   `address` itself.
    /// - Every register `handler` writes holds a value the game code at
    ///   `address` can take: a wrong pointer or count in a register is a
    ///   wrong pointer or count in the game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the hook: a second one of this mod on
    /// `address` or another mod holding it exclusively.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // An instruction inside a function, pinned per build in the mod's rows.
    ///     let site = load.symbol("building.range_check")?;
    ///     // SAFETY: the site starts an instruction and reads a count in `rax`;
    ///     // a lower count is one the game takes.
    ///     unsafe { load.hook_at(site, |registers| registers.rax = registers.rax.min(64)) }
    /// }
    /// ```
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
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (hook.handler)(&mut copy))) {
        Ok(()) => *registers = copy,
        Err(_) => {
            if let Some(host) = crate::host() {
                host.fault(&format!("panic in the hook at {:#x}", hook.address));
            }
        }
    }
}
