//! Check shapes: the code a route's symbol must start with, made from the
//! code at the symbol on several builds and matched as any shape is.

use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};

use super::shape::{self, Step};
use crate::reflect::Image;

/// The bytes a check covers, as a `<id>.bytes` check does.
const COVER: u64 = 8;
/// The most steps a check has.
const MAX_STEPS: usize = 8;
/// The longest x86-64 instruction.
const LONGEST: u64 = 15;

/// Up to `count` instructions at `rva` in `image`'s code section.
fn code_at(image: &Image<'_>, rva: u64, count: usize) -> Vec<Instruction> {
    let Some(text) = image.section(".text").map(|s| image.range(s)) else {
        return Vec::new();
    };
    let va = image.base().saturating_add(rva);
    if !text.contains(&va) {
        return Vec::new();
    }
    let len = (text.end - va).min(LONGEST * count as u64);
    let bytes = image.read(va, len).unwrap_or_default();
    Decoder::with_ip(64, bytes, va, DecoderOptions::NONE)
        .into_iter()
        .take(count)
        .collect()
}

/// Whether the code at `rva` in `image` starts with the check shape `check`,
/// as a route's `check` row reads; false for a step that does not parse.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::check_holds;
/// use enshwick_gamedata::reflect::{Image, Layout};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let holds = check_holds(&["push rbx".to_owned()], &image, 0x1000);
/// # let _ = holds;
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn check_holds(check: &[String], image: &Image<'_>, rva: u64) -> bool {
    let Ok(steps) = check
        .iter()
        .map(|s| Step::parse(s))
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    !steps.is_empty() && shape::starts(&steps, &code_at(image, rva, steps.len()))
}

/// Makes the check shape of a symbol from its code on each build, given as
/// the image and the symbol's RVA.
///
/// The check is the fewest instructions that cover eight bytes on every
/// build, or end in a `ret` or `jmp`, each step exact where every build
/// agrees and loose where only the registers do; `None` when no such run is
/// the same on every build.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::make_check;
/// use enshwick_gamedata::reflect::{Image, Layout};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let old = std::fs::read("old/enshrouded_server.exe")?;
/// let new = std::fs::read("new/enshrouded_server.exe")?;
/// let (old, new) = (Image::parse(&old, Layout::File, None)?, Image::parse(&new, Layout::File, None)?);
/// let check = make_check(&[(&old, 0x1000), (&new, 0x1040)]);
/// # let _ = check;
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn make_check(code: &[(&Image<'_>, u64)]) -> Option<Vec<String>> {
    let runs: Vec<Vec<Instruction>> = code
        .iter()
        .map(|&(image, rva)| code_at(image, rva, MAX_STEPS))
        .collect();
    if runs.is_empty() || runs.iter().any(Vec::is_empty) {
        return None;
    }
    let mut steps = Vec::new();
    for k in 0..MAX_STEPS {
        let here: Vec<&Instruction> = runs.iter().map(|r| r.get(k)).collect::<Option<_>>()?;
        if here.iter().any(|i| i.is_invalid()) {
            return None;
        }
        let texts = [true, false].into_iter().find_map(|exact| {
            let texts: Vec<String> = here.iter().map(|i| step_text(i, exact)).collect();
            texts
                .iter()
                .all(|t| *t == texts[0])
                .then(|| texts[0].clone())
        })?;
        steps.push(texts);
        let covered = runs.iter().all(|r| r[k].next_ip() - r[0].ip() >= COVER);
        let ends = matches!(
            here[0].mnemonic(),
            Mnemonic::Ret | Mnemonic::Jmp | Mnemonic::Int3
        );
        if covered || ends {
            let holds = code
                .iter()
                .all(|&(image, rva)| check_holds(&steps, image, rva));
            return holds.then_some(steps);
        }
    }
    None
}

fn name(x: impl core::fmt::Debug) -> String {
    format!("{x:?}").to_lowercase()
}

/// One instruction as a shape step: displacements and immediates exact
/// when `exact`, else only their sign.
fn step_text(ins: &Instruction, exact: bool) -> String {
    let ops: Vec<String> = (0..ins.op_count())
        .map(|i| match ins.op_kind(i) {
            OpKind::Register => name(ins.op_register(i)),
            OpKind::Memory if ins.is_ip_rel_memory_operand() => "[rip+?]".to_owned(),
            OpKind::Memory
                if ins.memory_base() == Register::None
                    || ins.segment_prefix() != Register::None =>
            {
                "?".to_owned()
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
                format!("[{}{index}{disp}]", name(ins.memory_base()))
            }
            OpKind::Immediate8
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64
                if exact =>
            {
                format!("{:#x}", ins.immediate(i))
            }
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
    use super::{check_holds, make_check};
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
        // `mov [rsp+0x8],rbx; push rdi; sub rsp,0x20` and the same with 0x30.
        let old = image_bytes(&[0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x20]);
        let new = image_bytes(&[0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x30]);
        let old = Image::parse(&old, Layout::File, None).unwrap();
        let new = Image::parse(&new, Layout::File, None).unwrap();
        let check = make_check(&[(&old, 0x1000), (&new, 0x1000)]).unwrap();
        assert_eq!(check, ["mov [rsp+0x8],rbx", "push rdi", "sub rsp,?"]);
        assert!(check_holds(&check, &new, 0x1000));
        assert!(!check_holds(&check, &new, 0x1005));
        assert!(!check_holds(&["frobnicate".to_owned()], &new, 0x1000));
    }

    #[test]
    fn code_that_differs_in_kind_gets_no_check() {
        let old = image_bytes(&[0x53, 0x48, 0x83, 0xec, 0x20, 0x90, 0x90, 0x90, 0x90]);
        let new = image_bytes(&[0x57, 0x48, 0x83, 0xec, 0x20, 0x90, 0x90, 0x90, 0x90]);
        let old = Image::parse(&old, Layout::File, None).unwrap();
        let new = Image::parse(&new, Layout::File, None).unwrap();
        assert_eq!(make_check(&[(&old, 0x1000), (&new, 0x1000)]), None);
    }
}
