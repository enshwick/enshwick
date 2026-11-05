//! Instruction shapes: a run of decoded instructions written as Intel
//! assembly with holes, matched on what the instructions do rather than on
//! their bytes, so a changed encoding, register prefix or displacement size
//! still matches.
//!
//! A step is a mnemonic and its operands, comma separated: a register
//! (`r13`); memory through a base register, an optional index register with
//! its scale and a displacement (`[rcx]`, `[rbp-*]`, `[rbp+?]`, `[rdx+0x78]`,
//! `[rcx+r10*4+?]`) or RIP-relative (`[rip+*]`); an exact immediate
//! (`0x1`); memory with its size first where the registers do not give it
//! (`dword [rcx+0xb8]`); `?` for any operand; `?r`, `?i`, `?b` and `?m` for
//! any register, immediate, branch target and memory operand; `*` for any operand whose value the route
//! takes; and `{id}` for the address of another entry, as a branch target or
//! a RIP-relative operand. A `*` or `?` displacement is not zero and has the
//! sign written.

use iced_x86::{Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register};
use rustc_hash::FxHashMap;

/// What a memory operand's displacement must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disp {
    /// No displacement.
    None,
    /// A displacement of this sign, taken when `take` is set.
    Signed { negative: bool, take: bool },
    /// This displacement.
    Exact(i64),
}

/// One operand of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Register(Register),
    Memory {
        base: Register,
        /// The index register and its scale; `Register::None` for none.
        index: (Register, u32),
        disp: Disp,
        /// The bytes the operand reads or writes, when the step says.
        size: Option<usize>,
    },
    Immediate(u64),
    Any {
        take: bool,
    },
    /// Any operand of one kind.
    Wild(Wild),
    Entry(String),
}

/// The kind a typed wildcard operand must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wild {
    /// `?r`: a register.
    Register,
    /// `?i`: an immediate.
    Immediate,
    /// `?b`: a direct branch target.
    Branch,
    /// `?m`: memory.
    Memory,
}

impl Wild {
    const TOKENS: [(&str, Self); 4] = [
        ("?r", Self::Register),
        ("?i", Self::Immediate),
        ("?b", Self::Branch),
        ("?m", Self::Memory),
    ];

    fn holds(self, kind: OpKind) -> bool {
        match self {
            Self::Register => kind == OpKind::Register,
            Self::Immediate => is_immediate(kind),
            Self::Branch => matches!(
                kind,
                OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
            ),
            Self::Memory => kind == OpKind::Memory,
        }
    }
}

fn is_immediate(kind: OpKind) -> bool {
    matches!(
        kind,
        OpKind::Immediate8
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64
    )
}

/// One instruction of a shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    mnemonic: Mnemonic,
    ops: Vec<Op>,
}

/// Why a step does not parse.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub(crate) struct ShapeError(String);

/// The names shape steps use for instructions and registers, made once and
/// passed to every function that reads steps, since making them takes a
/// millisecond.
///
/// # Examples
///
/// ```
/// let names = enshwick_gamedata::ShapeNames::new();
/// # let _ = names;
/// ```
#[derive(Debug)]
pub struct ShapeNames {
    mnemonics: FxHashMap<String, Mnemonic>,
    registers: FxHashMap<String, Register>,
}

impl Default for ShapeNames {
    fn default() -> Self {
        Self::new()
    }
}

impl ShapeNames {
    /// Makes the names of every mnemonic and register, lowercase.
    ///
    /// # Examples
    ///
    /// See [`ShapeNames`].
    #[must_use]
    pub fn new() -> Self {
        let name = |x: &dyn core::fmt::Debug| format!("{x:?}").to_lowercase();
        Self {
            mnemonics: Mnemonic::values().map(|m| (name(&m), m)).collect(),
            registers: Register::values()
                .filter(|&r| r != Register::None)
                .map(|r| (name(&r), r))
                .collect(),
        }
    }

    fn register(&self, text: &str) -> Result<Register, ShapeError> {
        self.registers
            .get(&text.to_lowercase())
            .copied()
            .ok_or_else(|| ShapeError(format!("{text:?} is no register")))
    }
}

/// The size words a memory operand may start with, and their bytes.
const SIZES: [(&str, usize); 6] = [
    ("byte ", 1),
    ("word ", 2),
    ("dword ", 4),
    ("qword ", 8),
    ("xmmword ", 16),
    ("ymmword ", 32),
];

impl Step {
    /// Reads one step, such as `lea rdx,[rbp-*]`.
    pub(crate) fn parse(text: &str, names: &ShapeNames) -> Result<Self, ShapeError> {
        let text = text.trim();
        let (name, ops) = text.split_once(' ').unwrap_or((text, ""));
        let mnemonic = names
            .mnemonics
            .get(&name.to_lowercase())
            .copied()
            .ok_or_else(|| ShapeError(format!("{name:?} is no mnemonic")))?;
        let ops = ops
            .split(',')
            .map(str::trim)
            .filter(|op| !op.is_empty())
            .map(|op| operand(op, names))
            .collect::<Result<_, _>>()?;
        Ok(Self { mnemonic, ops })
    }

    /// How many values the step takes.
    pub(crate) fn takes(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    Op::Any { take: true }
                        | Op::Memory {
                            disp: Disp::Signed { take: true, .. },
                            ..
                        }
                )
            })
            .count()
    }

    /// The ids of the entries the step names.
    pub(crate) fn entries(&self) -> impl Iterator<Item = &str> {
        self.ops.iter().filter_map(|op| match op {
            Op::Entry(id) => Some(id.as_str()),
            _ => None,
        })
    }

    /// Whether the step pins an operand to one value: an exact immediate,
    /// an exact displacement or another entry's address.
    pub(crate) fn is_exact(&self) -> bool {
        self.ops.iter().any(|op| {
            matches!(
                op,
                Op::Immediate(_)
                    | Op::Entry(_)
                    | Op::Memory {
                        disp: Disp::Exact(_),
                        ..
                    }
            )
        })
    }

    /// Whether an operand of the step holds `value` in its own encoding, so
    /// a match confirms it: memory without a displacement holds 0, else an
    /// index scale or immediate equal to it. A `nop`'s operand is padding
    /// and spells nothing.
    pub(crate) fn spells(&self, value: u64) -> bool {
        self.mnemonic != Mnemonic::Nop
            && self.ops.iter().any(|op| match op {
                Op::Memory {
                    base, index, disp, ..
                } => {
                    value == 0 && *disp == Disp::None && *base != Register::RIP
                        || index.0 != Register::None && u64::from(index.1) == value
                }
                Op::Immediate(v) => *v == value,
                _ => false,
            })
    }

    /// Whether `ins` is this step, with `entry` giving the address of a named
    /// entry; the taken value, if the step takes one, goes to `taken`.
    pub(crate) fn matches(
        &self,
        ins: &Instruction,
        entry: &impl Fn(&str) -> Option<u64>,
        taken: &mut Option<u64>,
    ) -> bool {
        if ins.mnemonic() != self.mnemonic || ins.op_count() as usize != self.ops.len() {
            return false;
        }
        self.ops.iter().enumerate().all(|(i, op)| {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            let kind = ins.op_kind(i);
            match op {
                Op::Register(want) => kind == OpKind::Register && ins.op_register(i) == *want,
                Op::Memory {
                    base,
                    index,
                    disp,
                    size,
                } => {
                    kind == OpKind::Memory
                        && size.is_none_or(|bytes| ins.memory_size().size() == bytes)
                        && ins.memory_base() == *base
                        && ins.memory_index() == index.0
                        && (index.0 == Register::None || ins.memory_index_scale() == index.1)
                        && memory(ins, *base, *disp, taken)
                }
                Op::Immediate(want) => {
                    kind != OpKind::Memory
                        && kind != OpKind::NearBranch64
                        && value(ins, i, kind) == Some(*want)
                }
                Op::Any { take } => !take || value(ins, i, kind).is_some_and(|v| set(taken, v)),
                Op::Wild(wild) => wild.holds(kind),
                Op::Entry(id) => entry(id).is_some_and(|want| value(ins, i, kind) == Some(want)),
            }
        })
    }
}

fn set(taken: &mut Option<u64>, value: u64) -> bool {
    *taken = Some(value);
    true
}

/// Whether the memory operand of `ins` through `base` has the displacement
/// `disp` asks for; a RIP-relative one takes its target address.
fn memory(ins: &Instruction, base: Register, disp: Disp, taken: &mut Option<u64>) -> bool {
    if base == Register::RIP {
        return match disp {
            Disp::Signed { take, .. } => !take || set(taken, ins.ip_rel_memory_address()),
            Disp::None | Disp::Exact(_) => false,
        };
    }
    let signed = ins.memory_displacement64().cast_signed();
    match disp {
        Disp::None => signed == 0,
        Disp::Exact(want) => signed == want,
        Disp::Signed { negative, take } => {
            (signed < 0) == negative && signed != 0 && (!take || set(taken, signed.unsigned_abs()))
        }
    }
}

/// The value an operand carries: a branch target, an immediate, or a
/// RIP-relative address.
fn value(ins: &Instruction, i: u32, kind: OpKind) -> Option<u64> {
    match kind {
        OpKind::NearBranch64 => Some(ins.near_branch64()),
        OpKind::Immediate8
        | OpKind::Immediate16
        | OpKind::Immediate32
        | OpKind::Immediate64
        | OpKind::Immediate8to16
        | OpKind::Immediate8to32
        | OpKind::Immediate8to64
        | OpKind::Immediate32to64 => Some(ins.immediate(i)),
        OpKind::Memory if ins.is_ip_rel_memory_operand() => Some(ins.ip_rel_memory_address()),
        _ => None,
    }
}

fn hex(text: &str) -> Option<u64> {
    u64::from_str_radix(text.strip_prefix("0x")?, 16).ok()
}

fn operand(text: &str, names: &ShapeNames) -> Result<Op, ShapeError> {
    match text {
        "?" => return Ok(Op::Any { take: false }),
        "*" => return Ok(Op::Any { take: true }),
        _ => {}
    }
    if let Some(&(_, wild)) = Wild::TOKENS.iter().find(|(token, _)| *token == text) {
        return Ok(Op::Wild(wild));
    }
    if let Some(id) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
        return Ok(Op::Entry(id.to_owned()));
    }
    if let Some(value) = hex(text) {
        return Ok(Op::Immediate(value));
    }
    let (size, text) = SIZES
        .iter()
        .find_map(|&(word, bytes)| Some((Some(bytes), text.strip_prefix(word)?)))
        .unwrap_or((None, text));
    let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) else {
        if size.is_some() {
            return Err(ShapeError(format!("{text:?}: a size goes with memory")));
        }
        return names.register(text).map(Op::Register);
    };
    memory_operand(inner, size, names)
}

/// Reads the inside of a memory operand: `base`, then `+index*scale`, then
/// a signed `*`, `?` or hexadecimal displacement, each but the base optional.
fn memory_operand(inner: &str, size: Option<usize>, names: &ShapeNames) -> Result<Op, ShapeError> {
    let cut = |text: &str| text.find(['+', '-']).unwrap_or(text.len());
    let at = cut(inner);
    let base = names.register(&inner[..at])?;
    let mut rest = &inner[at..];
    let mut index = (Register::None, 1);
    if let Some(after) = rest.strip_prefix('+')
        && let Some((name, tail)) = after[..cut(after)].split_once('*')
        && !name.is_empty()
    {
        let scale = tail
            .parse()
            .ok()
            .filter(|s| [1, 2, 4, 8].contains(s))
            .ok_or_else(|| ShapeError(format!("{tail:?}: a scale is 1, 2, 4 or 8")))?;
        index = (names.register(name)?, scale);
        rest = &after[cut(after)..];
    }
    let disp = match rest.split_at_checked(1) {
        None => Disp::None,
        Some((sign, value)) => {
            let negative = sign == "-";
            match value {
                "*" => Disp::Signed {
                    negative,
                    take: true,
                },
                "?" => Disp::Signed {
                    negative,
                    take: false,
                },
                other => {
                    let exact = hex(other)
                        .and_then(|v| i64::try_from(v).ok())
                        .filter(|&v| v != 0)
                        .ok_or_else(|| {
                            ShapeError(format!(
                                "{other:?}: a displacement is `*`, `?` or a hexadecimal number"
                            ))
                        })?;
                    Disp::Exact(if negative { -exact } else { exact })
                }
            }
        }
    };
    if base == Register::RIP && !matches!(disp, Disp::Signed { .. }) {
        return Err(ShapeError(
            "a RIP-relative operand is `[rip+*]` or `[rip+?]`".to_owned(),
        ));
    }
    if base == Register::RIP && index.0 != Register::None {
        return Err(ShapeError("a RIP-relative operand has no index".to_owned()));
    }
    Ok(Op::Memory {
        base,
        index,
        disp,
        size,
    })
}

/// Where a shape matched: the first instruction's address, the address after
/// the last, and the value it took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) taken: Option<u64>,
}

/// Every run of `instructions` that is `steps`, in order.
pub(crate) fn matches<'s>(
    steps: &'s [Step],
    instructions: &'s [Instruction],
    entry: &'s impl Fn(&str) -> Option<u64>,
) -> impl Iterator<Item = Match> + 's {
    instructions.windows(steps.len()).filter_map(move |run| {
        let mut taken = None;
        steps
            .iter()
            .zip(run)
            .all(|(step, ins)| step.matches(ins, entry, &mut taken))
            .then(|| Match {
                start: run[0].ip(),
                end: run[run.len() - 1].next_ip(),
                taken,
            })
    })
}

const READS: [OpAccess; 4] = [
    OpAccess::Read,
    OpAccess::ReadWrite,
    OpAccess::CondRead,
    OpAccess::ReadCondWrite,
];

/// The factor `run` multiplies one index by: the register the first
/// instruction reads stands for the index `i`, each register holds `k · i`
/// plus something that does not depend on `i`, and the answer is `k` of the
/// last instruction's memory address, else of the register it writes.
///
/// `lea`, `mov`, `movzx`, `movsx`, `movsxd`, `add`, `sub`, `shl` and `sal`
/// by an immediate, and `imul` by an immediate fold; a value read from
/// memory or an immediate holds no `i`; an instruction that only reads
/// registers changes nothing, and one that only writes them clears them.
/// Any other instruction that reads a register holding `i` and writes a
/// register, and a call, leaves no answer, and so do a first instruction reading
/// no register or two, and a factor below 2, which multiplies nothing.
pub(crate) fn scale(run: &[Instruction]) -> Option<u64> {
    let last = run.len().checked_sub(1)?;
    let mut info = InstructionInfoFactory::new();
    let mut index = None;
    for used in info.info(&run[0]).used_registers() {
        let register = used.register().full_register();
        if READS.contains(&used.access())
            && register != Register::RIP
            && index.replace(register).is_some_and(|r| r != register)
        {
            return None;
        }
    }
    let mut factors: FxHashMap<Register, i64> = FxHashMap::default();
    factors.insert(index?, 1);
    let mut answer = None;
    for (at, ins) in run.iter().enumerate() {
        let memory = (0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory);
        if at == last && memory {
            answer = Some(address_factor(ins, &factors));
        }
        fold(ins, &mut factors, &mut info)?;
        if at == last && !memory && ins.op_count() > 0 && ins.op0_kind() == OpKind::Register {
            answer = Some(factor(&factors, ins.op0_register()));
        }
    }
    answer
        .and_then(|k| u64::try_from(k).ok())
        .filter(|&k| k >= 2)
}

/// The factor register `register` holds, 0 for one that holds no index.
fn factor(factors: &FxHashMap<Register, i64>, register: Register) -> i64 {
    factors.get(&register.full_register()).copied().unwrap_or(0)
}

/// The factor of the address of `ins`'s memory operand: its base's and its
/// scaled index's; RIP holds none.
fn address_factor(ins: &Instruction, factors: &FxHashMap<Register, i64>) -> i64 {
    let scale = i64::from(ins.memory_index_scale());
    factor(factors, ins.memory_base())
        .saturating_add(factor(factors, ins.memory_index()).saturating_mul(scale))
}

/// Applies `ins` to `factors`; `None` for an instruction it cannot fold.
fn fold(
    ins: &Instruction,
    factors: &mut FxHashMap<Register, i64>,
    info: &mut InstructionInfoFactory,
) -> Option<()> {
    let operand = |i: u32| match ins.op_kind(i) {
        OpKind::Register => factor(factors, ins.op_register(i)),
        _ => 0,
    };
    let to_register = ins.op_count() > 0 && ins.op0_kind() == OpKind::Register;
    let folded = match ins.mnemonic() {
        _ if !to_register => None,
        Mnemonic::Lea => Some(address_factor(ins, factors)),
        Mnemonic::Mov | Mnemonic::Movzx | Mnemonic::Movsx | Mnemonic::Movsxd => Some(operand(1)),
        Mnemonic::Add => operand(0).checked_add(operand(1)),
        Mnemonic::Sub => operand(0).checked_sub(operand(1)),
        Mnemonic::Shl | Mnemonic::Sal if is_immediate(ins.op1_kind()) => {
            let by = u32::try_from(ins.immediate(1)).ok()?;
            operand(0).checked_mul(1i64.checked_shl(by)?)
        }
        Mnemonic::Imul if ins.op_count() == 3 => {
            operand(1).checked_mul(ins.immediate(2).cast_signed())
        }
        _ => None,
    };
    if let Some(k) = folded {
        let register = ins.op0_register().full_register();
        if k == 0 {
            factors.remove(&register);
        } else {
            factors.insert(register, k);
        }
        return Some(());
    }
    let used = info.info(ins).used_registers();
    let holding = |r: Register| factors.contains_key(&r.full_register());
    let reads = used
        .iter()
        .any(|u| holding(u.register()) && READS.contains(&u.access()));
    let written: Vec<Register> = used
        .iter()
        .filter(|u| {
            !matches!(
                u.access(),
                OpAccess::Read | OpAccess::CondRead | OpAccess::None
            )
        })
        .map(|u| u.register().full_register())
        .collect();
    if ins.mnemonic() == Mnemonic::Call || reads && !written.is_empty() {
        return None;
    }
    for register in written {
        factors.remove(&register);
    }
    Some(())
}
