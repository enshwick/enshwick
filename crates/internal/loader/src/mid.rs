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

use crate::mod_index::ModIndex;

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
///
/// The state components, as `xgetbv` reads them in use, also name the
/// parts a `zmm` stub keeps, in `r12b` across the handlers.
const AVX_STATE: u8 = 0b100;
/// The mask registers.
const MASK_STATE: u8 = 0b10_0000;
/// The upper halves of `zmm0`-`zmm15`.
const ZMM_HI256: u8 = 0b100_0000;
/// `zmm16`-`zmm31`.
const HI16_ZMM: u8 = 0b1000_0000;
/// The AVX-512 state components.
const ZMM_STATE: u8 = MASK_STATE | ZMM_HI256 | HI16_ZMM;

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
    /// By value: ORs of the vector and mask registers pick, part by part,
    /// `xmm0`-`xmm5`, `ymm0`-`ymm15` or `zmm0`-`zmm15`, then `zmm16`-`zmm31`
    /// and the mask registers where they are not zero. For AVX-512 on a
    /// processor where 512-bit instructions cost no clock speed.
    ZmmByValue {
        /// The width of the mask registers.
        masks: Masks,
    },
    /// By the state in use, read with `xgetbv`: the same parts for AVX-512
    /// on a processor where a 512-bit instruction lowers the clock.
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
    pub(crate) owner: ModIndex,
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
    off: fn(ModIndex) -> bool,
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
            enshwick::__private::rules::hooks_clash(l.owner == link.owner, l.flags, link.flags)
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
    pub(crate) fn sites(&self, off: fn(ModIndex) -> bool) -> Vec<&'static Site> {
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
    pub(crate) fn owners(&self, address: u64) -> Vec<ModIndex> {
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
        if !(site.off)(ModIndex::new(link.owner.get())) {
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
        let base = crate::pages::allocate(None, len)?.as_ptr();
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

    /// `vpord zmm<a>, zmm<b>, zmm<c>`.
    fn or_zmm(&mut self, a: u8, b: u8, c: u8) {
        self.evex(1, 1, false, 2, 0xeb, [a, b, c]);
    }

    /// `vpternlogq zmm<a>, zmm<b>, zmm<c>, 0xfe`: `a | b | c` into `a`.
    fn or3_zmm(&mut self, a: u8, b: u8, c: u8) {
        self.evex(3, 1, true, 2, 0x25, [a, b, c]);
        self.emit(&[0xfe]);
    }

    /// `vextracti64x4 ymm<to>, zmm<from>, 1`.
    fn upper_zmm(&mut self, to: u8, from: u8) {
        self.evex(3, 1, true, 2, 0x3b, [from, 0, to]);
        self.emit(&[1]);
    }

    /// `vptest ymm0, ymm0`.
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

    /// Zero in `zmm16`-`zmm31`.
    fn zero_hi16(&mut self) {
        for n in 16..32 {
            // vpxord xmm<n>, xmm<n>, xmm<n>
            self.evex(1, 1, false, 0, 0xef, [n, n, n]);
        }
    }

    /// Zero in the mask registers.
    fn zero_masks(&mut self, masks: Masks) {
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

    /// `vzeroupper`.
    fn vzeroupper(&mut self) {
        self.emit(&[0xc5, 0xf8, 0x77]);
    }

    /// `test r12b, bits`.
    fn test_path(&mut self, bits: u8) {
        self.emit(&[0x41, 0xf6, 0xc4, bits]);
    }

    /// `test al, bits`.
    fn test_used(&mut self, bits: u8) {
        self.emit(&[0xa8, bits]);
    }

    /// `mov r12d, eax`.
    fn path_from_eax(&mut self) {
        self.emit(&[0x41, 0x89, 0xc4]);
    }

    /// `test r12b, bits; jnz`: to the path the stub chose before.
    fn branch(&mut self, bits: u8) -> usize {
        self.test_path(bits);
        self.jnz()
    }

    /// `test r12b, bits; jz`: past a part the stub did not keep.
    fn unless(&mut self, bits: u8) -> usize {
        self.test_path(bits);
        self.jz()
    }

    /// `test al, bits; jz`: past a part the handlers left unused.
    fn unless_used(&mut self, bits: u8) -> usize {
        self.test_used(bits);
        self.jz()
    }

    /// `or r12b, bits`: a part to keep.
    fn keep(&mut self, bits: u8) {
        self.emit(&[0x41, 0x80, 0xcc, bits]);
    }

    /// `mov ecx, 1; xgetbv`: the state in use, in `eax`.
    fn in_use(&mut self) {
        self.emit(&[0xb9, 1, 0, 0, 0, 0x0f, 0x01, 0xd0]);
    }

    /// `jnz`.
    fn jnz(&mut self) -> usize {
        self.jump(&[0x0f, 0x85])
    }

    /// `jz`.
    fn jz(&mut self) -> usize {
        self.jump(&[0x0f, 0x84])
    }

    /// `jmp`.
    fn jmp(&mut self) -> usize {
        self.jump(&[0xe9])
    }

    /// `xor r12d, r12d`.
    fn clear_path(&mut self) {
        self.emit(&[0x45, 0x31, 0xe4]);
    }

    /// `setnz r12b`.
    fn path_if_nonzero(&mut self) {
        self.emit(&[0x41, 0x0f, 0x95, 0xc4]);
    }

    /// `and rsp, -64; sub rsp, room`.
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
    // The parts that are not zero, as `xgetbv` names the parts in use: an
    // upper half of ymm0-ymm15, of zmm0-zmm15, zmm16-zmm31, a mask register.
    code.clear_path();
    code.test_upper_ymm0();
    let zero = code.jz();
    code.keep(AVX_STATE);
    code.bind(&[zero]);
    code.upper_zmm(0, 0);
    code.test_ymm0();
    let zero = code.jz();
    code.keep(ZMM_HI256);
    code.bind(&[zero]);
    code.upper_zmm(0, 1);
    code.or_ymm(0, 1);
    code.test_ymm0();
    let zero = code.jz();
    code.keep(HI16_ZMM);
    code.bind(&[zero]);
    let mut live = Vec::new();
    for k in (0..8).step_by(2) {
        code.test_masks(masks, k, k + 1);
        live.push(code.jnz());
    }
    let zero = code.jmp();
    code.bind(&live);
    code.keep(MASK_STATE);
    code.bind(&[zero]);
    keep_parts(code, masks, 2);
    call(code);
    give_back_zmm(code, masks, false);
}

/// [`Vectors::ZmmInUse`].
fn keep_zmm_in_use(code: &mut Code, masks: Masks, call: impl Fn(&mut Code)) {
    code.frame(mask_slot(8));
    // The parts in use.
    code.in_use();
    code.path_from_eax();
    keep_parts(code, masks, 0);
    call(code);
    // What the handlers left in use: a part that stays unused stays zero, and
    // zeroing it would mark it in use for every later hit.
    code.in_use();
    give_back_zmm(code, masks, true);
}

/// Stores the parts `r12b` names, `xmm<from>` up (the stub stored the
/// registers below whole), and clears the upper halves for the handlers.
///
/// `zmm16`-`zmm31` and the mask registers are kept apart from `zmm0`-`zmm15`,
/// so that live AVX-512 registers alone do not make the stub give back
/// `zmm0`-`zmm15` with 512-bit loads, which mark their upper halves in use.
fn keep_parts(code: &mut Code, masks: Masks, from: u8) {
    let zmm = code.branch(ZMM_HI256);
    let ymm = code.branch(AVX_STATE);
    for n in from..6 {
        code.xmm(STORE, n, slot(n, ZMM));
    }
    let mut rest = vec![code.jmp()];
    code.bind(&[ymm]);
    for n in from..16 {
        code.ymm(STORE, n, slot(n, ZMM));
    }
    rest.push(code.jmp());
    code.bind(&[zmm]);
    for n in from..16 {
        code.zmm(STORE, n, slot(n, ZMM));
    }
    code.bind(&rest);
    let unused = code.unless(HI16_ZMM);
    for n in 16..32 {
        code.zmm(STORE, n, slot(n, ZMM));
    }
    code.bind(&[unused]);
    let unused = code.unless(MASK_STATE);
    for k in 0..8 {
        code.mask(masks, STORE, k, mask_slot(k));
    }
    code.bind(&[unused]);
    code.vzeroupper();
}

/// Loads back the parts [`keep_parts`] stored. A part it did not store was
/// zero, so it is given back as zero: where `al` names it used after the
/// handlers if `checked`, every time if not.
fn give_back_zmm(code: &mut Code, masks: Masks, checked: bool) {
    let zero = code.unless(HI16_ZMM);
    for n in 16..32 {
        code.zmm(LOAD, n, slot(n, ZMM));
    }
    let mut done = vec![code.jmp()];
    code.bind(&[zero]);
    if checked {
        done.push(code.unless_used(HI16_ZMM));
    }
    code.zero_hi16();
    code.bind(&done);
    let zero = code.unless(MASK_STATE);
    for k in 0..8 {
        code.mask(masks, LOAD, k, mask_slot(k));
    }
    let mut done = vec![code.jmp()];
    code.bind(&[zero]);
    if checked {
        done.push(code.unless_used(MASK_STATE));
    }
    code.zero_masks(masks);
    code.bind(&done);
    let zmm = code.branch(ZMM_HI256);
    code.vzeroupper();
    let ymm = code.branch(AVX_STATE);
    for n in 0..6 {
        code.xmm(LOAD, n, slot(n, ZMM));
    }
    let mut end = vec![code.jmp()];
    code.bind(&[ymm]);
    for n in 0..16 {
        code.ymm(LOAD, n, slot(n, ZMM));
    }
    end.push(code.jmp());
    code.bind(&[zmm]);
    for n in 0..16 {
        code.zmm(LOAD, n, slot(n, ZMM));
    }
    code.bind(&end);
}

#[cfg(test)]
mod tests;
