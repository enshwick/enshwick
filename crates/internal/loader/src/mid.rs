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

use enshwick::abi::{HookFlags, MidHookFn, Registers, Status};
use rustc_hash::FxHashMap;
use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc};

/// Bytes per stub: the code, `int3` padding, [`UPPER`] and the continuation.
const STUB: usize = 2048;
/// Where the stub keeps the continuation: its last 8 bytes.
const CONTINUATION: usize = STUB - 8;
/// Where the stub keeps [`UPPER`].
const UPPER_AT: usize = STUB - 64;
/// A `vptest` mask of the upper half of a `ymm` register.
const UPPER: [u8; 32] = {
    let mut mask = [0; 32];
    let mut i = 16;
    while i < 32 {
        mask[i] = 0xff;
        i += 1;
    }
    mask
};
/// Bytes of the saved registers on the stack: 16 registers and the flags.
const SAVED: u8 = 17 * 8;
/// Where the vector registers lie above the stub's aligned stack pointer:
/// past the handler's shadow space.
const AREA: u32 = 0x40;
/// Bytes per register where a stub may keep whole `zmm` registers.
const ZMM: u32 = 64;
/// Bytes per register where a stub keeps at most `ymm` registers.
const YMM: u32 = 32;
/// Where a stub that may keep `zmm0`-`zmm31` keeps the mask registers.
const MASKS: u32 = AREA + 32 * ZMM;
/// The AVX state component: the upper halves of `ymm0`-`ymm15`.
const AVX_STATE: u8 = 0b100;
/// The AVX-512 state components: the mask registers, the upper halves of
/// `zmm0`-`zmm15`, and `zmm16`-`zmm31`.
const ZMM_STATE: u8 = 0b1110_0000;

/// How a stub keeps the vector registers of the hooked code across the
/// handlers, chosen per processor when the stubs are written.
///
/// At an instruction inside a function any vector register may hold a live
/// value. The calling convention keeps only the low halves of `xmm6`-`xmm15`
/// across the handler call, AVX code in a handler or in the runtime it calls
/// ends with `vzeroupper`, which clears the upper halves of `zmm0`-`zmm15`,
/// and AVX-512 code may change any `zmm` and mask register. A register in its
/// initial state holds zero, so a stub that finds a part zero may give it back
/// as zero rather than load it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Vectors {
    /// `xmm0`-`xmm5`: a processor or system without AVX, where the calling
    /// convention keeps the rest.
    Sse,
    /// `ymm0`-`ymm15` every time: AVX without AVX-512 on a processor whose
    /// SSE code runs at full speed with the upper halves in use.
    Ymm,
    /// `ymm0`-`ymm15` every time, given back with `vzeroupper` and `xmm`
    /// loads when every upper half is zero: AVX without AVX-512 on a
    /// processor whose SSE code slows down while the upper halves are in
    /// use, so the state stays clear for the code after the hook.
    YmmClean,
    /// By value: an OR of every vector and mask register picks `xmm0`-`xmm5`,
    /// `ymm0`-`ymm15`, or all of `zmm0`-`zmm31` and the mask registers. For
    /// AVX-512 on a processor where 512-bit instructions cost no clock speed.
    ZmmByValue {
        /// The width of the mask registers.
        masks: Masks,
    },
    /// By the state in use, read with `xgetbv`: the same three paths for
    /// AVX-512 on a processor where a 512-bit instruction lowers the clock.
    ZmmInUse {
        /// The width of the mask registers.
        masks: Masks,
    },
}

/// The width of the AVX-512 mask registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Masks {
    /// 16 bits: AVX-512 without byte and word instructions.
    Word,
    /// 64 bits.
    Quad,
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
        const XGETBV_IN_USE: u32 = 1 << 2;
        const AVX512BW: u32 = 1 << 30;
        if __cpuid(1).ecx & OSXSAVE == 0 {
            return Self::Sse;
        }
        // SAFETY: CPUID's OSXSAVE bit, checked by the guard before it, says the
        // system turned `xgetbv` on, so the instruction does not fault.
        let enabled = unsafe { _xgetbv(0) };
        if enabled & u64::from(AVX_STATE) == 0 {
            return Self::Sse;
        }
        // AMD's processors, and Hygon's built on them, run SSE code at full
        // speed with the upper halves in use and keep their clock on 512-bit
        // instructions. Intel's slow SSE code down while the upper halves
        // are in use, and their first AVX-512 processors lower the clock for
        // any 512-bit instruction (Intel's optimization reference manual).
        let vendor = __cpuid(0);
        let mut name = [0; 12];
        name[..4].copy_from_slice(&vendor.ebx.to_le_bytes());
        name[4..8].copy_from_slice(&vendor.edx.to_le_bytes());
        name[8..].copy_from_slice(&vendor.ecx.to_le_bytes());
        let amd = &name == b"AuthenticAMD" || &name == b"HygonGenuine";
        if enabled & u64::from(ZMM_STATE) != u64::from(ZMM_STATE) {
            return if amd { Self::Ymm } else { Self::YmmClean };
        }
        let masks = if __cpuid_count(7, 0).ebx & AVX512BW == 0 {
            Masks::Word
        } else {
            Masks::Quad
        };
        if !amd && __cpuid_count(0xd, 1).eax & XGETBV_IN_USE != 0 {
            Self::ZmmInUse { masks }
        } else {
            Self::ZmmByValue { masks }
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
///
/// # Safety
///
/// - `registers` points at the registers the calling stub saved, valid and
///   unaliased for the call.
/// - `site` is null or a `&'static Site` from [`Mids::sites`] whose
///   provenance was exposed, as [`Stubs::dispatching`] does.
pub(crate) unsafe extern "system" fn dispatch(registers: *mut Registers, site: *const Site) {
    // The stub's machine code passes the address `Stubs::dispatching`
    // exposed; the pointer takes back that provenance.
    let site = core::ptr::with_exposed_provenance::<Site>(site.addr());
    // SAFETY: `site` is null or a `&'static Site` with its exposed provenance
    // by this function's `# Safety`; `as_ref` maps null to `None`.
    let Some(site) = (unsafe { site.as_ref() }) else {
        return;
    };
    for link in &site.links {
        if !(site.off)(link.owner) {
            let user = core::ptr::with_exposed_provenance_mut::<c_void>(link.user);
            // SAFETY: `site.off` says the mod is on, so its DLL and the
            // handler in it are loaded; the handler gets the pointer its mod
            // registered and the stub's saved registers, valid by this
            // function's `# Safety`; an `extern "system"` function aborts
            // rather than unwinds.
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
        // SAFETY: `VirtualAlloc` with a null address reserves fresh pages and
        // touches no memory of ours; null on failure, checked below.
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
            // SAFETY: `i < sites.len()` and `len` is `sites.len() * STUB` at
            // least, so `i * STUB` stays inside the allocation at `base`.
            let at = unsafe { base.add(i * STUB) };
            // SAFETY: `at` starts stub `i`, whose `STUB` bytes lie inside the
            // `len` read-write bytes at `base`; `code` is a fresh `STUB`-byte
            // array, so the ranges do not overlap.
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
        let callback = (dispatch as *const ()).expose_provenance();
        let pairs: Vec<(usize, usize)> = sites
            .iter()
            .map(|&site| (callback, core::ptr::from_ref(site).expose_provenance()))
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
        // SAFETY: `(i + 1) * STUB <= self.len`, checked by the guard before it,
        // and `CONTINUATION` lies inside a stub, so the offset stays inside the
        // allocation at `self.base`.
        let at = unsafe { self.base.add(i * STUB + CONTINUATION) };
        // SAFETY: `at` is stub `i`'s last 8 code bytes, inside the allocation,
        // read-write until `seal`, and no thread runs the stub before
        // `commit` jumps to it.
        unsafe { at.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
    }

    /// Makes the stubs executable and no longer writable; false when they
    /// could not be.
    pub(crate) fn seal(&self) -> bool {
        // SAFETY: `self.base` and `self.len` are the pages `write` allocated
        // for this batch and filled; no thread runs the stubs before the
        // batch arms them.
        unsafe { crate::pages::seal(self.base.expose_provenance(), self.len) }
    }
}

/// Machine code with forward `rel32` jumps.
#[derive(Default)]
struct Code {
    bytes: Vec<u8>,
    /// Where `rel32` operands point at [`UPPER`].
    uppers: Vec<usize>,
}

/// The direction of a move to memory, for [`Code::xmm`], [`Code::ymm`],
/// [`Code::zmm`] and [`Code::mask`].
const STORE: u8 = 1;
/// The direction of a move from memory.
const LOAD: u8 = 0;

impl Code {
    fn emit(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// `opcode` with a `rel32` bound later by [`bind`](Self::bind).
    fn jump(&mut self, opcode: &[u8]) -> usize {
        self.emit(opcode);
        let at = self.bytes.len();
        self.emit(&[0; 4]);
        at
    }

    /// Points the jumps at `sites` here.
    fn bind(&mut self, sites: &[usize]) {
        let here = self.bytes.len();
        for &site in sites {
            let distance = u32::try_from(here - (site + 4)).unwrap_or(0);
            self.bytes[site..site + 4].copy_from_slice(&distance.to_le_bytes());
        }
    }

    /// `opcode` with register `reg` and the operand `[rsp + displacement]`.
    fn stack(&mut self, opcode: &[u8], reg: u8, displacement: u32) {
        self.emit(opcode);
        self.emit(&[0x84 | (reg & 7) << 3, 0x24]);
        self.emit(&displacement.to_le_bytes());
    }

    /// `movaps` of `xmm<reg>` to or from the stack.
    fn xmm(&mut self, direction: u8, reg: u8, displacement: u32) {
        if reg >= 8 {
            self.emit(&[0x44]);
        }
        self.stack(&[0x0f, 0x28 | direction], reg, displacement);
    }

    /// `vmovdqu` of `ymm<reg>` to or from the stack.
    fn ymm(&mut self, direction: u8, reg: u8, displacement: u32) {
        let vex = if reg < 8 { 0xfe } else { 0x7e };
        self.stack(&[0xc5, vex, 0x6f | direction << 4], reg, displacement);
    }

    /// `vmovdqu64` of `zmm<reg>` to or from the stack.
    fn zmm(&mut self, direction: u8, reg: u8, displacement: u32) {
        let p0 = u8::from(reg & 8 == 0) << 7 | 0x60 | u8::from(reg & 16 == 0) << 4 | 1;
        self.stack(
            &[0x62, p0, 0xfe, 0x48, 0x6f | direction << 4],
            reg,
            displacement,
        );
    }

    /// `kmovq` or `kmovw` of `k<reg>` to or from the stack.
    fn mask(&mut self, masks: Masks, direction: u8, reg: u8, displacement: u32) {
        let opcode = 0x90 | direction;
        match masks {
            Masks::Quad => self.stack(&[0xc4, 0xe1, 0xf8, opcode], reg, displacement),
            Masks::Word => self.stack(&[0xc5, 0xf8, opcode], reg, displacement),
        }
    }

    /// A VEX instruction on registers `[reg, vvvv, rm]` (0 for an unused
    /// `vvvv`), in opcode map `map` with prefix `pp`, 256 bits wide if `wide`.
    fn vex(&mut self, map: u8, pp: u8, wide: bool, opcode: u8, regs: [u8; 3]) {
        let [reg, vvvv, rm] = regs;
        let r = u8::from(reg & 8 == 0) << 7;
        let rest = (!vvvv & 15) << 3 | u8::from(wide) << 2 | pp;
        if map == 1 && rm < 8 {
            self.emit(&[0xc5, r | rest]);
        } else {
            self.emit(&[0xc4, r | 0x40 | u8::from(rm & 8 == 0) << 5 | map, rest]);
        }
        self.emit(&[opcode, 0xc0 | (reg & 7) << 3 | (rm & 7)]);
    }

    /// An EVEX instruction on registers `[reg, vvvv, rm]` (0 for an unused
    /// `vvvv`), in opcode map `map` with prefix `pp`, 128 << `width` bits
    /// wide.
    fn evex(&mut self, map: u8, pp: u8, w: bool, width: u8, opcode: u8, regs: [u8; 3]) {
        let [reg, vvvv, rm] = regs;
        let p0 = u8::from(reg & 8 == 0) << 7
            | u8::from(rm & 16 == 0) << 6
            | u8::from(rm & 8 == 0) << 5
            | u8::from(reg & 16 == 0) << 4
            | map;
        let p1 = u8::from(w) << 7 | (!vvvv & 15) << 3 | 4 | pp;
        let p2 = width << 5 | u8::from(vvvv & 16 == 0) << 3;
        self.emit(&[0x62, p0, p1, p2, opcode, 0xc0 | (reg & 7) << 3 | (rm & 7)]);
    }

    /// `vpor ymm<a>, ymm<a>, ymm<b>`, with the sources swapped where that
    /// fits the shorter VEX form.
    fn or_ymm(&mut self, a: u8, b: u8) {
        let sources = if a < 8 && b >= 8 { [b, a] } else { [a, b] };
        self.vex(1, 1, true, 0xeb, [a, sources[0], sources[1]]);
    }

    /// `vpord zmm<a>, zmm<b>, zmm<c>`
    fn or_zmm(&mut self, a: u8, b: u8, c: u8) {
        self.evex(1, 1, false, 2, 0xeb, [a, b, c]);
    }

    /// `vpternlogq zmm<a>, zmm<b>, zmm<c>, 0xfe`: `a | b | c` into `a`.
    fn or3_zmm(&mut self, a: u8, b: u8, c: u8) {
        self.evex(3, 1, true, 2, 0x25, [a, b, c]);
        self.emit(&[0xfe]);
    }

    /// `vextracti64x4 ymm<to>, zmm<from>, 1`
    fn upper_zmm(&mut self, to: u8, from: u8) {
        self.evex(3, 1, true, 2, 0x3b, [from, 0, to]);
        self.emit(&[1]);
    }

    /// `vptest ymm0, ymm0`
    fn test_ymm0(&mut self) {
        self.vex(2, 1, true, 0x17, [0, 0, 0]);
    }

    /// `vptest ymm0, [rip + UPPER]`: whether the upper half of `ymm0` is
    /// zero.
    fn test_upper_ymm0(&mut self) {
        let at = self.jump(&[0xc4, 0xe2, 0x7d, 0x17, 0x05]);
        self.uppers.push(at);
    }

    /// `kortestq` or `kortestw` of `k<a>` and `k<b>`.
    fn test_masks(&mut self, masks: Masks, a: u8, b: u8) {
        let modrm = 0xc0 | a << 3 | b;
        match masks {
            Masks::Quad => self.emit(&[0xc4, 0xe1, 0xf8, 0x98, modrm]),
            Masks::Word => self.emit(&[0xc5, 0xf8, 0x98, modrm]),
        }
    }

    /// Zero in `zmm16`-`zmm31` and the mask registers.
    fn zero_avx512(&mut self, masks: Masks) {
        for n in 16..32 {
            // vpxord xmm<n>, xmm<n>, xmm<n>
            self.evex(1, 1, false, 0, 0xef, [n, n, n]);
        }
        for k in 0..8 {
            // kxorq or kxorw k<k>, k<k>, k<k>
            let rest = (!k & 15) << 3 | 4;
            let modrm = 0xc0 | k << 3 | k;
            match masks {
                Masks::Quad => self.emit(&[0xc4, 0xe1, 0x80 | rest, 0x47, modrm]),
                Masks::Word => self.emit(&[0xc5, 0x80 | rest, 0x47, modrm]),
            }
        }
    }

    /// `vzeroupper`
    fn vzeroupper(&mut self) {
        self.emit(&[0xc5, 0xf8, 0x77]);
    }

    /// `test r12b, bits; jnz`: to the path the stub chose before.
    fn branch(&mut self, bits: u8) -> usize {
        self.emit(&[0x41, 0xf6, 0xc4, bits]);
        self.jump(&[0x0f, 0x85])
    }

    /// `jnz`
    fn jnz(&mut self) -> usize {
        self.jump(&[0x0f, 0x85])
    }

    /// `jmp`
    fn jmp(&mut self) -> usize {
        self.jump(&[0xe9])
    }

    /// `mov r12d, path`: the path the stub chose, for after the handlers.
    fn choose(&mut self, path: u8) {
        self.emit(&[0x41, 0xbc, path, 0, 0, 0]);
    }

    /// `xor r12d, r12d`
    fn clear_path(&mut self) {
        self.emit(&[0x45, 0x31, 0xe4]);
    }

    /// `setnz r12b`
    fn path_if_nonzero(&mut self) {
        self.emit(&[0x41, 0x0f, 0x95, 0xc4]);
    }

    /// `and rsp, -64; sub rsp, room`
    fn frame(&mut self, room: u32) {
        self.emit(&[0x48, 0x83, 0xe4, 0xc0, 0x48, 0x81, 0xec]);
        self.emit(&room.to_le_bytes());
    }
}

/// The code of one stub; the continuation is written by [`Stubs::finish`].
fn stub(callback: usize, context: usize, vectors: Vectors) -> [u8; STUB] {
    const MOVAPS: [u8; 6] = [0x44, 0x4c, 0x54, 0x5c, 0x64, 0x6c];
    let mut code = Code::default();
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
        Vectors::Ymm => keep_ymm(&mut code, call),
        Vectors::YmmClean => keep_ymm_clean(&mut code, call),
        Vectors::ZmmByValue { masks } => keep_zmm_by_value(&mut code, masks, call),
        Vectors::ZmmInUse { masks } => keep_zmm_in_use(&mut code, masks, call),
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
    debug_assert!(code.bytes.len() <= UPPER_AT);
    let mut bytes = [0xcc; STUB];
    let len = code.bytes.len().min(UPPER_AT);
    bytes[..len].copy_from_slice(&code.bytes[..len]);
    let relative = |site: usize, to: usize| {
        u32::try_from(to.saturating_sub(site + 4))
            .unwrap_or(0)
            .to_le_bytes()
    };
    for (site, to) in core::iter::once((jump, CONTINUATION))
        .chain(code.uppers.iter().map(|&site| (site, UPPER_AT)))
        .filter(|&(site, _)| site < len)
    {
        bytes[site..site + 4].copy_from_slice(&relative(site, to));
    }
    bytes[UPPER_AT..UPPER_AT + UPPER.len()].copy_from_slice(&UPPER);
    bytes
}

/// Where register `n` lies with `stride` bytes per register.
fn slot(n: u8, stride: u32) -> u32 {
    AREA + stride * u32::from(n)
}

/// Where mask register `k` lies.
fn mask_slot(k: u8) -> u32 {
    MASKS + 8 * u32::from(k)
}

/// [`Vectors::Ymm`].
fn keep_ymm(code: &mut Code, call: impl Fn(&mut Code)) {
    code.frame(slot(16, YMM));
    for n in 0..16 {
        code.ymm(STORE, n, slot(n, YMM));
    }
    call(code);
    for n in 0..16 {
        code.ymm(LOAD, n, slot(n, YMM));
    }
}

/// [`Vectors::YmmClean`].
fn keep_ymm_clean(code: &mut Code, call: impl Fn(&mut Code)) {
    /// The registers the OR below writes that the handler keeps.
    const REDUCED: [u8; 5] = [6, 8, 10, 12, 14];
    code.frame(slot(16, YMM));
    code.clear_path();
    for n in 0..16 {
        code.ymm(STORE, n, slot(n, YMM));
    }
    for step in [1, 2, 4, 8] {
        for a in (0..16).step_by(2 * usize::from(step)) {
            code.or_ymm(a, a + step);
        }
    }
    code.test_upper_ymm0();
    code.path_if_nonzero();
    code.vzeroupper();
    call(code);
    let ymm = code.branch(KEEP_YMM);
    code.vzeroupper();
    for n in (0..6).chain(REDUCED) {
        code.xmm(LOAD, n, slot(n, YMM));
    }
    let end = code.jmp();
    code.bind(&[ymm]);
    for n in 0..16 {
        code.ymm(LOAD, n, slot(n, YMM));
    }
    code.bind(&[end]);
}

/// The path that keeps `ymm0`-`ymm15`, in `r12d` across the handlers.
const KEEP_YMM: u8 = 1;
/// The path that keeps `zmm0`-`zmm31` and the mask registers.
const KEEP_ZMM: u8 = 2;

/// [`Vectors::ZmmByValue`].
fn keep_zmm_by_value(code: &mut Code, masks: Masks, call: impl Fn(&mut Code)) {
    code.frame(mask_slot(8));
    code.zmm(STORE, 0, slot(0, ZMM));
    code.zmm(STORE, 1, slot(1, ZMM));
    // zmm0: zmm0 | .. | zmm15; zmm1: zmm16 | .. | zmm31.
    code.or3_zmm(0, 1, 2);
    code.or_zmm(1, 16, 17);
    for n in (3..15).step_by(2) {
        code.or3_zmm(0, n, n + 1);
        code.or3_zmm(1, n + 15, n + 16);
    }
    code.or3_zmm(1, 30, 31);
    code.or_zmm(0, 0, 15);
    // Whether an upper half of ymm0-ymm15 is in use, then whether an upper
    // half of zmm0-zmm15, zmm16-zmm31 or a mask register is.
    code.clear_path();
    code.test_upper_ymm0();
    code.path_if_nonzero();
    code.upper_zmm(0, 0);
    code.or_zmm(1, 1, 0);
    code.upper_zmm(0, 1);
    code.or_ymm(0, 1);
    code.test_ymm0();
    let mut wide = vec![code.jnz()];
    for k in (0..8).step_by(2) {
        code.test_masks(masks, k, k + 1);
        wide.push(code.jnz());
    }
    let ymm = code.branch(KEEP_YMM);
    for n in 2..6 {
        code.xmm(STORE, n, slot(n, ZMM));
    }
    let mut calls = vec![code.jmp()];
    code.bind(&[ymm]);
    for n in 2..16 {
        code.ymm(STORE, n, slot(n, ZMM));
    }
    calls.push(code.jmp());
    code.bind(&wide);
    code.choose(KEEP_ZMM);
    for n in 2..32 {
        code.zmm(STORE, n, slot(n, ZMM));
    }
    for k in 0..8 {
        code.mask(masks, STORE, k, mask_slot(k));
    }
    code.bind(&calls);
    code.vzeroupper();
    call(code);
    give_back_zmm(code, masks);
}

/// [`Vectors::ZmmInUse`].
fn keep_zmm_in_use(code: &mut Code, masks: Masks, call: impl Fn(&mut Code)) {
    code.frame(mask_slot(8));
    // mov ecx, 1; xgetbv; mov r12d, eax: the state in use.
    code.emit(&[0xb9, 1, 0, 0, 0, 0x0f, 0x01, 0xd0, 0x41, 0x89, 0xc4]);
    let wide = code.branch(ZMM_STATE);
    let ymm = code.branch(AVX_STATE);
    code.clear_path();
    for n in 0..6 {
        code.xmm(STORE, n, slot(n, ZMM));
    }
    let mut calls = vec![code.jmp()];
    code.bind(&[ymm]);
    code.choose(KEEP_YMM);
    for n in 0..16 {
        code.ymm(STORE, n, slot(n, ZMM));
    }
    calls.push(code.jmp());
    code.bind(&[wide]);
    code.choose(KEEP_ZMM);
    for n in 0..32 {
        code.zmm(STORE, n, slot(n, ZMM));
    }
    for k in 0..8 {
        code.mask(masks, STORE, k, mask_slot(k));
    }
    code.bind(&calls);
    code.vzeroupper();
    call(code);
    give_back_zmm(code, masks);
}

/// Loads back what a `zmm` stub kept by the path in `r12d`; a part it did not
/// keep was zero, so it is given back as zero.
fn give_back_zmm(code: &mut Code, masks: Masks) {
    let wide = code.branch(KEEP_ZMM);
    let ymm = code.branch(KEEP_YMM);
    code.vzeroupper();
    for n in 0..6 {
        code.xmm(LOAD, n, slot(n, ZMM));
    }
    let zero = code.jmp();
    code.bind(&[ymm]);
    for n in 0..16 {
        code.ymm(LOAD, n, slot(n, ZMM));
    }
    code.bind(&[zero]);
    code.zero_avx512(masks);
    let end = code.jmp();
    code.bind(&[wide]);
    for n in 0..32 {
        code.zmm(LOAD, n, slot(n, ZMM));
    }
    for k in 0..8 {
        code.mask(masks, LOAD, k, mask_slot(k));
    }
    code.bind(&[end]);
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
        // SAFETY: `VirtualAlloc` with a null address reserves a fresh page
        // and touches no memory of ours; null on failure, asserted below.
        let page = unsafe {
            VirtualAlloc(
                core::ptr::null(),
                4096,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_EXECUTE_READWRITE,
            )
        };
        assert!(!page.is_null());
        // SAFETY: `page` is non-null, asserted after the allocation, and holds
        // 4096 writable bytes; every test's `code` is shorter, and the fresh
        // page does not overlap it.
        unsafe {
            page.cast::<u8>()
                .copy_from_nonoverlapping(code.as_ptr(), code.len());
        }
        page.addr()
    }

    fn callable(address: usize) -> Unary {
        // SAFETY: every test passes the address of code it wrote with
        // `function`, which takes a u32 in ecx and returns one in eax.
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
        .unwrap()
        .map_err(|e| e.to_string())?;
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
        // SAFETY: the stub passes a pointer to the registers it saved on its
        // stack, valid and unaliased until the handler returns.
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
        // SAFETY: the stub passes a pointer to the registers it saved on its
        // stack, valid and unaliased until the handler returns.
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
        // SAFETY: the assembly writes only `xmm0`, `xmm5` and `eax`, all
        // declared as outputs, and touches no memory.
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

    /// Bytes copied from a test function: its code and the `int3` after it.
    const BODY: usize = 128;

    /// A fresh copy of `function`, which must not refer to anything by its
    /// own address, so that each stub hooks a function of its own.
    fn copy(function: Unary) -> usize {
        let start = (function as *const ()).addr();
        // SAFETY: every `function` passed here is a naked test function of
        // this module, shorter than `BODY - 64` bytes and followed by 64
        // bytes of `int3`, so `BODY` bytes of mapped code start at `start`.
        let code = unsafe { core::slice::from_raw_parts(start as *const u8, BODY) };
        self::function(code)
    }

    /// Hooks the first [`MARK`] in a copy of `function` with `handler` and
    /// returns the copy.
    fn hook_mark(function: Unary, handler: usize, vectors: Vectors) -> Unary {
        let target = copy(function);
        // SAFETY: `copy` wrote `BODY` bytes into a fresh page of 4096 at
        // `target`, which no thread writes now.
        let code = unsafe { core::slice::from_raw_parts(target as *const u8, BODY) };
        let at = code.windows(MARK.len()).position(|w| w == MARK).unwrap();
        install_with(target + at, handler, 0, vectors).unwrap();
        callable(target)
    }

    /// The AVX-512 mask width, where the system turns AVX-512 on.
    fn masks() -> Option<Masks> {
        match Vectors::current() {
            Vectors::ZmmByValue { masks } | Vectors::ZmmInUse { masks } => Some(masks),
            _ => None,
        }
    }

    /// Whether `xgetbv(1)` reads the state in use.
    fn xgetbv_in_use() -> bool {
        Vectors::current() != Vectors::Sse
            && core::arch::x86_64::__cpuid_count(0xd, 1).eax & (1 << 2) != 0
    }

    /// Every way of keeping the vector state this processor can run, by
    /// name: what is chosen for other processors runs here too.
    fn kinds() -> Vec<(&'static str, Vectors)> {
        let mut kinds = vec![("sse", Vectors::Sse)];
        if Vectors::current() == Vectors::Sse {
            return kinds;
        }
        kinds.extend([("ymm", Vectors::Ymm), ("ymm clean", Vectors::YmmClean)]);
        if let Some(masks) = masks() {
            kinds.push(("zmm by value", Vectors::ZmmByValue { masks }));
            if xgetbv_in_use() {
                kinds.push(("zmm in use", Vectors::ZmmInUse { masks }));
            }
        }
        kinds
    }

    /// Every way of keeping the vector state but [`Vectors::Sse`].
    fn avx_kinds() -> Vec<(&'static str, Vectors)> {
        kinds().into_iter().skip(1).collect()
    }

    /// Holds `value` in the upper half of `ymm4` across the marked `nop`,
    /// then returns it.
    #[unsafe(naked)]
    extern "system" fn upper_ymm(value: u32) -> u32 {
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

    /// What AVX code in a handler leaves behind: every upper half cleared.
    extern "system" fn clear_upper_halves(_registers: *mut Registers, _context: usize) {
        // SAFETY: the assembly writes `xmm4`, declared as an output, and the
        // upper halves of the vector registers, which no code of this crate
        // keeps live; it touches no memory.
        unsafe {
            core::arch::asm!("vxorps ymm4, ymm4, ymm4", "vzeroupper", out("xmm4") _);
        }
    }

    #[test]
    fn upper_halves_of_vector_registers_survive_a_handler_using_avx() {
        let handler = (clear_upper_halves as *const ()).addr();
        for (name, vectors) in avx_kinds() {
            let hooked = hook_mark(upper_ymm, handler, vectors);
            assert_eq!(hooked(0x1234_5678), 0x1234_5678, "{name}");
        }
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
        // SAFETY: the assembly writes `xmm4`, declared as an output, and its
        // upper half, which no code of this crate keeps live; it touches no
        // memory.
        unsafe {
            core::arch::asm!("vcmptrueps ymm4, ymm4, ymm4", out("xmm4") _);
        }
    }

    #[test]
    fn clear_upper_halves_stay_clear_after_a_handler_using_avx() {
        if !xgetbv_in_use() {
            return;
        }
        let handler = (fill_upper_halves as *const ()).addr();
        for (name, vectors) in avx_kinds() {
            if vectors == Vectors::Ymm {
                continue;
            }
            let hooked = hook_mark(upper_in_use, handler, vectors);
            assert_eq!(hooked(0), 0, "{name}");
        }
    }

    /// Holds `value` in `zmm16`, in the top quarter of `zmm1` and in `k1`
    /// across the marked `nop`, then returns their sum.
    #[unsafe(naked)]
    extern "system" fn wide_zmm(value: u32) -> u32 {
        core::arch::naked_asm!(
            "vpbroadcastd zmm16, ecx",
            "vpbroadcastd zmm1, ecx",
            "kmovw k1, ecx",
            ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
            "vextracti32x4 xmm2, zmm16, 3",
            "vmovd eax, xmm2",
            "vextracti32x4 xmm3, zmm1, 3",
            "vmovd edx, xmm3",
            "add eax, edx",
            "kmovw edx, k1",
            "add eax, edx",
            "vzeroupper",
            "ret",
            ".fill 64, 1, 0xcc",
        )
    }

    /// Holds `value` in the top quarter of `zmm31` alone across the marked
    /// `nop`, the wide registers other tests touch clear, then returns it.
    #[unsafe(naked)]
    extern "system" fn zmm31_only(value: u32) -> u32 {
        core::arch::naked_asm!(
            "vzeroupper",
            "vpxord xmm16, xmm16, xmm16",
            "kxorw k1, k1, k1",
            "kxorw k2, k2, k2",
            "kxorw k3, k3, k3",
            "kxorw k4, k4, k4",
            "kxorw k5, k5, k5",
            "kxorw k6, k6, k6",
            "kxorw k7, k7, k7",
            "vpbroadcastd zmm31, ecx",
            ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
            "vextracti32x4 xmm2, zmm31, 3",
            "vmovd eax, xmm2",
            "vzeroupper",
            "ret",
            ".fill 64, 1, 0xcc",
        )
    }

    extern "system" fn clear_zmm31(_registers: *mut Registers, _context: usize) {
        // SAFETY: the assembly writes only `zmm31`, which no code of this
        // crate keeps live, and touches no memory.
        unsafe {
            core::arch::asm!("vpxord zmm31, zmm31, zmm31");
        }
    }

    /// Holds `value` in `k1` alone across the marked `nop`, then returns it.
    #[unsafe(naked)]
    extern "system" fn mask_only(value: u32) -> u32 {
        core::arch::naked_asm!(
            "vzeroupper",
            "kmovw k1, ecx",
            ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
            "kmovw eax, k1",
            "ret",
            ".fill 64, 1, 0xcc",
        )
    }

    extern "system" fn clear_wide_registers(_registers: *mut Registers, _context: usize) {
        // SAFETY: the assembly writes `xmm1`, declared as an output, and
        // `zmm16`, `k1` and the upper halves, which no code of this crate
        // keeps live; it touches no memory.
        unsafe {
            core::arch::asm!(
                "vpxord zmm16, zmm16, zmm16",
                "vxorps ymm1, ymm1, ymm1",
                "kxorw k1, k1, k1",
                "vzeroupper",
                out("xmm1") _,
            );
        }
    }

    #[test]
    fn avx512_registers_survive_where_the_system_turns_them_on() {
        let handler = (clear_wide_registers as *const ()).addr();
        for (name, vectors) in avx_kinds() {
            if !matches!(
                vectors,
                Vectors::ZmmByValue { .. } | Vectors::ZmmInUse { .. }
            ) {
                continue;
            }
            assert_eq!(
                hook_mark(zmm31_only, (clear_zmm31 as *const ()).addr(), vectors)(77),
                77,
                "{name}"
            );
            assert_eq!(hook_mark(wide_zmm, handler, vectors)(21), 63, "{name}");
            assert_eq!(
                hook_mark(mask_only, handler, vectors)(0x4321),
                0x4321,
                "{name}"
            );
        }
    }

    /// Whether `zmm16` or `k2` is anything but zero after the marked `nop`,
    /// with both zero before it.
    #[unsafe(naked)]
    extern "system" fn wide_zero(value: u32) -> u32 {
        core::arch::naked_asm!(
            "vpxord xmm16, xmm16, xmm16",
            "kxorw k2, k2, k2",
            ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
            "vmovd eax, xmm16",
            "kmovw edx, k2",
            "or eax, edx",
            "ret",
            ".fill 64, 1, 0xcc",
        )
    }

    extern "system" fn fill_wide_registers(_registers: *mut Registers, _context: usize) {
        // SAFETY: the assembly writes only `zmm16` and `k2`, which no code
        // of this crate keeps live, and touches no memory.
        unsafe {
            core::arch::asm!("vpternlogd zmm16, zmm16, zmm16, 0xff", "kxnorw k2, k2, k2");
        }
    }

    #[test]
    fn zero_avx512_registers_stay_zero_after_a_handler_using_them() {
        let handler = (fill_wide_registers as *const ()).addr();
        for (name, vectors) in avx_kinds() {
            if !matches!(
                vectors,
                Vectors::ZmmByValue { .. } | Vectors::ZmmInUse { .. }
            ) {
                continue;
            }
            assert_eq!(hook_mark(wide_zero, handler, vectors)(0), 0, "{name}");
        }
    }

    /// The instructions [`Code`] writes, as the assembler writes them.
    #[unsafe(naked)]
    extern "system" fn encodings() {
        core::arch::naked_asm!(
            "movaps [rsp + 0x12345], xmm3",
            "movaps xmm9, [rsp + 0x12345]",
            "vmovdqu [rsp + 0x12345], ymm2",
            "vmovdqu ymm13, [rsp + 0x12345]",
            "vmovdqu64 [rsp + 0x12345], zmm0",
            "vmovdqu64 [rsp + 0x12345], zmm9",
            "vmovdqu64 zmm17, [rsp + 0x12345]",
            "vmovdqu64 zmm30, [rsp + 0x12345]",
            "kmovq [rsp + 0x12345], k3",
            "kmovq k7, [rsp + 0x12345]",
            "kmovw [rsp + 0x12345], k1",
            "kmovw k6, [rsp + 0x12345]",
            "vpor ymm0, ymm0, ymm1",
            "vpor ymm8, ymm8, ymm12",
            "vpor ymm6, ymm6, ymm14",
            "vpord zmm1, zmm16, zmm17",
            "vpord zmm0, zmm0, zmm15",
            "vpternlogq zmm0, zmm1, zmm2, 0xfe",
            "vpternlogq zmm1, zmm30, zmm31, 0xfe",
            "vpternlogq zmm0, zmm13, zmm14, 0xfe",
            "vextracti64x4 ymm0, zmm1, 1",
            "vptest ymm0, ymm0",
            "kortestq k0, k1",
            "kortestw k6, k7",
            "vpxord xmm16, xmm16, xmm16",
            "vpxord xmm27, xmm27, xmm27",
            "kxorq k5, k5, k5",
            "kxorw k0, k0, k0",
            "int3",
        )
    }

    #[test]
    fn the_stub_writes_instructions_as_the_assembler_does() {
        const AT: u32 = 0x12345;
        let mut code = Code::default();
        code.xmm(STORE, 3, AT);
        code.xmm(LOAD, 9, AT);
        code.ymm(STORE, 2, AT);
        code.ymm(LOAD, 13, AT);
        code.zmm(STORE, 0, AT);
        code.zmm(STORE, 9, AT);
        code.zmm(LOAD, 17, AT);
        code.zmm(LOAD, 30, AT);
        code.mask(Masks::Quad, STORE, 3, AT);
        code.mask(Masks::Quad, LOAD, 7, AT);
        code.mask(Masks::Word, STORE, 1, AT);
        code.mask(Masks::Word, LOAD, 6, AT);
        code.or_ymm(0, 1);
        code.or_ymm(8, 12);
        code.or_ymm(6, 14);
        code.or_zmm(1, 16, 17);
        code.or_zmm(0, 0, 15);
        code.or3_zmm(0, 1, 2);
        code.or3_zmm(1, 30, 31);
        code.or3_zmm(0, 13, 14);
        code.upper_zmm(0, 1);
        code.test_ymm0();
        code.test_masks(Masks::Quad, 0, 1);
        code.test_masks(Masks::Word, 6, 7);
        let zero = code.bytes.len();
        let mut zeroing = Code::default();
        zeroing.zero_avx512(Masks::Quad);
        // vpxord of xmm16 and xmm27, kxorq of k5.
        code.emit(&zeroing.bytes[..6]);
        code.emit(&zeroing.bytes[11 * 6..12 * 6]);
        code.emit(&zeroing.bytes[16 * 6 + 5 * 5..16 * 6 + 6 * 5]);
        let mut words = Code::default();
        words.zero_avx512(Masks::Word);
        code.emit(&words.bytes[16 * 6..16 * 6 + 4]);
        code.emit(&[0xcc]);
        let start = (encodings as *const ()).addr();
        // SAFETY: `encodings` is a naked function of this module whose code
        // is the instructions `code` encodes, so `code.bytes.len()` bytes of
        // mapped code start at `start`.
        let assembled =
            unsafe { core::slice::from_raw_parts(start as *const u8, code.bytes.len()) };
        assert_eq!(&assembled[..zero], &code.bytes[..zero]);
        assert_eq!(assembled, &code.bytes[..]);
    }

    #[test]
    fn every_stub_fits() {
        let masks = [Masks::Word, Masks::Quad];
        let kinds = [Vectors::Sse, Vectors::Ymm, Vectors::YmmClean]
            .into_iter()
            .chain(masks.map(|masks| Vectors::ZmmByValue { masks }))
            .chain(masks.map(|masks| Vectors::ZmmInUse { masks }));
        for vectors in kinds {
            let code = stub(usize::MAX, usize::MAX, vectors);
            let end = code[..UPPER_AT].iter().rposition(|&b| b != 0xcc).unwrap();
            assert!(end + 16 < UPPER_AT, "{vectors:?}");
            assert_eq!(code[UPPER_AT..UPPER_AT + 32], UPPER);
        }
    }

    extern "system" fn nothing(_registers: *mut Registers, _context: usize) {}

    /// `x + 6` with the upper halves clear across the marked `nop`.
    #[unsafe(naked)]
    extern "system" fn plain_clear(value: u32) -> u32 {
        core::arch::naked_asm!(
            "vzeroupper",
            ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
            "lea eax, [rcx + 6]",
            "ret",
            ".fill 64, 1, 0xcc",
        )
    }

    /// `x + 6` with the upper half of `ymm4` in use across the marked `nop`.
    #[unsafe(naked)]
    extern "system" fn plain_ymm(value: u32) -> u32 {
        core::arch::naked_asm!(
            "vpcmpeqd xmm4, xmm4, xmm4",
            "vinsertf128 ymm4, ymm4, xmm4, 1",
            ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
            "lea eax, [rcx + 6]",
            "vzeroupper",
            "ret",
            ".fill 64, 1, 0xcc",
        )
    }

    /// `x + 6` with the upper half of `zmm4` in use across the marked `nop`.
    #[unsafe(naked)]
    extern "system" fn plain_zmm(value: u32) -> u32 {
        core::arch::naked_asm!(
            "vpcmpeqd ymm4, ymm4, ymm4",
            "vinserti64x4 zmm4, zmm4, ymm4, 1",
            ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
            "lea eax, [rcx + 6]",
            "vzeroupper",
            "ret",
            ".fill 64, 1, 0xcc",
        )
    }

    /// Nanoseconds per call of `function`, the best of a few rounds.
    fn per_call(function: Unary) -> f64 {
        const CALLS: u32 = 2_000_000;
        (0..5)
            .map(|_| {
                let start = std::time::Instant::now();
                let mut sum = 0u32;
                for i in 0..CALLS {
                    sum = sum.wrapping_add(function(core::hint::black_box(i)));
                }
                core::hint::black_box(sum);
                start.elapsed().as_secs_f64() * 1e9 / f64::from(CALLS)
            })
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    #[ignore = "a measurement: cargo test -p enshwick-loader --release --lib stub_cost -- --ignored --nocapture"]
    fn stub_cost() {
        let callback = (nothing as *const ()).addr();
        println!("chosen here: {:?}", Vectors::current());
        let mut cases: Vec<(&str, Unary)> = vec![("clear", plain_clear)];
        if Vectors::current() != Vectors::Sse {
            cases.push(("ymm in use", plain_ymm));
        }
        if masks().is_some() {
            cases.push(("zmm in use", plain_zmm));
        }
        for (state, function) in cases {
            let bare = per_call(callable(copy(function)));
            println!("{state}, no hook: {bare:.2} ns");
            for (name, vectors) in kinds() {
                let hooked = per_call(hook_mark(function, callback, vectors));
                println!("{state}, {name}: {hooked:.2} ns (+{:.2})", hooked - bare);
            }
        }
    }

    static PANICS: AtomicU32 = AtomicU32::new(0);
    static FAULTED: AtomicBool = AtomicBool::new(false);

    /// What the API's wrapper does: work on a copy, fault on a panic.
    ///
    /// # Safety
    ///
    /// - `registers` points at the stub's saved registers, valid and
    ///   unaliased for the call.
    unsafe extern "system" fn panicking(registers: *mut Registers, _user: *mut c_void) {
        // SAFETY: `registers` is valid and unaliased for the call by this
        // function's `# Safety`, which `dispatch` keeps.
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

    /// Adds 100 to `rax`.
    ///
    /// # Safety
    ///
    /// - `registers` points at the stub's saved registers, valid and
    ///   unaliased for the call.
    unsafe extern "system" fn add_hundred(registers: *mut Registers, _user: *mut c_void) {
        // SAFETY: `registers` is valid and unaliased for the call by this
        // function's `# Safety`, which `dispatch` keeps.
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
            (dispatch as *const ()).expose_provenance(),
            core::ptr::from_ref(site).expose_provenance(),
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
