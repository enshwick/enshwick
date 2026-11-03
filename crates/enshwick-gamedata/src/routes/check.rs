//! Check shapes: the code a route's symbol must start with, made from the
//! code at the symbol on several builds and matched as any shape is, except
//! that a `jmp` step goes on at the jump's target, so a thunk's check covers
//! the code it jumps to.

use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};

use super::shape::{ShapeNames, Step};
use crate::reflect::Image;

/// The bytes a check covers past its last jump, as a `<id>.bytes` check does.
const COVER: u64 = 8;
/// The most steps a check has.
const MAX_STEPS: usize = 12;
/// The most jumps a check follows.
const MAX_JUMPS: usize = 2;
/// The longest x86-64 instruction.
const LONGEST: u64 = 15;

/// The instruction at `va` in `image`'s code section.
fn one_at(image: &Image<'_>, va: u64) -> Option<Instruction> {
    let text = image.range(image.section(".text")?);
    if !text.contains(&va) {
        return None;
    }
    let bytes = image.read(va, (text.end - va).min(LONGEST))?;
    let ins = Decoder::with_ip(64, bytes, va, DecoderOptions::NONE).decode();
    (!ins.is_invalid()).then_some(ins)
}

/// Where a check goes on after `ins`: the target of a direct `jmp`, else
/// the next instruction.
fn next_of(ins: &Instruction) -> u64 {
    if ins.mnemonic() == Mnemonic::Jmp && ins.op0_kind() == OpKind::NearBranch64 {
        ins.near_branch64()
    } else {
        ins.next_ip()
    }
}

/// Whether the code at `rva` in `image` runs as `steps` say, a `jmp` step
/// going on at its target, and the steps check eight bytes past the last
/// jump or end the function there.
pub(crate) fn runs(steps: &[Step], image: &Image<'_>, rva: u64) -> bool {
    let mut va = image.base().saturating_add(rva);
    let mut taken = None;
    let mut covered = 0;
    let mut ended = false;
    let matched = !steps.is_empty()
        && steps.iter().all(|step| {
            let Some(ins) = one_at(image, va) else {
                return false;
            };
            va = next_of(&ins);
            let jump = va != ins.next_ip();
            covered = if jump { 0 } else { covered + ins.len() as u64 };
            ended = matches!(ins.mnemonic(), Mnemonic::Ret | Mnemonic::Int3)
                || ins.mnemonic() == Mnemonic::Jmp && !jump;
            step.matches(&ins, &|_: &str| None, &mut taken)
        });
    matched && (covered >= COVER || ended)
}

fn parse(names: &ShapeNames, steps: &[String]) -> Option<Vec<Step>> {
    steps.iter().map(|s| Step::parse(s, names).ok()).collect()
}

/// Whether the code at `rva` in `image` runs as the check shape `check`
/// says, as a route's `check` row reads; false for a step that does not
/// parse.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::reflect::{Image, Layout};
/// use enshwick_gamedata::{ShapeNames, check_holds};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let holds = check_holds(&ShapeNames::new(), &["push rbx".to_owned()], &image, 0x1000);
/// # let _ = holds;
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn check_holds(names: &ShapeNames, check: &[String], image: &Image<'_>, rva: u64) -> bool {
    parse(names, check).is_some_and(|steps| runs(&steps, image, rva))
}

/// Whether a route's start shape checks the code at `rva` as a hook needs:
/// two steps or more, matching there one after the other, over eight bytes.
///
/// A step naming another entry does not match here, where no entry is known.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::reflect::{Image, Layout};
/// use enshwick_gamedata::{ShapeNames, shape_checks};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let shape = ["push rbx".to_owned(), "sub rsp,?".to_owned()];
/// let checks = shape_checks(&ShapeNames::new(), &shape, &image, 0x1000);
/// # let _ = checks;
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn shape_checks(names: &ShapeNames, shape: &[String], image: &Image<'_>, rva: u64) -> bool {
    let Some(steps) = parse(names, shape) else {
        return false;
    };
    let start = image.base().saturating_add(rva);
    let mut va = start;
    let mut taken = None;
    let matched = steps.iter().all(|step| {
        let Some(ins) = one_at(image, va) else {
            return false;
        };
        va = ins.next_ip();
        step.matches(&ins, &|_: &str| None, &mut taken)
    });
    matched && match_checks(steps.len(), start, va)
}

/// Whether a start shape's match from `start` to `end` checks the code at
/// `start` as a hook needs: [`shape_checks`] for a match.
pub(crate) fn match_checks(steps: usize, start: u64, end: u64) -> bool {
    steps >= 2 && end - start >= COVER
}

/// Makes the check shape of a symbol from its code on each build, given as
/// the image and the symbol's RVA.
///
/// The check is the fewest instructions that cover eight bytes on every
/// build past the last direct `jmp` it follows, or end the function, each
/// step exact where every build agrees and loose where only the registers
/// do; `None` when no such run is the same on every build.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::reflect::{Image, Layout};
/// use enshwick_gamedata::{ShapeNames, make_check};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let old = std::fs::read("old/enshrouded_server.exe")?;
/// let new = std::fs::read("new/enshrouded_server.exe")?;
/// let (old, new) = (Image::parse(&old, Layout::File, None)?, Image::parse(&new, Layout::File, None)?);
/// let check = make_check(&ShapeNames::new(), &[(&old, 0x1000), (&new, 0x1040)]);
/// # let _ = check;
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn make_check(names: &ShapeNames, code: &[(&Image<'_>, u64)]) -> Option<Vec<String>> {
    if code.is_empty() {
        return None;
    }
    let mut at: Vec<u64> = code
        .iter()
        .map(|&(image, rva)| image.base().saturating_add(rva))
        .collect();
    let mut covered = vec![0u64; code.len()];
    let mut jumps = 0;
    let mut steps = Vec::new();
    while steps.len() < MAX_STEPS {
        let here: Vec<Instruction> = code
            .iter()
            .zip(&at)
            .map(|(&(image, _), &va)| one_at(image, va))
            .collect::<Option<_>>()?;
        let text = [Level::Exact, Level::Loose, Level::Registers]
            .into_iter()
            .find_map(|level| {
                let texts: Vec<String> = here.iter().map(|i| step_text(i, level)).collect();
                texts
                    .iter()
                    .all(|t| *t == texts[0])
                    .then(|| texts[0].clone())
            })?;
        steps.push(text);
        let jump = here[0].mnemonic() == Mnemonic::Jmp
            && here.iter().all(|i| i.op0_kind() == OpKind::NearBranch64);
        let ends = matches!(here[0].mnemonic(), Mnemonic::Ret | Mnemonic::Int3)
            || here[0].mnemonic() == Mnemonic::Jmp && !jump;
        for ((va, done), ins) in at.iter_mut().zip(&mut covered).zip(&here) {
            *va = next_of(ins);
            *done = if jump { 0 } else { *done + ins.len() as u64 };
        }
        if jump {
            jumps += 1;
            if jumps > MAX_JUMPS {
                return None;
            }
            continue;
        }
        if ends || covered.iter().all(|&c| c >= COVER) {
            let holds = code
                .iter()
                .all(|&(image, rva)| check_holds(names, &steps, image, rva));
            return holds.then_some(steps);
        }
    }
    None
}

fn name(x: impl core::fmt::Debug) -> String {
    format!("{x:?}").to_lowercase()
}

/// How much of an instruction a check step keeps, tried in this order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    /// Registers, displacements and immediates.
    Exact,
    /// Registers and the sign of each displacement.
    Loose,
    /// The sign of each displacement and the memory operand's base, index
    /// and size; other registers may differ, but stay registers.
    Registers,
}

/// One instruction as a check step at `level`.
fn step_text(ins: &Instruction, level: Level) -> String {
    let exact = level == Level::Exact;
    let ops: Vec<String> = (0..ins.op_count())
        .map(|i| match ins.op_kind(i) {
            OpKind::Register if level == Level::Registers => "?r".to_owned(),
            OpKind::Register => name(ins.op_register(i)),
            OpKind::Memory if ins.is_ip_rel_memory_operand() => "[rip+?]".to_owned(),
            OpKind::Memory
                if ins.memory_base() == Register::None
                    || ins.segment_prefix() != Register::None =>
            {
                "?m".to_owned()
            }
            OpKind::Memory => {
                let index = if ins.memory_index() == Register::None {
                    String::new()
                } else {
                    format!("+{}*{}", name(ins.memory_index()), ins.memory_index_scale())
                };
                let disp = match (ins.memory_displacement64().cast_signed(), exact) {
                    (0, _) => String::new(),
                    (d, true) if d < 0 => format!("-{:#x}", d.unsigned_abs()),
                    (d, true) => format!("+{d:#x}"),
                    (d, false) if d < 0 => "-?".to_owned(),
                    (_, false) => "+?".to_owned(),
                };
                // Without a register operand, only the size tells a byte
                // store from a dword one.
                let registers = level != Level::Registers
                    && (0..ins.op_count()).any(|k| ins.op_kind(k) == OpKind::Register);
                let size = match ins.memory_size().size() {
                    _ if registers => "",
                    1 => "byte ",
                    2 => "word ",
                    4 => "dword ",
                    8 => "qword ",
                    16 => "xmmword ",
                    32 => "ymmword ",
                    _ => "",
                };
                format!("{size}[{}{index}{disp}]", name(ins.memory_base()))
            }
            OpKind::Immediate8
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64 => {
                if exact {
                    format!("{:#x}", ins.immediate(i))
                } else {
                    "?i".to_owned()
                }
            }
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => "?b".to_owned(),
            _ => "?".to_owned(),
        })
        .collect();
    let mnemonic = name(ins.mnemonic());
    if ops.is_empty() {
        mnemonic
    } else {
        format!("{mnemonic} {}", ops.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::{ShapeNames, check_holds, make_check};
    use crate::reflect::{Image, Layout};

    /// A PE32+ file of one `.text` section at 0x1000 holding `code`.
    fn image_bytes(code: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xccu8; 0x2000];
        bytes[..0x200].fill(0);
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&0xf0u16.to_le_bytes());
        bytes[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[0x98 + 24..0x98 + 32].copy_from_slice(&0x1_4000_0000u64.to_le_bytes());
        let header = 0x98 + 0xf0;
        bytes[header..header + 8].copy_from_slice(b".text\0\0\0");
        for (field, value) in [(8, 0x1000u32), (12, 0x1000), (16, 0x1000), (20, 0x1000)] {
            bytes[header + field..header + field + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[0x1000..0x1000 + code.len()].copy_from_slice(code);
        bytes
    }

    #[test]
    fn a_check_covers_eight_bytes_exact_where_the_builds_agree() {
        let n = ShapeNames::new();
        // `mov [rsp+0x8],rbx; push rdi; sub rsp,0x20` and the same with 0x30.
        let old = image_bytes(&[0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x20]);
        let new = image_bytes(&[0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x30]);
        let old = Image::parse(&old, Layout::File, None).unwrap();
        let new = Image::parse(&new, Layout::File, None).unwrap();
        let check = make_check(&n, &[(&old, 0x1000), (&new, 0x1000)]).unwrap();
        assert_eq!(check, ["mov [rsp+0x8],rbx", "push rdi", "sub rsp,?i"]);
        assert!(check_holds(&n, &check, &new, 0x1000));
        assert!(!check_holds(&n, &check, &new, 0x1005));
        assert!(!check_holds(&n, &["frobnicate".to_owned()], &new, 0x1000));
    }

    fn image_at(code: &[(usize, &[u8])]) -> Vec<u8> {
        let mut bytes = image_bytes(&[]);
        for (at, part) in code {
            bytes[*at..*at + part.len()].copy_from_slice(part);
        }
        bytes
    }

    #[test]
    fn a_thunk_s_check_goes_on_at_the_code_it_jumps_to() {
        let n = ShapeNames::new();
        // `jmp 0x1100` at 0x1000; `mov [rsp+0x8],rbx; push rdi; sub rsp,0x20` at 0x1100.
        let bytes = image_at(&[
            (0x1000, &[0xe9, 0xfb, 0x00, 0x00, 0x00]),
            (
                0x1100,
                &[0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x20],
            ),
        ]);
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let check = make_check(&n, &[(&image, 0x1000), (&image, 0x1000)]).unwrap();
        assert_eq!(
            check,
            ["jmp ?b", "mov [rsp+0x8],rbx", "push rdi", "sub rsp,0x20"]
        );
        assert!(check_holds(&n, &check, &image, 0x1000));
        assert!(!check_holds(&n, &["jmp ?".to_owned()], &image, 0x1000));
    }

    #[test]
    fn a_store_s_check_names_its_size() {
        let n = ShapeNames::new();
        // `mov dword [rcx+0xb8],0x2` and `mov byte [rcx+0xb8],0x2; ret`.
        let dword = image_bytes(&[0xc7, 0x81, 0xb8, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00]);
        let byte = image_bytes(&[0xc6, 0x81, 0xb8, 0x00, 0x00, 0x00, 0x02, 0xc3]);
        let dword = Image::parse(&dword, Layout::File, None).unwrap();
        let byte = Image::parse(&byte, Layout::File, None).unwrap();
        let check = make_check(&n, &[(&dword, 0x1000)]).unwrap();
        assert_eq!(check, ["mov dword [rcx+0xb8],0x2"]);
        assert!(!check_holds(&n, &check, &byte, 0x1000));
    }

    #[test]
    fn builds_that_keep_other_registers_get_a_check_over_the_rest() {
        let n = ShapeNames::new();
        // `mov rax,rsp; mov [rax+0x8],rbx; mov [rax+0x10],rsi|rbp; push rbp`.
        let old = image_bytes(&[
            0x48, 0x8b, 0xc4, 0x48, 0x89, 0x58, 0x08, 0x48, 0x89, 0x70, 0x10, 0x55,
        ]);
        let new = image_bytes(&[
            0x48, 0x8b, 0xc4, 0x48, 0x89, 0x58, 0x08, 0x48, 0x89, 0x68, 0x10, 0x55,
        ]);
        let old = Image::parse(&old, Layout::File, None).unwrap();
        let new = Image::parse(&new, Layout::File, None).unwrap();
        let check = make_check(&n, &[(&old, 0x1000), (&new, 0x1000)]).unwrap();
        assert_eq!(
            check,
            ["mov rax,rsp", "mov [rax+0x8],rbx", "mov qword [rax+?],?r"]
        );
        assert!(check_holds(&n, &check, &old, 0x1000) && check_holds(&n, &check, &new, 0x1000));
    }

    #[test]
    fn a_register_and_an_immediate_at_one_place_get_no_check() {
        let n = ShapeNames::new();
        // `mov [rax+0x10],rsi; push rbp; ...` and `mov qword [rax+0x10],0x5; push rbp; ...`.
        let register = image_bytes(&[0x48, 0x89, 0x70, 0x10, 0x55, 0x90, 0x90, 0x90, 0x90]);
        let immediate = image_bytes(&[
            0x48, 0xc7, 0x40, 0x10, 0x05, 0x00, 0x00, 0x00, 0x55, 0x90, 0x90,
        ]);
        let register = Image::parse(&register, Layout::File, None).unwrap();
        let immediate = Image::parse(&immediate, Layout::File, None).unwrap();
        assert_eq!(
            make_check(&n, &[(&register, 0x1000), (&immediate, 0x1000)]),
            None
        );
        let typed = ["mov qword [rax+?],?r".to_owned(), "push ?r".to_owned()];
        assert!(!check_holds(&n, &typed, &immediate, 0x1000));
    }

    #[test]
    fn a_branch_wildcard_takes_no_indirect_jump() {
        let n = ShapeNames::new();
        // `jmp [rip+0x10]`, then padding.
        let bytes = image_bytes(&[0xff, 0x25, 0x10, 0x00, 0x00, 0x00]);
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        assert!(!check_holds(&n, &["jmp ?b".to_owned()], &image, 0x1000));
        assert!(check_holds(&n, &["jmp [rip+?]".to_owned()], &image, 0x1000));
    }

    #[test]
    fn code_that_differs_in_kind_gets_no_check() {
        let n = ShapeNames::new();
        let old = image_bytes(&[0x53, 0x48, 0x83, 0xec, 0x20, 0x90, 0x90, 0x90, 0x90]);
        let new = image_bytes(&[0x90, 0x48, 0x83, 0xec, 0x20, 0x90, 0x90, 0x90, 0x90]);
        let old = Image::parse(&old, Layout::File, None).unwrap();
        let new = Image::parse(&new, Layout::File, None).unwrap();
        assert_eq!(make_check(&n, &[(&old, 0x1000), (&new, 0x1000)]), None);
    }
}
