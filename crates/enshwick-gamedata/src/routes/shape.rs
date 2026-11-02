//! Instruction shapes: a run of decoded instructions written as Intel
//! assembly with holes, matched on what the instructions do rather than on
//! their bytes, so a changed encoding, register prefix or displacement size
//! still matches.
//!
//! A step is a mnemonic and its operands, comma separated: a register
//! (`r13`), memory through a register with a displacement (`[rbp-*]`,
//! `[rbp+?]`, `[rcx]`) or RIP-relative (`[rip+*]`), `?` for any operand, `*`
//! for any operand whose value the route takes, and `{id}` for the address
//! of another entry, as a branch target or a RIP-relative operand.

use iced_x86::{Instruction, Mnemonic, OpKind, Register};

/// What a memory operand's displacement must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disp {
    /// No displacement.
    None,
    /// A displacement of this sign, taken when `take` is set.
    Signed { negative: bool, take: bool },
}

/// One operand of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Register(Register),
    Memory { base: Register, disp: Disp },
    Any { take: bool },
    Entry(String),
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

impl Step {
    /// Reads one step, such as `lea rdx,[rbp-*]`.
    pub(crate) fn parse(text: &str) -> Result<Self, ShapeError> {
        let text = text.trim();
        let (name, ops) = text.split_once(' ').unwrap_or((text, ""));
        let mnemonic = Mnemonic::values()
            .find(|m| format!("{m:?}").eq_ignore_ascii_case(name))
            .ok_or_else(|| ShapeError(format!("{name:?} is no mnemonic")))?;
        let ops = ops
            .split(',')
            .map(str::trim)
            .filter(|op| !op.is_empty())
            .map(operand)
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
                Op::Memory { base, disp } => {
                    kind == OpKind::Memory
                        && ins.memory_base() == *base
                        && ins.memory_index() == Register::None
                        && memory(ins, *base, *disp, taken)
                }
                Op::Any { take } => !take || value(ins, i, kind).is_some_and(|v| set(taken, v)),
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
            Disp::None => false,
        };
    }
    let signed = ins.memory_displacement64().cast_signed();
    match disp {
        Disp::None => signed == 0,
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

fn register(name: &str) -> Result<Register, ShapeError> {
    Register::values()
        .find(|r| *r != Register::None && format!("{r:?}").eq_ignore_ascii_case(name))
        .ok_or_else(|| ShapeError(format!("{name:?} is no register")))
}

fn operand(text: &str) -> Result<Op, ShapeError> {
    match text {
        "?" => return Ok(Op::Any { take: false }),
        "*" => return Ok(Op::Any { take: true }),
        _ => {}
    }
    if let Some(id) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
        return Ok(Op::Entry(id.to_owned()));
    }
    let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) else {
        return register(text).map(Op::Register);
    };
    let (base, disp) = match inner.find(['+', '-']) {
        None => (inner, Disp::None),
        Some(at) => {
            let take = match &inner[at + 1..] {
                "*" => true,
                "?" => false,
                other => {
                    return Err(ShapeError(format!(
                        "{other:?}: a displacement is `*` or `?`"
                    )));
                }
            };
            let negative = inner.as_bytes()[at] == b'-';
            (&inner[..at], Disp::Signed { negative, take })
        }
    };
    let base = register(base)?;
    if base == Register::RIP && disp == Disp::None {
        return Err(ShapeError(
            "a RIP-relative operand is `[rip+*]` or `[rip+?]`".to_owned(),
        ));
    }
    Ok(Op::Memory { base, disp })
}

/// Where a shape matched: the first instruction's address, the address after
/// the last, and the value it took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) taken: Option<u64>,
}

/// The first run of `instructions` that is `steps`, in order.
pub(crate) fn find(
    steps: &[Step],
    instructions: &[Instruction],
    entry: &impl Fn(&str) -> Option<u64>,
) -> Option<Match> {
    instructions.windows(steps.len()).find_map(|run| {
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
