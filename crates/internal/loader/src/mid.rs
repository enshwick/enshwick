//! Mid-function hooks: mods' handlers at an instruction inside a game
//! function, with the registers there.
//!
//! Each hooked address gets one stub, wherever memory is free: it saves the
//! general-purpose registers, the flags and the vector state ([`Vectors`]),
//! calls [`dispatch`] with the saved registers and the address's [`Site`],
//! loads everything back but `rsp`, and jumps to the detour trampoline, which
//! runs the moved instructions and returns to the function. The jump over the
//! address is installed with the function detours, in the same batch.
//!
//! Several mods on one address share the stub: [`dispatch`] calls every
//! handler in running order, skipping mods that are off, so turning a mod off
//! touches no code either.

use core::ffi::c_void;
use std::sync::OnceLock;

use enshwick_loader_api::abi::{HookFlags, MidHookFn, Registers, Status};
use rustc_hash::FxHashMap;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READ, PAGE_READWRITE, VirtualAlloc, VirtualProtect,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// Bytes per stub: the code, `int3` padding and the continuation.
const STUB: usize = 768;
/// Where the stub keeps the continuation: its last 8 bytes.
const CONTINUATION: usize = STUB - 8;
/// Bytes of the saved registers on the stack: 16 registers and the flags.
const SAVED: u8 = 17 * 8;
/// Where the vector state lies above the stub's aligned stack pointer: past
/// the handler's shadow space, 64-byte aligned for `xsave`.
const AREA: u8 = 0x40;
/// Bytes of `ymm0`-`ymm15`.
const YMM: u32 = 16 * 32;
/// The largest `xsave` area a stub takes on the stack, under a page so the
/// stack's guard page is still touched in order.
const MAX_XSAVE: u32 = 3584;
/// `xsave` state components: SSE, AVX, AVX-512 opmasks, the upper halves of
/// `zmm0`-`zmm15`, and `zmm16`-`zmm31`.
const VECTOR_STATE: u64 = 0b1110_0110;
/// The AVX state component: the upper halves of `ymm0`-`ymm15`.
const AVX_STATE: u8 = 0b100;
/// The AVX-512 register components; the opmasks only matter with them.
const ZMM_STATE: u8 = 0b1100_0000;

/// How a stub keeps the vector registers of the hooked code across the
/// handlers.
///
/// At an instruction inside a function any vector register may hold a live
/// value. The calling convention keeps only the low halves of `xmm6`-`xmm15`
/// across the handler call, and AVX code in a handler or in the runtime it
/// calls ends with `vzeroupper`, which clears the upper halves of all of
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Vectors {
    /// `movaps` of the volatile `xmm0`-`xmm5`: a processor or system without
    /// AVX, where the calling convention keeps the rest.
    Sse,
    /// By the state in use as the stub runs: `xmm0`-`xmm5` and `vzeroupper`
    /// after the handlers when the upper halves are clear, which keeps SSE
    /// code after the hook free of AVX transition costs; `ymm0`-`ymm15` when
    /// they are not; and `xsave` of everything while AVX-512 registers are
    /// in use.
    Avx {
        /// Whether `xgetbv` reads the state in use; without it the stub
        /// takes the widest path every time.
        in_use: bool,
        /// The AVX-512 state the system turns on.
        wide: Option<Xsave>,
    },
}

/// An `xsave` of the vector state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Xsave {
    /// Whether the processor has `xsavec`.
    pub(crate) compact: bool,
    /// The state components saved.
    pub(crate) mask: u32,
    /// Bytes of the save area.
    pub(crate) size: u32,
}

impl Vectors {
    /// What this processor and system need, found once.
    pub(crate) fn current() -> Self {
        static FOUND: OnceLock<Vectors> = OnceLock::new();
        *FOUND.get_or_init(Self::detect)
    }

    fn detect() -> Self {
        use core::arch::x86_64::{__cpuid, __cpuid_count, _xgetbv};
        const OSXSAVE: u32 = 1 << 27;
        const XSAVEC: u32 = 1 << 1;
        const XGETBV_IN_USE: u32 = 1 << 2;
        const ALIGNED: u32 = 1 << 1;
        if __cpuid(1).ecx & OSXSAVE == 0 {
            return Self::Sse;
        }
        // SAFETY: the system has turned `xgetbv` on (OSXSAVE, checked above).
        let enabled = unsafe { _xgetbv(0) };
        let mask = enabled & VECTOR_STATE;
        if mask & u64::from(AVX_STATE) == 0 {
            return Self::Sse;
        }
        let features = __cpuid_count(0xd, 1).eax;
        let in_use = features & XGETBV_IN_USE != 0;
        if mask & u64::from(ZMM_STATE) == 0 {
            return Self::Avx { in_use, wide: None };
        }
        let compact = features & XSAVEC != 0;
        let size = if compact {
            // The legacy area and the header, then each component in order.
            (2..u64::BITS)
                .filter(|&c| mask & (1 << c) != 0)
                .fold(576_u32, |at, c| {
                    let leaf = __cpuid_count(0xd, c);
                    let at = if leaf.ecx & ALIGNED == 0 {
                        at
                    } else {
                        at.next_multiple_of(64)
                    };
                    at + leaf.eax
                })
        } else {
            __cpuid_count(0xd, 0).ebx
        };
        match u32::try_from(mask) {
            Ok(mask) if size <= MAX_XSAVE => Self::Avx {
                in_use,
                wide: Some(Xsave {
                    compact,
                    mask,
                    size,
                }),
            },
            _ => Self::Sse,
        }
    }
}

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
    /// Writes a stub calling `callback(registers, context)` for each pair,
    /// keeping the vector state as `vectors` says; `None` without memory.
    pub(crate) fn write(sites: &[(usize, usize)], vectors: Vectors) -> Option<Self> {
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
            let code = stub(callback, context, vectors);
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
        Self::write(&pairs, Vectors::current())
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

/// Machine code with forward `rel32` jumps.
struct Code(Vec<u8>);

impl Code {
    fn emit(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    /// `opcode` with a `rel32` bound later by [`bind`](Self::bind).
    fn jump(&mut self, opcode: &[u8]) -> usize {
        self.emit(opcode);
        let at = self.0.len();
        self.emit(&[0; 4]);
        at
    }

    /// Points the jumps at `sites` here.
    fn bind(&mut self, sites: &[usize]) {
        let here = self.0.len();
        for &site in sites {
            let distance = u32::try_from(here - (site + 4)).unwrap_or(0);
            self.0[site..site + 4].copy_from_slice(&distance.to_le_bytes());
        }
    }

    /// `opcode` with register `reg` and the operand `[rsp + displacement]`.
    fn stack(&mut self, opcode: &[u8], reg: u8, displacement: u32) {
        self.emit(opcode);
        self.emit(&[0x84 | (reg & 7) << 3, 0x24]);
        self.emit(&displacement.to_le_bytes());
    }

    /// `vmovdqu` of `ymm<reg>` to (`0x7f`) or from (`0x6f`) the stack.
    fn ymm(&mut self, opcode: u8, reg: u8, displacement: u32) {
        let vex = if reg < 8 { 0xfe } else { 0x7e };
        self.stack(&[0xc5, vex, opcode], reg, displacement);
    }

    /// `mov eax, mask; xor edx, edx`: the components for `xsave` and `xrstor`.
    fn mask(&mut self, mask: u32) {
        self.emit(&[0xb8]);
        self.emit(&mask.to_le_bytes());
        self.emit(&[0x31, 0xd2]);
    }
}

/// The code of one stub; the continuation is written by [`Stubs::finish`].
fn stub(callback: usize, context: usize, vectors: Vectors) -> [u8; STUB] {
    const MOVAPS: [u8; 6] = [0x44, 0x4c, 0x54, 0x5c, 0x64, 0x6c];
    let mut code = Code(Vec::with_capacity(STUB));
    // pushfq; push r15 .. r8; push rdi, rsi, rbp, rsp, rbx, rdx, rcx, rax:
    // `Registers` from rax up at the new stack pointer.
    code.emit(&[0x9c]);
    for reg in (0x50..=0x57).rev() {
        code.emit(&[0x41, reg]);
    }
    code.emit(&[0x57, 0x56, 0x55, 0x54, 0x53, 0x52, 0x51, 0x50]);
    // lea rax, [rsp + SAVED]; mov [rsp + 0x20], rax: rsp as it was.
    code.emit(&[0x48, 0x8d, 0x84, 0x24, SAVED, 0, 0, 0]);
    code.emit(&[0x48, 0x89, 0x44, 0x24, 0x20]);
    // mov rbx, rsp
    code.emit(&[0x48, 0x89, 0xe3]);
    let call = |code: &mut Code| {
        // cld; mov rcx, rbx; mov rdx, context; mov rax, callback; call rax
        code.emit(&[0xfc, 0x48, 0x89, 0xd9, 0x48, 0xba]);
        code.emit(&(context as u64).to_le_bytes());
        code.emit(&[0x48, 0xb8]);
        code.emit(&(callback as u64).to_le_bytes());
        code.emit(&[0xff, 0xd0]);
    };
    match vectors {
        Vectors::Sse => {
            // and rsp, -16; sub rsp, 0x80: room for xmm0-xmm5 above the
            // shadow space.
            code.emit(&[0x48, 0x83, 0xe4, 0xf0]);
            code.emit(&[0x48, 0x81, 0xec, 0x80, 0, 0, 0]);
            // movaps [rsp + 0x20 + 16 * n], xmm<n>
            for (n, modrm) in (0u8..).zip(MOVAPS) {
                code.emit(&[0x0f, 0x29, modrm, 0x24, 0x20 + 16 * n]);
            }
            call(&mut code);
            // movaps xmm<n>, [rsp + 0x20 + 16 * n]
            for (n, modrm) in (0u8..).zip(MOVAPS) {
                code.emit(&[0x0f, 0x28, modrm, 0x24, 0x20 + 16 * n]);
            }
        }
        Vectors::Avx { in_use, wide } => keep_avx(&mut code, in_use, wide, call),
    }
    // mov rsp, rbx; pop rax, rcx, rdx, rbx; add rsp, 8 (rsp stays);
    // pop rbp, rsi, rdi; pop r8 .. r15; popfq
    code.emit(&[0x48, 0x89, 0xdc]);
    code.emit(&[0x58, 0x59, 0x5a, 0x5b]);
    code.emit(&[0x48, 0x83, 0xc4, 0x08]);
    code.emit(&[0x5d, 0x5e, 0x5f]);
    for reg in 0x58..=0x5f {
        code.emit(&[0x41, reg]);
    }
    code.emit(&[0x9d]);
    // jmp [rip + distance]: on to the continuation.
    let jump = code.jump(&[0xff, 0x25]);
    let distance = u32::try_from(CONTINUATION.saturating_sub(jump + 4)).unwrap_or(0);
    code.0[jump..jump + 4].copy_from_slice(&distance.to_le_bytes());
    debug_assert!(code.0.len() <= CONTINUATION);
    let mut bytes = [0xcc; STUB];
    let len = code.0.len().min(CONTINUATION);
    bytes[..len].copy_from_slice(&code.0[..len]);
    bytes
}

/// Saves the vector state by what is in use, runs `call` and loads it back.
fn keep_avx(code: &mut Code, in_use: bool, wide: Option<Xsave>, call: impl Fn(&mut Code)) {
    let area = u32::from(AREA);
    let room = wide.map_or(YMM, |w| w.size.next_multiple_of(64).max(YMM));
    // and rsp, -64; sub rsp, AREA + room
    code.emit(&[0x48, 0x83, 0xe4, 0xc0, 0x48, 0x81, 0xec]);
    code.emit(&(area + room).to_le_bytes());
    if in_use {
        // mov ecx, 1; xgetbv; mov r12d, eax: the state in use.
        code.emit(&[0xb9, 1, 0, 0, 0, 0x0f, 0x01, 0xd0, 0x41, 0x89, 0xc4]);
    } else {
        // mov r12d, -1: all of it.
        code.emit(&[0x41, 0xbc, 0xff, 0xff, 0xff, 0xff]);
    }
    // test r12b, state; jnz: to the path that keeps it.
    let branch = |code: &mut Code, state: u8| {
        code.emit(&[0x41, 0xf6, 0xc4, state]);
        code.jump(&[0x0f, 0x85])
    };
    let save_wide = wide.map(|_| branch(code, ZMM_STATE));
    let save_ymm = branch(code, AVX_STATE);
    // movaps [rsp + AREA + 16 * n], xmm<n>
    for n in 0..6 {
        code.stack(&[0x0f, 0x29], n, area + 16 * u32::from(n));
    }
    let mut calls = vec![code.jump(&[0xe9])];
    code.bind(&[save_ymm]);
    for n in 0..16 {
        code.ymm(0x7f, n, area + 32 * u32::from(n));
    }
    if let (Some(save_wide), Some(wide)) = (save_wide, wide) {
        calls.push(code.jump(&[0xe9]));
        code.bind(&[save_wide]);
        // xor eax, eax; mov [rsp + AREA + 512 + 8 * n], rax: a clean
        // header, which `xrstor` checks and `xsave` leaves as it
        // finds it.
        code.emit(&[0x31, 0xc0]);
        for n in 0..8 {
            code.stack(&[0x48, 0x89], 0, area + 512 + 8 * n);
        }
        // xsavec64 or xsave64 [rsp + AREA]
        code.mask(wide.mask);
        let opcode = if wide.compact { 0xc7 } else { 0xae };
        code.emit(&[0x48, 0x0f, opcode, 0x64, 0x24, AREA]);
    }
    code.bind(&calls);
    call(code);
    let restore_wide = wide.map(|_| branch(code, ZMM_STATE));
    let restore_ymm = branch(code, AVX_STATE);
    // movaps xmm<n>, [rsp + AREA + 16 * n]; vzeroupper
    for n in 0..6 {
        code.stack(&[0x0f, 0x28], n, area + 16 * u32::from(n));
    }
    code.emit(&[0xc5, 0xf8, 0x77]);
    let mut ends = vec![code.jump(&[0xe9])];
    code.bind(&[restore_ymm]);
    for n in 0..16 {
        code.ymm(0x6f, n, area + 32 * u32::from(n));
    }
    if let (Some(restore_wide), Some(wide)) = (restore_wide, wide) {
        ends.push(code.jump(&[0xe9]));
        code.bind(&[restore_wide]);
        // xrstor64 [rsp + AREA]
        code.mask(wide.mask);
        code.emit(&[0x48, 0x0f, 0xae, 0x6c, 0x24, AREA]);
    }
    code.bind(&ends);
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
        install_with(target, callback, context, Vectors::current())
    }

    fn install_with(
        target: usize,
        callback: usize,
        context: usize,
        vectors: Vectors,
    ) -> Result<(), String> {
        let stubs = Stubs::write(&[(callback, context)], vectors).ok_or("no memory")?;
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

    /// The 5-byte `nop` the vector tests hook.
    const MARK: [u8; 5] = [0x0f, 0x1f, 0x44, 0x00, 0x00];

    /// The address of the first [`MARK`] in `function`.
    fn mark(function: Unary) -> usize {
        let start = (function as *const ()).addr();
        // SAFETY: each marked function is longer than 64 bytes of code.
        let code = unsafe { core::slice::from_raw_parts(start as *const u8, 64) };
        start + code.windows(MARK.len()).position(|w| w == MARK).unwrap()
    }

    /// Whether `xgetbv` reads the state in use, and the AVX-512 state, unless
    /// the system turns no AVX on.
    fn avx() -> Option<(bool, Option<Xsave>)> {
        match Vectors::current() {
            Vectors::Avx { in_use, wide } => Some((in_use, wide)),
            Vectors::Sse => None,
        }
    }

    /// Hooks the mark in `function` with `handler`.
    fn hook_mark(function: Unary, handler: usize, vectors: Vectors) {
        install_with(mark(function), handler, 0, vectors).unwrap();
    }

    /// Declares a function that holds `value` in the upper half of `ymm4`
    /// across the marked `nop`, then returns it.
    macro_rules! upper_ymm {
        ($name:ident) => {
            #[unsafe(naked)]
            extern "system" fn $name(value: u32) -> u32 {
                core::arch::naked_asm!(
                    "vmovd xmm1, ecx",
                    "vinsertf128 ymm4, ymm4, xmm1, 1",
                    ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
                    "vextractf128 xmm2, ymm4, 1",
                    "vmovd eax, xmm2",
                    "vzeroupper",
                    "ret",
                    ".fill 64, 1, 0xcc",
                )
            }
        };
    }

    upper_ymm!(upper_ymm_current);
    upper_ymm!(upper_ymm_always);

    /// What AVX code in a handler leaves behind: every upper half cleared.
    extern "system" fn clear_upper_halves(_registers: *mut Registers, _context: usize) {
        // SAFETY: clobbers `xmm4`, declared, and the upper halves of the
        // vector registers, which no code of this crate keeps live.
        unsafe {
            core::arch::asm!("vxorps ymm4, ymm4, ymm4", "vzeroupper", out("xmm4") _);
        }
    }

    #[test]
    fn upper_halves_of_vector_registers_survive_a_handler_using_avx() {
        let Some((_, wide)) = avx() else {
            return;
        };
        let handler = (clear_upper_halves as *const ()).addr();
        hook_mark(upper_ymm_current, handler, Vectors::current());
        assert_eq!(upper_ymm_current(0x1234_5678), 0x1234_5678);
        let always = Vectors::Avx {
            in_use: false,
            wide: None,
        };
        hook_mark(upper_ymm_always, handler, always);
        assert_eq!(upper_ymm_always(77), 77);
        assert!(wide.is_none_or(|w| w.size <= MAX_XSAVE));
    }

    /// Whether the upper halves are in use after the marked `nop`, with
    /// them cleared before it.
    #[unsafe(naked)]
    extern "system" fn upper_in_use(value: u32) -> u32 {
        core::arch::naked_asm!(
            "vzeroupper",
            ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
            "mov ecx, 1",
            "xgetbv",
            "and eax, 4",
            "ret",
            ".fill 64, 1, 0xcc",
        )
    }

    extern "system" fn fill_upper_halves(_registers: *mut Registers, _context: usize) {
        // SAFETY: clobbers `xmm4`, declared, and its upper half, which no
        // code of this crate keeps live.
        unsafe {
            core::arch::asm!("vcmptrueps ymm4, ymm4, ymm4", out("xmm4") _);
        }
    }

    #[test]
    fn clear_upper_halves_stay_clear_after_a_handler_using_avx() {
        if avx().is_none_or(|(in_use, _)| !in_use) {
            return;
        }
        let handler = (fill_upper_halves as *const ()).addr();
        hook_mark(upper_in_use, handler, Vectors::current());
        assert_eq!(upper_in_use(0), 0);
    }

    /// Declares a function that holds `value` in `zmm16` and in the top
    /// quarter of `zmm1` across the marked `nop`, then returns their sum.
    macro_rules! wide_zmm {
        ($name:ident) => {
            #[unsafe(naked)]
            extern "system" fn $name(value: u32) -> u32 {
                core::arch::naked_asm!(
                    "vpbroadcastd zmm16, ecx",
                    "vpbroadcastd zmm1, ecx",
                    ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
                    "vextracti32x4 xmm2, zmm16, 3",
                    "vmovd eax, xmm2",
                    "vextracti32x4 xmm3, zmm1, 3",
                    "vmovd edx, xmm3",
                    "add eax, edx",
                    "vzeroupper",
                    "ret",
                    ".fill 64, 1, 0xcc",
                )
            }
        };
    }

    wide_zmm!(wide_zmm_current);
    wide_zmm!(wide_zmm_always);

    extern "system" fn clear_wide_registers(_registers: *mut Registers, _context: usize) {
        // SAFETY: clobbers `xmm1`, declared, `zmm16` and the upper halves,
        // which no code of this crate keeps live.
        unsafe {
            core::arch::asm!(
                "vpxord zmm16, zmm16, zmm16",
                "vxorps ymm1, ymm1, ymm1",
                "vzeroupper",
                out("xmm1") _,
            );
        }
    }

    #[test]
    fn avx512_registers_survive_where_the_system_turns_them_on() {
        let Some((_, Some(wide))) = avx() else {
            return;
        };
        let handler = (clear_wide_registers as *const ()).addr();
        hook_mark(wide_zmm_current, handler, Vectors::current());
        assert_eq!(wide_zmm_current(21), 42);
        let standard = Xsave {
            compact: false,
            size: core::arch::x86_64::__cpuid_count(0xd, 0).ebx,
            ..wide
        };
        let always = Vectors::Avx {
            in_use: false,
            wide: Some(standard),
        };
        hook_mark(wide_zmm_always, handler, always);
        assert_eq!(wide_zmm_always(5), 10);
    }

    extern "system" fn nothing(_registers: *mut Registers, _context: usize) {}

    /// `x + 6` with the upper half of `ymm4` in use across the marked `nop`.
    const DIRTY: [u8; 20] = [
        0xc4, 0xe3, 0x5d, 0x18, 0xe4, 0x01, // vinsertf128 ymm4, ymm4, xmm4, 1
        0x0f, 0x1f, 0x44, 0x00, 0x00, // nop <- hooked
        0x8d, 0x41, 0x06, // lea eax, [rcx + 6]
        0xc5, 0xf8, 0x77, // vzeroupper
        0xc3, 0xcc, 0xcc, // ret
    ];

    /// `x + 6` with the upper halves clear across the marked `nop`.
    const CLEAN: [u8; 15] = [
        0xc5, 0xf8, 0x77, // vzeroupper
        0x0f, 0x1f, 0x44, 0x00, 0x00, // nop <- hooked
        0x8d, 0x41, 0x06, // lea eax, [rcx + 6]
        0xc3, 0xcc, 0xcc, 0xcc, // ret
    ];

    /// Nanoseconds per call of `function`.
    fn per_call(function: Unary) -> f64 {
        const CALLS: u32 = 2_000_000;
        let start = std::time::Instant::now();
        let mut sum = 0u32;
        for i in 0..CALLS {
            sum = sum.wrapping_add(function(core::hint::black_box(i)));
        }
        core::hint::black_box(sum);
        start.elapsed().as_secs_f64() * 1e9 / f64::from(CALLS)
    }

    #[test]
    #[ignore = "a measurement: cargo test -p enshwick-loader --release --lib stub_cost -- --ignored --nocapture"]
    fn stub_cost() {
        let Some((in_use, wide)) = avx() else {
            return;
        };
        let callback = (nothing as *const ()).addr();
        let mut variants = vec![
            ("sse", Vectors::Sse),
            ("current", Vectors::current()),
            (
                "ymm always",
                Vectors::Avx {
                    in_use: false,
                    wide: None,
                },
            ),
        ];
        if wide.is_some() {
            variants.push((
                "xsave always",
                Vectors::Avx {
                    in_use: false,
                    wide,
                },
            ));
        }
        println!("xgetbv reads the state in use: {in_use}; AVX-512: {wide:?}");
        for (code, state) in [(&CLEAN[..], "clear"), (&DIRTY[..], "in use")] {
            let at = code.windows(MARK.len()).position(|w| w == MARK).unwrap();
            let bare = per_call(callable(function(code)));
            println!("upper halves {state}, no hook: {bare:.2} ns");
            for &(name, vectors) in &variants {
                let target = function(code);
                install_with(target + at, callback, 0, vectors).unwrap();
                let hooked = per_call(callable(target));
                println!(
                    "upper halves {state}, {name}: {hooked:.2} ns (+{:.2})",
                    hooked - bare
                );
            }
        }
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
