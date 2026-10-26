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
        assert_eq!(hooked(0), 0, "{name}, the second hit");
    }
}

/// Holds `value` in `k1` and `zmm17` across the marked `nop`, the upper
/// halves of `zmm0`-`zmm15` clear; then returns whether those are in use,
/// together with the bits that changed in `k1` and in `xmm17`.
#[unsafe(naked)]
extern "system" fn upper_in_use_beside_avx512(value: u32) -> u32 {
    core::arch::naked_asm!(
        "mov r8d, ecx",
        "kmovw k1, ecx",
        "vpbroadcastd zmm17, ecx",
        "vzeroupper",
        ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
        "mov ecx, 1",
        "xgetbv",
        "and eax, 4",
        "kmovw edx, k1",
        "xor edx, r8d",
        "or eax, edx",
        "vmovd edx, xmm17",
        "xor edx, r8d",
        "or eax, edx",
        "ret",
        ".fill 64, 1, 0xcc",
    )
}

extern "system" fn fill_upper_halves_and_avx512(_registers: *mut Registers, _context: usize) {
    // SAFETY: the assembly writes `xmm4`, declared as an output, its upper
    // half, `zmm17` and `k1`, which no code of this crate keeps live; it
    // touches no memory.
    unsafe {
        core::arch::asm!(
            "vcmptrueps ymm4, ymm4, ymm4",
            "vpternlogd zmm17, zmm17, zmm17, 0xff",
            "kxnorw k1, k1, k1",
            out("xmm4") _,
        );
    }
}

#[test]
fn clear_upper_halves_stay_clear_beside_live_avx512_registers_hit_after_hit() {
    if !xgetbv_in_use() {
        return;
    }
    let handler = (fill_upper_halves_and_avx512 as *const ()).addr();
    for (name, vectors) in avx_kinds() {
        if !matches!(
            vectors,
            Vectors::ZmmByValue { .. } | Vectors::ZmmInUse { .. }
        ) {
            continue;
        }
        let hooked = hook_mark(upper_in_use_beside_avx512, handler, vectors);
        for hit in 0..3 {
            assert_eq!(hooked(0x5a50), 0, "{name}, hit {hit}");
        }
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
    zeroing.zero_hi16();
    // vpxord of xmm16 and xmm27.
    code.emit(&zeroing.bytes[..6]);
    code.emit(&zeroing.bytes[11 * 6..12 * 6]);
    let mut quads = Code::default();
    quads.zero_masks(Masks::Quad);
    // kxorq of k5.
    code.emit(&quads.bytes[5 * 5..6 * 5]);
    let mut words = Code::default();
    words.zero_masks(Masks::Word);
    // kxorw of k0.
    code.emit(&words.bytes[..4]);
    code.emit(&[0xcc]);
    let start = (encodings as *const ()).addr();
    // SAFETY: `encodings` is a naked function of this module whose code
    // is the instructions `code` encodes, so `code.bytes.len()` bytes of
    // mapped code start at `start`.
    let assembled = unsafe { core::slice::from_raw_parts(start as *const u8, code.bytes.len()) };
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

fn link(owner: ModIndex, handler: MidHookFn, priority: i32) -> MidLink {
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
    assert_eq!(
        mids.add(0x10, link(ModIndex::new(1), add_hundred, 0)),
        Status::OK
    );
    assert_eq!(
        mids.add(0x10, link(ModIndex::new(0), panicking, -1)),
        Status::OK
    );
    let off: fn(ModIndex) -> bool =
        |owner| owner == ModIndex::new(0) && FAULTED.load(Ordering::Relaxed);
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
        ..link(ModIndex::new(0), add_hundred, 0)
    };
    assert_eq!(mids.add(0x10, exclusive), Status::OK);
    assert_eq!(
        mids.add(0x10, link(ModIndex::new(1), add_hundred, 0)),
        Status::REFUSED
    );
    assert_eq!(
        mids.add(0x20, link(ModIndex::new(1), add_hundred, 0)),
        Status::OK
    );
    assert_eq!(
        mids.add(0x20, link(ModIndex::new(1), add_hundred, 0)),
        Status::REFUSED
    );
    assert_eq!(
        mids.add(0x20, link(ModIndex::new(2), add_hundred, 0)),
        Status::OK
    );
    assert_eq!(
        mids.add(0, link(ModIndex::new(3), add_hundred, 0)),
        Status::BAD_ARGS
    );
    assert_eq!(mids.owners(0x20), [ModIndex::new(1), ModIndex::new(2)]);
    mids.seal();
    assert_eq!(
        mids.add(0x30, link(ModIndex::new(3), add_hundred, 0)),
        Status::TOO_LATE
    );
    assert_eq!(mids.len(), 2);
}

// TEMPORARY PROBE: records the state in use and the stub's path.

type Probe = extern "system" fn(*mut u64) -> u32;

/// rcx: a buffer. [0] state in use at the mark, [1] k1..k7 ORed (low 16
/// bits), [2] low qword of xmm16 | xmm31, [4] state in use after the mark;
/// [7] nonzero: zero zmm16-31 and the masks first.
#[unsafe(naked)]
extern "system" fn probe_upper(buffer: *mut u64) -> u32 {
    core::arch::naked_asm!(
        "mov r8, rcx",
        "vzeroupper",
        "cmp qword ptr [r8 + 56], 0",
        "je 2f",
        "vpxord xmm16, xmm16, xmm16",
        "vpxord xmm17, xmm17, xmm17",
        "vpxord xmm18, xmm18, xmm18",
        "vpxord xmm19, xmm19, xmm19",
        "vpxord xmm20, xmm20, xmm20",
        "vpxord xmm21, xmm21, xmm21",
        "vpxord xmm22, xmm22, xmm22",
        "vpxord xmm23, xmm23, xmm23",
        "vpxord xmm24, xmm24, xmm24",
        "vpxord xmm25, xmm25, xmm25",
        "vpxord xmm26, xmm26, xmm26",
        "vpxord xmm27, xmm27, xmm27",
        "vpxord xmm28, xmm28, xmm28",
        "vpxord xmm29, xmm29, xmm29",
        "vpxord xmm30, xmm30, xmm30",
        "vpxord xmm31, xmm31, xmm31",
        "kxorw k0, k0, k0",
        "kxorw k1, k1, k1",
        "kxorw k2, k2, k2",
        "kxorw k3, k3, k3",
        "kxorw k4, k4, k4",
        "kxorw k5, k5, k5",
        "kxorw k6, k6, k6",
        "kxorw k7, k7, k7",
        "2:",
        "kmovw eax, k1",
        "kmovw edx, k2",
        "or eax, edx",
        "kmovw edx, k3",
        "or eax, edx",
        "kmovw edx, k4",
        "or eax, edx",
        "kmovw edx, k5",
        "or eax, edx",
        "kmovw edx, k6",
        "or eax, edx",
        "kmovw edx, k7",
        "or eax, edx",
        "mov [r8 + 8], rax",
        "vmovq rax, xmm16",
        "vmovq rdx, xmm31",
        "or rax, rdx",
        "mov [r8 + 16], rax",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8], rax",
        "mov rcx, r8",
        ".byte 0x0f, 0x1f, 0x44, 0x00, 0x00",
        "mov r8, rcx",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 32], rax",
        "and eax, 4",
        "ret",
        ".fill 64, 1, 0xcc",
    )
}

/// Records `r12` (the stub's path) at [5] and the state in use at [6],
/// then dirties the upper half of `ymm4`.
#[unsafe(naked)]
extern "system" fn probe_handler(_registers: *mut Registers, _context: usize) {
    core::arch::naked_asm!(
        "mov r8, [rcx + 8]",
        "mov [r8 + 40], r12",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 48], rax",
        "vcmptrueps ymm4, ymm4, ymm4",
        "ret",
    )
}

/// The state in use after each step of `steps`, into rcx[0..12].
#[unsafe(naked)]
extern "system" fn probe_cpu(buffer: *mut u64) {
    core::arch::naked_asm!(
        "mov r8, rcx",
        "vzeroupper",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8], rax",
        "vpxord xmm16, xmm16, xmm16",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 8], rax",
        "kxorw k1, k1, k1",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 16], rax",
        "vpxord zmm0, zmm0, zmm0",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 24], rax",
        "vzeroupper",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 32], rax",
        "vmovdqu64 zmm0, [r8 + 128]",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 40], rax",
        "vzeroupper",
        "vmovdqu64 zmm17, [r8 + 128]",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 48], rax",
        "vmovdqu ymm0, [r8 + 128]",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 56], rax",
        "vzeroupper",
        "vpternlogd zmm0, zmm0, zmm0, 0xff",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 64], rax",
        "vzeroupper",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 72], rax",
        "vpternlogd zmm16, zmm16, zmm16, 0xff",
        "vpxord xmm16, xmm16, xmm16",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 80], rax",
        "kxnorw k1, k1, k1",
        "kxorw k1, k1, k1",
        "mov ecx, 1",
        "xgetbv",
        "mov [r8 + 88], rax",
        "ret",
    )
}

#[test]
fn clear_probe_records_the_state_in_use() {
    if masks().is_none() {
        return;
    }
    let v = core::arch::x86_64::__cpuid(0);
    let mut name = [0u8; 12];
    name[..4].copy_from_slice(&v.ebx.to_le_bytes());
    name[4..8].copy_from_slice(&v.edx.to_le_bytes());
    name[8..].copy_from_slice(&v.ecx.to_le_bytes());
    println!(
        "PROBE vendor {} chosen {:?}",
        String::from_utf8_lossy(&name),
        Vectors::current()
    );
    let handler = (probe_handler as *const ()).addr();
    for zeroed in [false, true] {
        for (name, vectors) in avx_kinds() {
            let start = (probe_upper as *const ()).addr();
            // SAFETY: probe only; 512 bytes of mapped code from the text.
            let code = unsafe { core::slice::from_raw_parts(start as *const u8, 512) };
            let target = function(code);
            let at = code.windows(MARK.len()).position(|w| w == MARK).unwrap();
            install_with(target + at, handler, 0, vectors).unwrap();
            // SAFETY: the copy of `probe_upper` takes a buffer pointer.
            let hooked = unsafe { core::mem::transmute::<usize, Probe>(target) };
            for hit in 0..3 {
                let mut b = [0u64; 8];
                b[7] = u64::from(zeroed);
                let r = hooked(b.as_mut_ptr());
                println!(
                    "PROBE zeroed={zeroed} {name} hit {hit}: in use at mark {:#x}, masks {:#x}, xmm16|31 {:#x}, path r12 {:#x}, in handler {:#x}, after {:#x}, result {r}",
                    b[0], b[1], b[2], b[5], b[6], b[4]
                );
            }
        }
    }
    let mut b = [0u64; 32];
    probe_cpu(b.as_mut_ptr());
    let steps = [
        "vzeroupper",
        "vpxord xmm16",
        "kxorw k1",
        "vpxord zmm0 (zero)",
        "vzeroupper",
        "vmovdqu64 zmm0 <- zeros",
        "vzeroupper; vmovdqu64 zmm17 <- zeros",
        "vmovdqu ymm0 <- zeros",
        "vzeroupper; vpternlogd zmm0 ones",
        "vzeroupper",
        "zmm16 ones then vpxord xmm16",
        "k1 ones then kxorw k1",
    ];
    for (i, step) in steps.iter().enumerate() {
        println!("PROBE cpu after {step}: {:#x}", b[i]);
    }
}
