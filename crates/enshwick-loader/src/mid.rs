//! Mid-function hooks: mods' handlers at an instruction inside a game
//! function, with the registers there.
//!
//! Each hooked address gets one stub, wherever memory is free: it saves the
//! general-purpose registers, the flags and the volatile `xmm0`-`xmm5`, calls
//! [`dispatch`] with the saved registers and the address's [`Site`], loads
//! everything back but `rsp`, and jumps to the detour trampoline, which runs
//! the moved instructions and returns to the function. The jump over the
//! address is installed with the function detours, in the same batch.
//!
//! Several mods on one address share the stub: [`dispatch`] calls every
//! handler in running order, skipping mods that are off, so turning a mod off
//! touches no code either.

use core::ffi::c_void;

use enshwick_api::abi::{HookFlags, MidHookFn, Registers, Status};
use rustc_hash::FxHashMap;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READ, PAGE_READWRITE, VirtualAlloc, VirtualProtect,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// Bytes per stub: the code, the continuation and `int3` padding.
const STUB: usize = 192;
/// Where the stub keeps the continuation: right after `jmp [rip + 0]`.
const CONTINUATION: usize = 175;
/// Bytes of the saved registers on the stack: 16 registers and the flags.
const SAVED: u8 = 17 * 8;

/// One mod's handler at an address.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MidLink {
    /// The owning mod's index.
    pub(crate) owner: usize,
    /// The handler, inside the mod's DLL.
    pub(crate) handler: MidHookFn,
    /// The mod's pointer for the handler, as an address.
    pub(crate) user: usize,
    /// Lower runs earlier.
    pub(crate) priority: i32,
    /// Hook flags.
    pub(crate) flags: HookFlags,
}

/// Every handler on one address, in running order; built when arming and
/// never freed.
#[derive(Debug)]
pub(crate) struct Site {
    /// The hooked instruction.
    pub(crate) address: u64,
    links: Box<[MidLink]>,
    /// Whether a mod is off, by index.
    off: fn(usize) -> bool,
}

/// Every mid-function hook, keyed by address.
#[derive(Debug, Default)]
pub(crate) struct Mids {
    sites: FxHashMap<u64, Vec<MidLink>>,
    armed: bool,
}

impl Mids {
    /// Adds `link` at `address`.
    ///
    /// Refused after arming, for a second link of the same mod on one address,
    /// and when either side claims the address exclusively.
    pub(crate) fn add(&mut self, address: u64, link: MidLink) -> Status {
        if self.armed {
            return Status::TOO_LATE;
        }
        if address == 0 {
            return Status::BAD_ARGS;
        }
        let links = self.sites.entry(address).or_default();
        let clash = links.iter().any(|l| {
            l.owner == link.owner
                || l.flags.contains(HookFlags::EXCLUSIVE)
                || link.flags.contains(HookFlags::EXCLUSIVE)
        });
        if clash {
            return Status::REFUSED;
        }
        links.push(link);
        Status::OK
    }

    /// Marks the hooks armed: no more links.
    pub(crate) fn seal(&mut self) {
        self.armed = true;
    }

    /// Every address with its links in running order, for arming.
    pub(crate) fn sites(&self, off: fn(usize) -> bool) -> Vec<&'static Site> {
        self.sites
            .iter()
            .map(|(&address, links)| {
                let mut links = links.clone();
                links.sort_by_key(|l| l.priority);
                &*Box::leak(Box::new(Site {
                    address,
                    links: links.into_boxed_slice(),
                    off,
                }))
            })
            .collect()
    }

    /// The mods with a handler at `address`.
    pub(crate) fn owners(&self, address: u64) -> Vec<usize> {
        self.sites
            .get(&address)
            .map(|links| links.iter().map(|l| l.owner).collect())
            .unwrap_or_default()
    }

    /// Every address with handlers.
    pub(crate) fn addresses(&self) -> Vec<u64> {
        self.sites.keys().copied().collect()
    }

    /// How many addresses have handlers.
    pub(crate) fn len(&self) -> usize {
        self.sites.len()
    }
}

/// Calls every handler at `site` whose mod is on; what each stub calls.
pub(crate) extern "system" fn dispatch(registers: *mut Registers, site: *const Site) {
    // SAFETY: the stub passes the leaked site it was written for.
    let Some(site) = (unsafe { site.as_ref() }) else {
        return;
    };
    for link in &site.links {
        if !(site.off)(link.owner) {
            let user = core::ptr::with_exposed_provenance_mut::<c_void>(link.user);
            // SAFETY: the mod registered the handler with this pointer; it
            // takes the saved registers and catches its own panics.
            unsafe { (link.handler)(registers, user) };
        }
    }
}

/// The stubs of one batch, writable until sealed.
#[derive(Debug)]
pub(crate) struct Stubs {
    base: *mut u8,
    len: usize,
}

impl Stubs {
    /// Writes a stub calling `callback(registers, context)` for each pair;
    /// `None` without memory.
    pub(crate) fn write(sites: &[(usize, usize)]) -> Option<Self> {
        let len = sites.len().max(1) * STUB;
        // SAFETY: a fresh reservation; null on failure.
        let base = unsafe {
            VirtualAlloc(
                core::ptr::null(),
                len,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_READWRITE,
            )
        }
        .cast::<u8>();
        if base.is_null() {
            return None;
        }
        for (i, &(callback, context)) in sites.iter().enumerate() {
            let code = stub(callback, context);
            // SAFETY: stub `i` lies inside the `len` writable bytes at `base`.
            let at = unsafe { base.add(i * STUB) };
            // SAFETY: as above, `STUB` bytes from `at`.
            unsafe { at.copy_from_nonoverlapping(code.as_ptr(), STUB) };
        }
        Some(Self { base, len })
    }

    /// A stub calling [`dispatch`] for each site, in order; `None` for no
    /// sites or without memory.
    pub(crate) fn dispatching(sites: &[&'static Site]) -> Option<Self> {
        if sites.is_empty() {
            return None;
        }
        let callback = (dispatch as *const ()).addr();
        let pairs: Vec<(usize, usize)> = sites
            .iter()
            .map(|&site| (callback, core::ptr::from_ref(site).addr()))
            .collect();
        Self::write(&pairs)
    }

    /// The entry of stub `i`.
    pub(crate) fn entry(&self, i: usize) -> usize {
        self.base.addr() + i * STUB
    }

    /// Points stub `i` on to `continuation` once it has run the handlers.
    pub(crate) fn finish(&self, i: usize, continuation: usize) {
        if (i + 1) * STUB > self.len {
            return;
        }
        let bytes = (continuation as u64).to_le_bytes();
        // SAFETY: inside stub `i`, checked above.
        let at = unsafe { self.base.add(i * STUB + CONTINUATION) };
        // SAFETY: the stub's last 8 code bytes, writable until `seal`.
        unsafe { at.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
    }

    /// Makes the stubs executable and no longer writable; false when they
    /// could not be.
    pub(crate) fn seal(&self) -> bool {
        let mut old = 0;
        // SAFETY: the stub pages belong to this batch.
        let sealed =
            unsafe { VirtualProtect(self.base.cast(), self.len, PAGE_EXECUTE_READ, &raw mut old) };
        // SAFETY: no preconditions.
        let process = unsafe { GetCurrentProcess() };
        // SAFETY: flushing our own process's freshly written code.
        unsafe { FlushInstructionCache(process, self.base.cast(), self.len) };
        sealed != 0
    }
}

/// The code of one stub; the continuation is written by [`Stubs::finish`].
fn stub(callback: usize, context: usize) -> [u8; STUB] {
    const MOVAPS: [u8; 6] = [0x44, 0x4c, 0x54, 0x5c, 0x64, 0x6c];
    let mut code = Vec::with_capacity(STUB);
    // pushfq; push r15 .. r8; push rdi, rsi, rbp, rsp, rbx, rdx, rcx, rax:
    // `Registers` from rax up at the new stack pointer.
    code.push(0x9c);
    for reg in (0x50..=0x57).rev() {
        code.extend_from_slice(&[0x41, reg]);
    }
    code.extend_from_slice(&[0x57, 0x56, 0x55, 0x54, 0x53, 0x52, 0x51, 0x50]);
    // lea rax, [rsp + SAVED]; mov [rsp + 0x20], rax: rsp as it was.
    code.extend_from_slice(&[0x48, 0x8d, 0x84, 0x24, SAVED, 0, 0, 0]);
    code.extend_from_slice(&[0x48, 0x89, 0x44, 0x24, 0x20]);
    // mov rbx, rsp; and rsp, -16; sub rsp, 0x80: aligned, with room for
    // xmm0-xmm5 above the shadow space.
    code.extend_from_slice(&[0x48, 0x89, 0xe3]);
    code.extend_from_slice(&[0x48, 0x83, 0xe4, 0xf0]);
    code.extend_from_slice(&[0x48, 0x81, 0xec, 0x80, 0, 0, 0]);
    // movaps [rsp + 0x20 + 16 * n], xmm<n>
    for (n, modrm) in (0u8..).zip(MOVAPS) {
        code.extend_from_slice(&[0x0f, 0x29, modrm, 0x24, 0x20 + 16 * n]);
    }
    // cld; mov rcx, rbx; mov rdx, context; mov rax, callback; call rax
    code.push(0xfc);
    code.extend_from_slice(&[0x48, 0x89, 0xd9]);
    code.extend_from_slice(&[0x48, 0xba]);
    code.extend_from_slice(&(context as u64).to_le_bytes());
    code.extend_from_slice(&[0x48, 0xb8]);
    code.extend_from_slice(&(callback as u64).to_le_bytes());
    code.extend_from_slice(&[0xff, 0xd0]);
    // movaps xmm<n>, [rsp + 0x20 + 16 * n]
    for (n, modrm) in (0u8..).zip(MOVAPS) {
        code.extend_from_slice(&[0x0f, 0x28, modrm, 0x24, 0x20 + 16 * n]);
    }
    // mov rsp, rbx; pop rax, rcx, rdx, rbx; add rsp, 8 (rsp stays);
    // pop rbp, rsi, rdi; pop r8 .. r15; popfq
    code.extend_from_slice(&[0x48, 0x89, 0xdc]);
    code.extend_from_slice(&[0x58, 0x59, 0x5a, 0x5b]);
    code.extend_from_slice(&[0x48, 0x83, 0xc4, 0x08]);
    code.extend_from_slice(&[0x5d, 0x5e, 0x5f]);
    for reg in 0x58..=0x5f {
        code.extend_from_slice(&[0x41, reg]);
    }
    code.push(0x9d);
    // jmp [rip + 0], then the continuation.
    code.extend_from_slice(&[0xff, 0x25, 0, 0, 0, 0]);
    debug_assert_eq!(code.len(), CONTINUATION);
    code.resize(STUB, 0xcc);
    let mut bytes = [0xcc; STUB];
    bytes.copy_from_slice(&code);
    bytes
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

    use windows_sys::Win32::System::Memory::PAGE_EXECUTE_READWRITE;

    use super::*;
    use crate::detour::{self, Request};

    type Unary = extern "system" fn(u32) -> u32;

    /// Writes `code` to fresh executable memory and returns its address.
    fn function(code: &[u8]) -> usize {
        // SAFETY: a fresh allocation, written before it is executed.
        let page = unsafe {
            VirtualAlloc(
                core::ptr::null(),
                4096,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_EXECUTE_READWRITE,
            )
        };
        assert!(!page.is_null());
        // SAFETY: 4096 writable bytes.
        unsafe {
            page.cast::<u8>()
                .copy_from_nonoverlapping(code.as_ptr(), code.len());
        }
        page.addr()
    }

    fn callable(address: usize) -> Unary {
        // SAFETY: every test function takes a u32 in ecx and returns one in eax.
        unsafe { core::mem::transmute::<usize, Unary>(address) }
    }

    /// Hooks the instruction at `target` with `callback(registers, context)`.
    fn install(target: usize, callback: usize, context: usize) -> Result<(), String> {
        let stubs = Stubs::write(&[(callback, context)]).ok_or("no memory")?;
        let pending = detour::prepare(&[Request {
            target,
            detour: stubs.entry(0),
            inside: true,
        }])
        .into_iter()
        .next()
        .unwrap()?;
        stubs.finish(0, pending.original);
        assert!(stubs.seal());
        assert!(detour::commit(&[&pending], &[]));
        Ok(())
    }

    /// mov eax, ecx; add eax, 1; add eax, 2; add eax, 3; ret: `x + 6`, hooked
    /// at offset 2.
    const SUM: [u8; 12] = [
        0x89, 0xc8, 0x83, 0xc0, 0x01, 0x83, 0xc0, 0x02, 0x83, 0xc0, 0x03, 0xc3,
    ];

    static SEEN: AtomicU64 = AtomicU64::new(0);

    extern "system" fn tenfold(registers: *mut Registers, _context: usize) {
        // SAFETY: the stub passes its saved registers.
        let registers = unsafe { &mut *registers };
        SEEN.store(registers.rax, Ordering::Relaxed);
        registers.rax *= 10;
    }

    #[test]
    fn a_handler_reads_and_writes_the_registers_at_the_address() {
        let target = function(&SUM);
        assert_eq!(callable(target)(1), 7);
        install(target + 2, (tenfold as *const ()).addr(), 0).unwrap();
        assert_eq!(callable(target)(1), 16);
        assert_eq!(SEEN.load(Ordering::Relaxed), 1);
        assert_eq!(callable(target)(3), 36);
    }

    static RSP_MATCHES: AtomicBool = AtomicBool::new(false);

    extern "system" fn compare_stack(registers: *mut Registers, _context: usize) {
        // SAFETY: the stub passes its saved registers.
        let registers = unsafe { &mut *registers };
        RSP_MATCHES.store(registers.rax == registers.rsp, Ordering::Relaxed);
        registers.rsp = 0;
    }

    #[test]
    fn the_handler_sees_the_stack_pointer_of_the_address_and_cannot_move_it() {
        let target = function(&[
            0x48, 0x89, 0xe0, // mov rax, rsp
            0x48, 0x83, 0xc0, 0x00, // add rax, 0 <- hooked
            0x90, // nop
            0x48, 0x29, 0xe0, // sub rax, rsp
            0xc3, // ret
        ]);
        install(target + 3, (compare_stack as *const ()).addr(), 0).unwrap();
        assert_eq!(callable(target)(0), 0);
        assert!(RSP_MATCHES.load(Ordering::Relaxed));
    }

    extern "system" fn clobber_vectors(_registers: *mut Registers, _context: usize) {
        // SAFETY: clobbers only volatile registers, declared to the compiler.
        unsafe {
            core::arch::asm!(
                "xorps xmm0, xmm0",
                "xorps xmm5, xmm5",
                "xor eax, eax",
                out("xmm0") _,
                out("xmm5") _,
                out("eax") _,
            );
        }
    }

    #[test]
    fn volatile_vector_registers_and_flags_survive_the_handler() {
        let target = function(&[
            0x66, 0x0f, 0x6e, 0xc1, // movd xmm0, ecx
            0x83, 0xf9, 0x05, // cmp ecx, 5
            0x0f, 0x1f, 0x00, // nop dword [rax] <- hooked
            0x0f, 0x1f, 0x00, // nop dword [rax]
            0x66, 0x0f, 0x7e, 0xc0, // movd eax, xmm0
            0x72, 0x02, // jb +2: no doubling below 5
            0x01, 0xc0, // add eax, eax
            0xc3, // ret
        ]);
        install(target + 7, (clobber_vectors as *const ()).addr(), 0).unwrap();
        assert_eq!(callable(target)(42), 84);
        assert_eq!(callable(target)(3), 3);
    }

    static PANICS: AtomicU32 = AtomicU32::new(0);
    static FAULTED: AtomicBool = AtomicBool::new(false);

    /// What the API's wrapper does: work on a copy, fault on a panic.
    unsafe extern "system" fn panicking(registers: *mut Registers, _user: *mut c_void) {
        // SAFETY: the dispatcher passes the stub's saved registers.
        let registers = unsafe { &mut *registers };
        let mut copy = *registers;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            PANICS.fetch_add(1, Ordering::Relaxed);
            copy.rax = 999;
            panic!("a mod bug");
        }));
        match outcome {
            Ok(()) => *registers = copy,
            Err(_) => FAULTED.store(true, Ordering::Relaxed),
        }
    }

    unsafe extern "system" fn add_hundred(registers: *mut Registers, _user: *mut c_void) {
        // SAFETY: the dispatcher passes the stub's saved registers.
        unsafe { (*registers).rax += 100 };
    }

    fn link(owner: usize, handler: MidHookFn, priority: i32) -> MidLink {
        MidLink {
            owner,
            handler,
            user: 0,
            priority,
            flags: HookFlags::NONE,
        }
    }

    #[test]
    fn a_panicking_handler_changes_nothing_and_is_skipped_once_its_mod_is_off() {
        let mut mids = Mids::default();
        assert_eq!(mids.add(0x10, link(1, add_hundred, 0)), Status::OK);
        assert_eq!(mids.add(0x10, link(0, panicking, -1)), Status::OK);
        let off: fn(usize) -> bool = |owner| owner == 0 && FAULTED.load(Ordering::Relaxed);
        let site = mids.sites(off)[0];
        let target = function(&SUM);
        install(
            target + 2,
            (dispatch as *const ()).addr(),
            core::ptr::from_ref(site).addr(),
        )
        .unwrap();
        assert_eq!(callable(target)(1), 107);
        assert!(FAULTED.load(Ordering::Relaxed));
        assert_eq!(callable(target)(1), 107);
        assert_eq!(PANICS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn an_address_whose_bytes_leave_before_the_jump_ends_is_refused() {
        // add eax, 1; ret; padding: hooked at the `ret`.
        let ret = function(&[0x83, 0xc0, 0x01, 0xc3, 0xcc, 0xcc, 0xcc, 0xcc]);
        assert!(install(ret + 3, (tenfold as *const ()).addr(), 0).is_err());
        // jmp +0; mov eax, 1; ret
        let jump = function(&[0xeb, 0x00, 0xb8, 0x01, 0, 0, 0, 0xc3]);
        assert!(install(jump, (tenfold as *const ()).addr(), 0).is_err());
    }

    #[test]
    fn duplicate_and_exclusive_handlers_are_refused() {
        let mut mids = Mids::default();
        let exclusive = MidLink {
            flags: HookFlags::EXCLUSIVE,
            ..link(0, add_hundred, 0)
        };
        assert_eq!(mids.add(0x10, exclusive), Status::OK);
        assert_eq!(mids.add(0x10, link(1, add_hundred, 0)), Status::REFUSED);
        assert_eq!(mids.add(0x20, link(1, add_hundred, 0)), Status::OK);
        assert_eq!(mids.add(0x20, link(1, add_hundred, 0)), Status::REFUSED);
        assert_eq!(mids.add(0x20, link(2, add_hundred, 0)), Status::OK);
        assert_eq!(mids.add(0, link(3, add_hundred, 0)), Status::BAD_ARGS);
        assert_eq!(mids.owners(0x20), [1, 2]);
        mids.seal();
        assert_eq!(mids.add(0x30, link(3, add_hundred, 0)), Status::TOO_LATE);
        assert_eq!(mids.len(), 2);
    }
}
